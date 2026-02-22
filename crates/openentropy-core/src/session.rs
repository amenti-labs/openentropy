//! Session recording for entropy collection research.
//!
//! Records timestamped entropy samples from one or more sources, storing raw
//! bytes, CSV metrics, and session metadata. Designed for offline analysis of
//! how entropy sources behave under different conditions.
//!
//! # Storage Format
//!
//! Each session is a directory containing:
//! - `session.json` — metadata (sources, timing, machine info, tags)
//! - `samples.csv` — per-sample metrics (raw + conditioned entropy stats)
//! - `raw.bin` — concatenated raw bytes
//! - `raw_index.csv` — byte offset index into raw.bin
//! - `conditioned.bin` — concatenated conditioned bytes
//! - `conditioned_index.csv` — byte offset index into conditioned.bin

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::analysis;
use crate::conditioning::{ConditioningMode, quick_min_entropy, quick_shannon};
#[cfg(test)]
use crate::telemetry::{TelemetryMetric, TelemetryMetricDelta};
use crate::telemetry::{
    TelemetrySnapshot, TelemetryWindowReport, collect_telemetry_snapshot, collect_telemetry_window,
};

// ---------------------------------------------------------------------------
// Machine info
// ---------------------------------------------------------------------------

/// Machine information captured at session start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineInfo {
    pub os: String,
    pub arch: String,
    pub chip: String,
    pub cores: usize,
}

/// Detect machine information (best-effort).
pub fn detect_machine_info() -> MachineInfo {
    let os = format!(
        "{} {}",
        std::env::consts::OS,
        os_version().unwrap_or_default()
    );
    let arch = std::env::consts::ARCH.to_string();
    let chip = detect_chip().unwrap_or_else(|| "unknown".to_string());
    let cores = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);

    MachineInfo {
        os,
        arch,
        chip,
        cores,
    }
}

/// Get OS version string (best-effort).
fn os_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                    l.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
            })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Detect chip/CPU name (best-effort).
fn detect_chip() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Per-source analysis summary (embedded in session.json)
// ---------------------------------------------------------------------------

/// Compact analysis summary for a single source, embedded in session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSourceAnalysis {
    pub autocorrelation_max: f64,
    pub autocorrelation_violations: usize,
    pub spectral_flatness: f64,
    pub spectral_dominant_freq: f64,
    pub bit_bias_max: f64,
    pub bit_bias_has_significant: bool,
    pub distribution_ks_p: f64,
    pub distribution_mean: f64,
    pub distribution_std: f64,
    pub stationarity_f_stat: f64,
    pub stationarity_is_stationary: bool,
    pub runs_longest: usize,
    pub runs_total: usize,
}

impl SessionSourceAnalysis {
    /// Build a compact summary from a full `SourceAnalysis`.
    fn from_full(sa: &analysis::SourceAnalysis) -> Self {
        Self {
            autocorrelation_max: sa.autocorrelation.max_abs_correlation,
            autocorrelation_violations: sa.autocorrelation.violations,
            spectral_flatness: sa.spectral.flatness,
            spectral_dominant_freq: sa.spectral.dominant_frequency,
            bit_bias_max: sa.bit_bias.overall_bias,
            bit_bias_has_significant: sa.bit_bias.has_significant_bias,
            distribution_ks_p: sa.distribution.ks_p_value,
            distribution_mean: sa.distribution.mean,
            distribution_std: sa.distribution.std_dev,
            stationarity_f_stat: sa.stationarity.f_statistic,
            stationarity_is_stationary: sa.stationarity.is_stationary,
            runs_longest: sa.runs.longest_run,
            runs_total: sa.runs.total_runs,
        }
    }
}

// ---------------------------------------------------------------------------
// Analysis buffer (retains last N bytes per source for end-of-session analysis)
// ---------------------------------------------------------------------------

/// Circular buffer that retains the last `capacity` bytes per source.
struct AnalysisBuffer {
    data: HashMap<String, VecDeque<u8>>,
    capacity: usize,
}

