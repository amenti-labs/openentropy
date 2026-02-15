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
//! - `samples.csv` — per-sample metrics (timestamp, source, entropy stats)
//! - `raw.bin` — concatenated raw bytes
//! - `raw_index.csv` — byte offset index into raw.bin

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::conditioning::{quick_min_entropy, quick_shannon, ConditioningMode};

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
        .map(|n| n.get())
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
                s.lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
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
        }
    }
}

// ---------------------------------------------------------------------------
// Session writer
// ---------------------------------------------------------------------------

/// Handles incremental file I/O for a recording session.
pub struct SessionWriter {
    session_dir: PathBuf,
    csv_writer: BufWriter<File>,
    raw_writer: BufWriter<File>,
    index_writer: BufWriter<File>,
    raw_offset: u64,
    total_samples: u64,
    samples_per_source: HashMap<String, u64>,
    started_at: SystemTime,
    started_instant: Instant,
    session_id: String,
    config: SessionConfig,
    machine: MachineInfo,
}

impl SessionWriter {
    /// Create a new session writer, creating the session directory and files.
    pub fn new(config: SessionConfig) -> std::io::Result<Self> {
        let machine = detect_machine_info();
        let session_id = Uuid::new_v4().to_string();
        let started_at = SystemTime::now();

        // Build directory name: {timestamp}-{source_names}
        let ts = started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let dt = format_iso8601_compact(ts);
        let sources_slug = config.sources.join("-");
        let dir_name = format!("{}-{}", dt, sources_slug);

        let session_dir = config.output_dir.join(&dir_name);
        fs::create_dir_all(&session_dir)?;

        // Create samples.csv with header
        let csv_file = File::create(session_dir.join("samples.csv"))?;
        let mut csv_writer = BufWriter::new(csv_file);
        writeln!(csv_writer, "timestamp_ns,source,value_hex,shannon,min_entropy")?;
        csv_writer.flush()?;

        // Create raw.bin
        let raw_file = File::create(session_dir.join("raw.bin"))?;
        let raw_writer = BufWriter::new(raw_file);

        // Create raw_index.csv with header
        let index_file = File::create(session_dir.join("raw_index.csv"))?;
        let mut index_writer = BufWriter::new(index_file);
        writeln!(index_writer, "offset,length,timestamp_ns,source")?;
        index_writer.flush()?;

        let samples_per_source: HashMap<String, u64> =
            config.sources.iter().map(|s| (s.clone(), 0)).collect();

        Ok(Self {
            session_dir,
            csv_writer,
            raw_writer,
            index_writer,
            raw_offset: 0,
            total_samples: 0,
            samples_per_source,
            started_at,
            started_instant: Instant::now(),
            session_id,
            config,
            machine,
        })
    }

    /// Record a single sample from a source.
    pub fn write_sample(
        &mut self,
        source: &str,
        raw_bytes: &[u8],
    ) -> std::io::Result<()> {
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let shannon = quick_shannon(raw_bytes);
        let min_entropy = quick_min_entropy(raw_bytes);
        let value_hex = hex_encode(raw_bytes);

        // Write CSV row
        writeln!(
            self.csv_writer,
            "{},{},{},{:.2},{:.2}",
            timestamp_ns, source, value_hex, shannon, min_entropy
        )?;
        self.csv_writer.flush()?;

        // Write raw bytes
        self.raw_writer.write_all(raw_bytes)?;
        self.raw_writer.flush()?;

        // Write index row
        writeln!(
            self.index_writer,
            "{},{},{},{}",
            self.raw_offset,
            raw_bytes.len(),
            timestamp_ns,
            source
        )?;
        self.index_writer.flush()?;

        self.raw_offset += raw_bytes.len() as u64;
        self.total_samples += 1;
        *self.samples_per_source.entry(source.to_string()).or_insert(0) += 1;

        Ok(())
    }

    /// Finalize the session, writing session.json. Call this on graceful shutdown.
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        // Flush all writers
        self.csv_writer.flush()?;
        self.raw_writer.flush()?;
        self.index_writer.flush()?;

        let ended_at = SystemTime::now();
        let duration = self.started_instant.elapsed();

        let meta = SessionMeta {
            version: 1,
            id: self.session_id,
            started_at: format_iso8601(
                self.started_at
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default(),
            ),
            ended_at: format_iso8601(
                ended_at.duration_since(UNIX_EPOCH).unwrap_or_default(),
            ),
            duration_ms: duration.as_millis() as u64,
            sources: self.config.sources.clone(),
            conditioning: self.config.conditioning.to_string(),
            interval_ms: self.config.interval.map(|d| d.as_millis() as u64),
            total_samples: self.total_samples,
            samples_per_source: self.samples_per_source.clone(),
            machine: self.machine,
            tags: self.config.tags.clone(),
            note: self.config.note.clone(),
            openentropy_version: crate::VERSION.to_string(),
        };

        let json = serde_json::to_string_pretty(&meta)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(self.session_dir.join("session.json"), json)?;

        Ok(self.session_dir.clone())
    }

    /// Get the session directory path.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Get total samples recorded so far.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Get elapsed time since recording started.
    pub fn elapsed(&self) -> Duration {
        self.started_instant.elapsed()
    }

    /// Get per-source sample counts.
    pub fn samples_per_source(&self) -> &HashMap<String, u64> {
        &self.samples_per_source
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
        write!(s, "{:02x}", b).unwrap();
    }
    s
}

/// Format a duration-since-epoch as a compact ISO-8601 timestamp for directory names.
/// Example: `2026-02-15T013000Z`
fn format_iso8601_compact(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs();
    let (year, month, day, hour, min, sec) = secs_to_utc(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}{:02}{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Format a duration-since-epoch as a full ISO-8601 timestamp.
/// Example: `2026-02-15T01:30:00Z`
fn format_iso8601(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs();
    let (year, month, day, hour, min, sec) = secs_to_utc(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
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
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
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

        // Finish and verify session.json
        let result_dir = writer.finish().unwrap();
        assert!(result_dir.join("session.json").exists());
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
        writer.write_sample("mock_source", &data).unwrap();
        writer.write_sample("mock_source", &data).unwrap();

        let dir = writer.session_dir().to_path_buf();
        let result_dir = writer.finish().unwrap();

        // Check CSV
        let csv = std::fs::read_to_string(dir.join("samples.csv")).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "timestamp_ns,source,value_hex,shannon,min_entropy");
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

        // Check session.json
        let json_str = std::fs::read_to_string(result_dir.join("session.json")).unwrap();
        let meta: SessionMeta = serde_json::from_str(&json_str).unwrap();
        assert_eq!(meta.version, 1);
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
        writer.write_sample("source_a", &[1; 50]).unwrap();
        writer.write_sample("source_b", &[2; 75]).unwrap();
        writer.write_sample("source_a", &[3; 50]).unwrap();

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
            version: 1,
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
            openentropy_version: "0.4.1".to_string(),
        };

        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: SessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.total_samples, 3000);
        assert_eq!(parsed.duration_ms, 300000);
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