impl AnalysisBuffer {
    fn new(sources: &[String], capacity: usize) -> Self {
        let data = sources
            .iter()
            .map(|s| (s.clone(), VecDeque::with_capacity(capacity)))
            .collect();
        Self { data, capacity }
    }

    fn push(&mut self, source: &str, bytes: &[u8]) {
        if self.capacity == 0 || bytes.is_empty() {
            return;
        }

        let buf = self
            .data
            .entry(source.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.capacity));

        if bytes.len() >= self.capacity {
            buf.clear();
            buf.extend(bytes[bytes.len() - self.capacity..].iter().copied());
            return;
        }

        let overflow = buf.len() + bytes.len();
        if overflow > self.capacity {
            let to_drop = overflow - self.capacity;
            for _ in 0..to_drop {
                let _ = buf.pop_front();
            }
        }

        buf.extend(bytes.iter().copied());
    }

    /// Run analysis on each source buffer and return the summary map.
    fn analyze(&self) -> HashMap<String, SessionSourceAnalysis> {
        self.data
            .iter()
            .filter(|(_, buf)| buf.len() >= 100) // Need minimum data for meaningful analysis
            .map(|(name, buf)| {
                let contiguous: Vec<u8> = buf.iter().copied().collect();
                let full = analysis::full_analysis(name, &contiguous);
                (name.clone(), SessionSourceAnalysis::from_full(&full))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Session metadata (session.json)
// ---------------------------------------------------------------------------

/// Session metadata written to session.json at the end of recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub version: u32,
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub sources: Vec<String>,
    pub conditioning: String,
    pub interval_ms: Option<u64>,
    pub total_samples: u64,
    pub samples_per_source: HashMap<String, u64>,
    pub machine: MachineInfo,
    pub tags: HashMap<String, String>,
    pub note: Option<String>,
    pub openentropy_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<HashMap<String, SessionSourceAnalysis>>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "telemetry")]
    pub telemetry_v1: Option<TelemetryWindowReport>,
}

// ---------------------------------------------------------------------------
// Session config
// ---------------------------------------------------------------------------

/// Configuration for a recording session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub sources: Vec<String>,
    pub conditioning: ConditioningMode,
    pub interval: Option<Duration>,
    pub output_dir: PathBuf,
    pub tags: HashMap<String, String>,
    pub note: Option<String>,
    pub duration: Option<Duration>,
    pub sample_size: usize,
    pub include_analysis: bool,
    pub include_telemetry: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            conditioning: ConditioningMode::Raw,
            interval: None,
            output_dir: PathBuf::from("sessions"),
            tags: HashMap::new(),
            note: None,
            duration: None,
            sample_size: 1000,
            include_analysis: false,
            include_telemetry: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Session writer
// ---------------------------------------------------------------------------

/// Number of samples between periodic flushes. Balances crash-safety
/// (data written to disk) against performance (fewer syscalls).
const FLUSH_INTERVAL: u64 = 64;

/// Handles incremental file I/O for a recording session.
///
/// Implements `Drop` to flush buffers and write a best-effort session.json
/// if `finish()` was never called (e.g., due to a panic or early exit).
pub struct SessionWriter {
    session_dir: PathBuf,
    csv_writer: BufWriter<File>,
    raw_writer: BufWriter<File>,
    conditioned_writer: BufWriter<File>,
    index_writer: BufWriter<File>,
    conditioned_index_writer: BufWriter<File>,
    raw_offset: u64,
    conditioned_offset: u64,
    total_samples: u64,
    samples_per_source: HashMap<String, u64>,
    started_at: SystemTime,
    started_instant: Instant,
    session_id: String,
    config: SessionConfig,
    machine: MachineInfo,
    /// Retains last 128 KiB per source for optional end-of-session analysis.
    analysis_buffer: Option<AnalysisBuffer>,
    /// Optional telemetry snapshot captured at session start.
    telemetry_start: Option<TelemetrySnapshot>,
    /// Set to true after `finish()` succeeds so `Drop` doesn't double-write.
    finished: bool,
}

impl SessionWriter {
    /// Create a new session writer, creating the session directory and files.
    ///
    /// # Errors
    ///
    /// Returns an error if the session directory or any output files cannot be created.
    pub fn new(config: SessionConfig) -> std::io::Result<Self> {
        let machine = detect_machine_info();
        let session_id = Uuid::new_v4().to_string();
        let started_at = SystemTime::now();

        // Build directory name: bounded and filesystem-safe to avoid ENAMETOOLONG
        // when many sources are recorded.
        let ts = started_at.duration_since(UNIX_EPOCH).unwrap_or_default();
        let dt = format_iso8601_compact(ts);
        let dir_name = build_session_dir_name(&dt, &config.sources, &session_id);

        let session_dir = config.output_dir.join(&dir_name);
        fs::create_dir_all(&session_dir)?;

        // Create samples.csv with header
        let csv_file = File::create(session_dir.join("samples.csv"))?;
        let mut csv_writer = BufWriter::new(csv_file);
        writeln!(
            csv_writer,
            "timestamp_ns,source,raw_hex,conditioned_hex,raw_shannon,raw_min_entropy,conditioned_shannon,conditioned_min_entropy"
        )?;
        csv_writer.flush()?;

        // Create raw.bin
        let raw_file = File::create(session_dir.join("raw.bin"))?;
        let raw_writer = BufWriter::new(raw_file);

        // Create conditioned.bin
        let conditioned_file = File::create(session_dir.join("conditioned.bin"))?;
        let conditioned_writer = BufWriter::new(conditioned_file);

        // Create raw_index.csv with header
        let index_file = File::create(session_dir.join("raw_index.csv"))?;
        let mut index_writer = BufWriter::new(index_file);
        writeln!(index_writer, "offset,length,timestamp_ns,source")?;
        index_writer.flush()?;

        // Create conditioned_index.csv with header
        let conditioned_index_file = File::create(session_dir.join("conditioned_index.csv"))?;
        let mut conditioned_index_writer = BufWriter::new(conditioned_index_file);
        writeln!(
            conditioned_index_writer,
            "offset,length,timestamp_ns,source"
        )?;
        conditioned_index_writer.flush()?;

        let samples_per_source: HashMap<String, u64> =
            config.sources.iter().map(|s| (s.clone(), 0)).collect();
        let analysis_buffer = if config.include_analysis {
            Some(AnalysisBuffer::new(&config.sources, 128 * 1024))
        } else {
            None
        };
        let telemetry_start = config.include_telemetry.then(collect_telemetry_snapshot);

        Ok(Self {
            session_dir,
            csv_writer,
            raw_writer,
            conditioned_writer,
            index_writer,
            conditioned_index_writer,
            raw_offset: 0,
            conditioned_offset: 0,
            total_samples: 0,
            samples_per_source,
            started_at,
            started_instant: Instant::now(),
            session_id,
            config,
            machine,
            analysis_buffer,
            telemetry_start,
            finished: false,
        })
    }

    /// Record a single sample from a source.
    ///
    /// Buffers are flushed periodically (every [`FLUSH_INTERVAL`] samples)
    /// rather than on every call, for performance. Data is still safe against
    /// process crashes because `Drop` flushes and writes session.json.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to any of the output files fails.
    pub fn write_sample(
        &mut self,
        source: &str,
        raw_bytes: &[u8],
        conditioned_bytes: &[u8],
    ) -> std::io::Result<()> {
        if raw_bytes.is_empty() {
            return Ok(());
        }

        #[allow(clippy::cast_possible_truncation)] // ns won't overflow u64 until ~2554
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let raw_shannon = quick_shannon(raw_bytes);
        // Clamp to 0.0 to avoid displaying "-0.00" in CSV
        let raw_min_entropy = quick_min_entropy(raw_bytes).max(0.0);
        let conditioned_shannon = quick_shannon(conditioned_bytes);
        let conditioned_min_entropy = quick_min_entropy(conditioned_bytes).max(0.0);
        let raw_hex = hex_encode(raw_bytes);
        let conditioned_hex = hex_encode(conditioned_bytes);

        // Write CSV row
        writeln!(
            self.csv_writer,
            "{timestamp_ns},{source},{raw_hex},{conditioned_hex},{raw_shannon:.2},{raw_min_entropy:.2},{conditioned_shannon:.2},{conditioned_min_entropy:.2}",
        )?;

        // Write raw bytes
        self.raw_writer.write_all(raw_bytes)?;
        self.conditioned_writer.write_all(conditioned_bytes)?;

        // Write index row
        writeln!(
            self.index_writer,
            "{},{},{timestamp_ns},{source}",
            self.raw_offset,
            raw_bytes.len(),
        )?;
        writeln!(
            self.conditioned_index_writer,
            "{},{},{timestamp_ns},{source}",
            self.conditioned_offset,
            conditioned_bytes.len(),
        )?;

        self.raw_offset += raw_bytes.len() as u64;
        self.conditioned_offset += conditioned_bytes.len() as u64;
        self.total_samples += 1;
        if let Some(buffer) = &mut self.analysis_buffer {
            buffer.push(source, raw_bytes);
        }
        *self
            .samples_per_source
            .entry(source.to_string())
            .or_insert(0) += 1;

        // Periodic flush for crash-safety without per-sample syscall overhead
        if self.total_samples.is_multiple_of(FLUSH_INTERVAL) {
            self.flush_all()?;
        }

        Ok(())
    }

    /// Flush all buffered writers to disk.
    fn flush_all(&mut self) -> std::io::Result<()> {
        self.csv_writer.flush()?;
        self.raw_writer.flush()?;
        self.conditioned_writer.flush()?;
        self.index_writer.flush()?;
        self.conditioned_index_writer.flush()?;
        Ok(())
    }

    /// Build the session metadata from current state.
    #[allow(clippy::cast_possible_truncation)] // durations won't overflow u64 in practice
    fn build_meta(&self) -> SessionMeta {
        let ended_at = SystemTime::now();
        let duration = self.started_instant.elapsed();

        let analysis = self.analysis_buffer.as_ref().and_then(|buffer| {
            let analysis_map = buffer.analyze();
            if analysis_map.is_empty() {
                None
            } else {
                Some(analysis_map)
            }
        });
        let telemetry = self
            .telemetry_start
            .as_ref()
            .cloned()
            .map(collect_telemetry_window);

        SessionMeta {
            version: 2,
            id: self.session_id.clone(),
            started_at: format_iso8601(
                self.started_at
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default(),
            ),
            ended_at: format_iso8601(ended_at.duration_since(UNIX_EPOCH).unwrap_or_default()),
            duration_ms: duration.as_millis() as u64,
            sources: self.config.sources.clone(),
            conditioning: self.config.conditioning.to_string(),
            interval_ms: self.config.interval.map(|d| d.as_millis() as u64),
            total_samples: self.total_samples,
            samples_per_source: self.samples_per_source.clone(),
            machine: self.machine.clone(),
            tags: self.config.tags.clone(),
            note: self.config.note.clone(),
            openentropy_version: crate::VERSION.to_string(),
            analysis,
            telemetry_v1: telemetry,
        }
    }

    /// Write session.json to disk.
    fn write_session_json(&self, meta: &SessionMeta) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(meta).map_err(std::io::Error::other)?;
        fs::write(self.session_dir.join("session.json"), json)
    }

    /// Finalize the session, writing session.json. Call this on graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if flushing buffers or writing session.json fails.
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        self.flush_all()?;
        let meta = self.build_meta();
        self.write_session_json(&meta)?;
        self.finished = true;
        Ok(self.session_dir.clone())
    }

    /// Get the session directory path.
    #[must_use]
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Get total samples recorded so far.
    #[must_use]
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Get elapsed time since recording started.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_instant.elapsed()
    }

    /// Get per-source sample counts.
    #[must_use]
    pub fn samples_per_source(&self) -> &HashMap<String, u64> {
        &self.samples_per_source
    }
}

impl Drop for SessionWriter {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Best-effort: flush buffers and write session.json so data isn't lost
        // on panic/early-exit. Errors are silently ignored since we're in Drop.
        let _ = self.flush_all();
        let meta = self.build_meta();
        let _ = self.write_session_json(&meta);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hex-encode bytes without any separator.
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// Format a duration-since-epoch as a compact ISO-8601 timestamp for directory names.
/// Example: `2026-02-15T013000Z`
fn format_iso8601_compact(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs();
    let (year, month, day, hour, min, sec) = secs_to_utc(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}{min:02}{sec:02}Z")
}

/// Format a duration-since-epoch as a full ISO-8601 timestamp.
/// Example: `2026-02-15T01:30:00Z`
fn format_iso8601(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs();
    let (year, month, day, hour, min, sec) = secs_to_utc(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Convert seconds since Unix epoch to (year, month, day, hour, minute, second) UTC.
/// Simple implementation — no leap second handling.
fn secs_to_utc(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;

    let mut days = secs / 86400;
    let mut year = 1970u64;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let months_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0u64;
    for (i, &md) in months_days.iter().enumerate() {
        if days < md {
            month = i as u64 + 1;
            break;
        }
        days -= md;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Build a compact, filesystem-safe session directory name.
///
/// Format: `{timestamp}-{source-label}-{id8}`
/// Examples:
/// - `2026-02-17T193000Z-clock_jitter-a1b2c3d4`
/// - `2026-02-17T193000Z-clock_jitter-plus34-a1b2c3d4`
fn build_session_dir_name(timestamp: &str, sources: &[String], session_id: &str) -> String {
    let first = sources.first().map(String::as_str).unwrap_or("unknown");
    let first = sanitize_for_path(first);
    let label = if sources.len() <= 1 {
        truncate_for_path(&first, 48)
    } else {
        let base = truncate_for_path(&first, 36);
        format!("{base}-plus{}", sources.len() - 1)
    };
    let id8 = session_id.chars().take(8).collect::<String>();
    format!("{timestamp}-{label}-{id8}")
}

/// Replace non path-safe characters with `_`.
fn sanitize_for_path(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Truncate by character count (ASCII-safe output from sanitize_for_path).
fn truncate_for_path(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Machine info tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_machine_info() {
        let info = detect_machine_info();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(info.cores > 0);
    }

    // -----------------------------------------------------------------------
    // ISO-8601 formatting tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_iso8601_epoch() {
        let s = format_iso8601(Duration::from_secs(0));
        assert_eq!(s, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_iso8601_compact_epoch() {
        let s = format_iso8601_compact(Duration::from_secs(0));
        assert_eq!(s, "1970-01-01T000000Z");
    }

    #[test]
    fn test_format_iso8601_known_date() {
        // 2026-02-15 01:30:00 UTC = 1771030200 seconds since epoch
        let s = format_iso8601(Duration::from_secs(1771030200));
        assert!(s.starts_with("2026-"));
    }

    // -----------------------------------------------------------------------
    // Hex encode tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hex_encode_empty() {
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_hex_encode_basic() {
        assert_eq!(hex_encode(&[0xab, 0xcd, 0x01]), "abcd01");
    }

    // -----------------------------------------------------------------------
    // SessionWriter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_writer_creates_directory_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["test_source".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let writer = SessionWriter::new(config).unwrap();
        let dir = writer.session_dir().to_path_buf();

        assert!(dir.exists());
        assert!(dir.join("samples.csv").exists());
        assert!(dir.join("raw.bin").exists());
        assert!(dir.join("raw_index.csv").exists());
        assert!(dir.join("conditioned.bin").exists());
        assert!(dir.join("conditioned_index.csv").exists());

        // Finish and verify session.json
        let result_dir = writer.finish().unwrap();
        assert!(result_dir.join("session.json").exists());
    }

    #[test]
    fn test_build_session_dir_name_is_compact() {
        let sources: Vec<String> = (0..40)
            .map(|i| format!("very_long_source_name_number_{i}_with_extra_detail"))
            .collect();
        let name = build_session_dir_name("2026-02-17T010203Z", &sources, "12345678-aaaa-bbbb");
        assert!(name.len() < 128, "dir name too long: {} chars", name.len());
        assert!(name.contains("plus39"));
    }

    #[test]
    fn test_session_writer_with_many_sources_does_not_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let sources: Vec<String> = (0..40)
            .map(|i| format!("very_long_source_name_number_{i}_with_extra_detail"))
            .collect();
        let config = SessionConfig {
            sources,
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let writer = SessionWriter::new(config).expect("SessionWriter should handle many sources");
        assert!(writer.session_dir().exists());
    }

    #[test]
    fn test_session_writer_writes_valid_csv() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["mock_source".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let mut writer = SessionWriter::new(config).unwrap();
        let data = vec![0xAA; 100];
        writer.write_sample("mock_source", &data, &data).unwrap();
        writer.write_sample("mock_source", &data, &data).unwrap();

        let dir = writer.session_dir().to_path_buf();
        let result_dir = writer.finish().unwrap();

        // Check CSV
        let csv = std::fs::read_to_string(dir.join("samples.csv")).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "timestamp_ns,source,raw_hex,conditioned_hex,raw_shannon,raw_min_entropy,conditioned_shannon,conditioned_min_entropy"
        );
        assert_eq!(lines.len(), 3); // header + 2 samples
        assert!(lines[1].contains("mock_source"));

        // Check raw.bin size
        let raw = std::fs::read(dir.join("raw.bin")).unwrap();
        assert_eq!(raw.len(), 200); // 2 x 100 bytes

        // Check raw_index.csv
        let index = std::fs::read_to_string(dir.join("raw_index.csv")).unwrap();
        let idx_lines: Vec<&str> = index.lines().collect();
        assert_eq!(idx_lines.len(), 3); // header + 2 entries
        assert!(idx_lines[1].starts_with("0,100,")); // first entry at offset 0
        assert!(idx_lines[2].starts_with("100,100,")); // second at offset 100

        // Check conditioned.bin/index
        let conditioned = std::fs::read(dir.join("conditioned.bin")).unwrap();
        assert_eq!(conditioned.len(), 200);
        let conditioned_index = std::fs::read_to_string(dir.join("conditioned_index.csv")).unwrap();
        let cidx_lines: Vec<&str> = conditioned_index.lines().collect();
        assert_eq!(cidx_lines.len(), 3);
        assert!(cidx_lines[1].starts_with("0,100,"));
        assert!(cidx_lines[2].starts_with("100,100,"));

        // Check session.json
        let json_str = std::fs::read_to_string(result_dir.join("session.json")).unwrap();
        let meta: SessionMeta = serde_json::from_str(&json_str).unwrap();
        assert_eq!(meta.version, 2);
        assert_eq!(meta.total_samples, 2);
        assert_eq!(meta.sources, vec!["mock_source"]);
        assert_eq!(*meta.samples_per_source.get("mock_source").unwrap(), 2);
        assert_eq!(meta.conditioning, "raw");
    }

    #[test]
    fn test_session_writer_multiple_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["source_a".to_string(), "source_b".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let mut writer = SessionWriter::new(config).unwrap();
        writer.write_sample("source_a", &[1; 50], &[4; 50]).unwrap();
        writer.write_sample("source_b", &[2; 75], &[5; 75]).unwrap();
        writer.write_sample("source_a", &[3; 50], &[6; 50]).unwrap();

        assert_eq!(writer.total_samples(), 3);
        assert_eq!(*writer.samples_per_source().get("source_a").unwrap(), 2);
        assert_eq!(*writer.samples_per_source().get("source_b").unwrap(), 1);

        let dir = writer.finish().unwrap();
        let meta: SessionMeta =
            serde_json::from_str(&std::fs::read_to_string(dir.join("session.json")).unwrap())
                .unwrap();
        assert_eq!(meta.total_samples, 3);
    }

    #[test]
    fn test_session_writer_with_tags_and_note() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tags = HashMap::new();
        tags.insert("crystal".to_string(), "quartz".to_string());
        tags.insert("distance".to_string(), "2cm".to_string());

        let config = SessionConfig {
            sources: vec!["test".to_string()],
            output_dir: tmp.path().to_path_buf(),
            tags,
            note: Some("Testing quartz crystal".to_string()),
            ..Default::default()
        };

        let writer = SessionWriter::new(config).unwrap();
        let dir = writer.finish().unwrap();

        let meta: SessionMeta =
            serde_json::from_str(&std::fs::read_to_string(dir.join("session.json")).unwrap())
                .unwrap();
        assert_eq!(meta.tags.get("crystal").unwrap(), "quartz");
        assert_eq!(meta.tags.get("distance").unwrap(), "2cm");
        assert_eq!(meta.note.unwrap(), "Testing quartz crystal");
    }

    #[test]
    fn test_session_meta_serialization_roundtrip() {
        let meta = SessionMeta {
            version: 2,
            id: "test-id".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: "2026-01-01T00:05:00Z".to_string(),
            duration_ms: 300000,
            sources: vec!["clock_jitter".to_string()],
            conditioning: "raw".to_string(),
            interval_ms: Some(100),
            total_samples: 3000,
            samples_per_source: {
                let mut m = HashMap::new();
                m.insert("clock_jitter".to_string(), 3000);
                m
            },
            machine: MachineInfo {
                os: "macos 15.4".to_string(),
                arch: "aarch64".to_string(),
                chip: "Apple M4".to_string(),
                cores: 10,
            },
            tags: HashMap::new(),
            note: None,
            openentropy_version: env!("CARGO_PKG_VERSION").to_string(),
            analysis: None,
            telemetry_v1: None,
        };

        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: SessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.total_samples, 3000);
        assert_eq!(parsed.duration_ms, 300000);
    }

    #[test]
    fn test_session_meta_accepts_legacy_telemetry_key() {
        let base = SessionMeta {
            version: 2,
            id: "test-id".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: "2026-01-01T00:05:00Z".to_string(),
            duration_ms: 300000,
            sources: vec!["clock_jitter".to_string()],
            conditioning: "raw".to_string(),
            interval_ms: Some(100),
            total_samples: 3000,
            samples_per_source: {
                let mut m = HashMap::new();
                m.insert("clock_jitter".to_string(), 3000);
                m
            },
            machine: MachineInfo {
                os: "macos 15.4".to_string(),
                arch: "aarch64".to_string(),
                chip: "Apple M4".to_string(),
                cores: 10,
            },
            tags: HashMap::new(),
            note: None,
            openentropy_version: env!("CARGO_PKG_VERSION").to_string(),
            analysis: None,
            telemetry_v1: None,
        };

        let window = TelemetryWindowReport {
            model_id: "telemetry_v1".to_string(),
            model_version: 1,
            elapsed_ms: 1234,
            start: TelemetrySnapshot {
                model_id: "telemetry_v1".to_string(),
                model_version: 1,
                collected_unix_ms: 1000,
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                cpu_count: 8,
                loadavg_1m: Some(1.0),
                loadavg_5m: Some(1.1),
                loadavg_15m: Some(1.2),
                metrics: vec![TelemetryMetric {
                    domain: "memory".to_string(),
                    name: "free_bytes".to_string(),
                    value: 100.0,
                    unit: "bytes".to_string(),
                    source: "test".to_string(),
                }],
            },
            end: TelemetrySnapshot {
                model_id: "telemetry_v1".to_string(),
                model_version: 1,
                collected_unix_ms: 2234,
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                cpu_count: 8,
                loadavg_1m: Some(1.3),
                loadavg_5m: Some(1.2),
                loadavg_15m: Some(1.1),
                metrics: vec![TelemetryMetric {
                    domain: "memory".to_string(),
                    name: "free_bytes".to_string(),
                    value: 80.0,
                    unit: "bytes".to_string(),
                    source: "test".to_string(),
                }],
            },
            deltas: vec![TelemetryMetricDelta {
                domain: "memory".to_string(),
                name: "free_bytes".to_string(),
                unit: "bytes".to_string(),
                source: "test".to_string(),
                start_value: 100.0,
                end_value: 80.0,
                delta_value: -20.0,
            }],
        };

        let mut json = serde_json::to_value(base).unwrap();
        let obj = json.as_object_mut().expect("session meta should be object");
        obj.insert(
            "telemetry".to_string(),
            serde_json::to_value(window).unwrap(),
        );

        let parsed: SessionMeta = serde_json::from_value(json).unwrap();
        assert!(parsed.telemetry_v1.is_some());
        assert_eq!(
            parsed
                .telemetry_v1
                .as_ref()
                .map(|t| t.model_id.as_str())
                .unwrap_or(""),
            "telemetry_v1"
        );
    }

    // -----------------------------------------------------------------------
    // Drop safety tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_drop_writes_session_json_without_finish() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["drop_test".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let mut writer = SessionWriter::new(config).unwrap();
        let dir = writer.session_dir().to_path_buf();
        writer
            .write_sample("drop_test", &[42; 100], &[24; 100])
            .unwrap();
        // Drop without calling finish()
        drop(writer);

        // session.json should still be written by Drop
        assert!(dir.join("session.json").exists());
        let meta: SessionMeta =
            serde_json::from_str(&std::fs::read_to_string(dir.join("session.json")).unwrap())
                .unwrap();
        assert_eq!(meta.total_samples, 1);
    }

    #[test]
    fn test_finish_prevents_double_write_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["test".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let writer = SessionWriter::new(config).unwrap();
        let dir = writer.session_dir().to_path_buf();
        let _ = writer.finish().unwrap();

        // session.json should exist (from finish), and Drop should not error
        assert!(dir.join("session.json").exists());
    }

    // -----------------------------------------------------------------------
    // Edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_sample_skips_empty_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["test".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let mut writer = SessionWriter::new(config).unwrap();
        writer.write_sample("test", &[], &[]).unwrap();
        assert_eq!(writer.total_samples(), 0);
        let _ = writer.finish().unwrap();
    }

    #[test]
    fn test_min_entropy_not_negative_in_csv() {
        let tmp = tempfile::tempdir().unwrap();
        let config = SessionConfig {
            sources: vec!["test".to_string()],
            output_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let mut writer = SessionWriter::new(config).unwrap();
        // All-same bytes produce near-zero min-entropy that could display as -0.00
        writer
            .write_sample("test", &[0xAA; 100], &[0xAA; 100])
            .unwrap();
        let dir = writer.session_dir().to_path_buf();
        let _ = writer.finish().unwrap();

        let csv = std::fs::read_to_string(dir.join("samples.csv")).unwrap();
        for line in csv.lines().skip(1) {
            assert!(
                !line.contains("-0.00"),
                "CSV should not contain negative zero: {line}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // UTC conversion tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_secs_to_utc_epoch() {
        let (y, m, d, h, mi, s) = secs_to_utc(0);
        assert_eq!((y, m, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_secs_to_utc_known_date() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let (y, m, d, h, mi, s) = secs_to_utc(946684800);
        assert_eq!((y, m, d, h, mi, s), (2000, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_is_leap() {
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert!(!is_leap(1900));
        assert!(!is_leap(2023));
    }
}
