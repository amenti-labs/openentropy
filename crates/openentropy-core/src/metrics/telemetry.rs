//! Best-effort system telemetry snapshots for entropy benchmark context.
//!
//! `telemetry_v1` is intentionally operational:
//! - works without elevated privileges where possible,
//! - captures only values observable from user space,
//! - leaves unavailable metrics as absent rather than guessing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::io::Read;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Stdio;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

/// Telemetry model identifier.
pub const MODEL_ID: &str = "telemetry_v1";
/// Telemetry model version.
pub const MODEL_VERSION: u32 = 1;

/// A single observed telemetry metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMetric {
    pub domain: String,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub source: String,
}

/// Point-in-time system telemetry snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub model_id: String,
    pub model_version: u32,
    pub collected_unix_ms: u64,
    pub os: String,
    pub arch: String,
    pub cpu_count: usize,
    pub loadavg_1m: Option<f64>,
    pub loadavg_5m: Option<f64>,
    pub loadavg_15m: Option<f64>,
    pub metrics: Vec<TelemetryMetric>,
}

/// Delta for a metric observed in both start and end snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMetricDelta {
    pub domain: String,
    pub name: String,
    pub unit: String,
    pub source: String,
    pub start_value: f64,
    pub end_value: f64,
    pub delta_value: f64,
}

/// Start/end telemetry window with aligned metric deltas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryWindowReport {
    pub model_id: String,
    pub model_version: u32,
    pub elapsed_ms: u64,
    pub start: TelemetrySnapshot,
    pub end: TelemetrySnapshot,
    pub deltas: Vec<TelemetryMetricDelta>,
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(target_os = "macos")]
fn unix_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn push_metric(
    out: &mut Vec<TelemetryMetric>,
    domain: &str,
    name: impl Into<String>,
    value: f64,
    unit: &str,
    source: &str,
) {
    if !value.is_finite() {
        return;
    }
    out.push(TelemetryMetric {
        domain: domain.to_string(),
        name: name.into(),
        value,
        unit: unit.to_string(),
        source: source.to_string(),
    });
}

#[cfg(target_os = "linux")]
fn read_trimmed(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v = raw.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(target_os = "linux")]
fn normalize_key(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_us = false;
    for ch in raw.to_ascii_lowercase().chars() {
        let mapped = if ch.is_ascii_alphanumeric() { ch } else { '_' };
        if mapped == '_' {
            if !prev_us {
                out.push(mapped);
            }
            prev_us = true;
        } else {
            out.push(mapped);
            prev_us = false;
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(target_os = "macos")]
fn run_command(cmd: &str, args: &[&str]) -> Option<String> {
    const COMMAND_TIMEOUT: Duration = Duration::from_millis(400);

    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = Vec::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_end(&mut out);
                }
                let s = String::from_utf8_lossy(&out).trim().to_string();
                return if s.is_empty() { None } else { Some(s) };
            }
            Ok(None) => {
                if start.elapsed() >= COMMAND_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

#[cfg(target_os = "macos")]
fn read_sysctl(key: &str) -> Option<String> {
    run_command("sysctl", &["-n", key])
}

#[cfg(target_os = "macos")]
fn parse_first_f64(s: &str) -> Option<f64> {
    s.split_whitespace().next()?.parse::<f64>().ok()
}

fn collect_loadavg() -> (Option<f64>, Option<f64>, Option<f64>) {
    #[cfg(unix)]
    {
        let mut values = [0.0_f64; 3];
        // SAFETY: `getloadavg` writes up to `n` doubles to a valid buffer.
        let n = unsafe { libc::getloadavg(values.as_mut_ptr(), 3) };
        if n <= 0 {
            (None, None, None)
        } else {
            (
                Some(values[0]),
                (n > 1).then_some(values[1]),
                (n > 2).then_some(values[2]),
            )
        }
    }
    #[cfg(not(unix))]
    {
        (None, None, None)
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_proc_metrics(out: &mut Vec<TelemetryMetric>) {
    if let Some(uptime) = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| {
        s.split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
    }) {
        push_metric(out, "system", "uptime_seconds", uptime, "s", "procfs");
    }

    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        for line in mem.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let Some(value_kb) = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
            else {
                continue;
            };
            let bytes = value_kb * 1024.0;
            let metric_name = match key {
                "MemTotal" => Some("total_bytes"),
                "MemAvailable" => Some("available_bytes"),
                "MemFree" => Some("free_bytes"),
                "Buffers" => Some("buffers_bytes"),
                "Cached" => Some("cached_bytes"),
                "SwapTotal" => Some("swap_total_bytes"),
                "SwapFree" => Some("swap_free_bytes"),
                "SwapCached" => Some("swap_cached_bytes"),
                _ => None,
            };
            if let Some(name) = metric_name {
                push_metric(out, "memory", name, bytes, "bytes", "procfs");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_freq_metrics(out: &mut Vec<TelemetryMetric>) {
    let cpufreq_paths = [
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_cur_freq",
    ];
    for path in cpufreq_paths {
        if let Some(khz) = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
        {
            push_metric(out, "frequency", "cpu0_hz", khz * 1000.0, "Hz", "cpufreq");
            return;
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_hwmon_metrics(out: &mut Vec<TelemetryMetric>) {
    let root = Path::new("/sys/class/hwmon");
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let chip = read_trimmed(&dir.join("name"))
            .map(|s| normalize_key(&s))
            .unwrap_or_else(|| {
                normalize_key(
                    dir.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown_hwmon"),
                )
            });

        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let Some(name_os) = path.file_name() else {
                continue;
            };
            let fname = name_os.to_string_lossy();
            if !fname.ends_with("_input") {
                continue;
            }
            let Some(raw) = read_trimmed(&path).and_then(|s| s.parse::<f64>().ok()) else {
                continue;
            };

            let label_path = dir.join(fname.replace("_input", "_label"));
            let label = read_trimmed(&label_path)
                .map(|s| normalize_key(&s))
                .unwrap_or_else(|| normalize_key(fname.trim_end_matches("_input")));
            let metric_key = format!("{chip}.{label}");

            if fname.starts_with("temp") {
                push_metric(out, "thermal", metric_key, raw / 1000.0, "C", "linux_hwmon");
            } else if fname.starts_with("in") {
                push_metric(out, "voltage", metric_key, raw / 1000.0, "V", "linux_hwmon");
            } else if fname.starts_with("curr") {
                push_metric(out, "current", metric_key, raw / 1000.0, "A", "linux_hwmon");
            } else if fname.starts_with("power") {
                push_metric(
                    out,
                    "power",
                    metric_key,
                    raw / 1_000_000.0,
                    "W",
                    "linux_hwmon",
                );
            } else if fname.starts_with("fan") {
                push_metric(out, "cooling", metric_key, raw, "rpm", "linux_hwmon");
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_metrics(out: &mut Vec<TelemetryMetric>) {
    if let Some(tb_hz) = read_sysctl("hw.tbfrequency").and_then(|s| parse_first_f64(&s)) {
        push_metric(out, "frequency", "timebase_hz", tb_hz, "Hz", "sysctl");
    }
    if let Some(total_bytes) = read_sysctl("hw.memsize").and_then(|s| parse_first_f64(&s)) {
        push_metric(out, "memory", "total_bytes", total_bytes, "bytes", "sysctl");
    }
    if let Some(boot_raw) = read_sysctl("kern.boottime")
        && let Some(sec_part) = boot_raw
            .split("sec =")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse::<u64>().ok())
    {
        let uptime = unix_secs_now().saturating_sub(sec_part) as f64;
        push_metric(out, "system", "uptime_seconds", uptime, "s", "sysctl");
    }

    if let Some(vm) = run_command("vm_stat", &[]) {
        let mut page_size = 4096.0_f64;
        for line in vm.lines() {
            if line.contains("page size of")
                && let Some(ps) = line
                    .split("page size of")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<f64>().ok())
            {
                page_size = ps;
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let cleaned = v.replace('.', "");
            let Some(pages) = cleaned
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
            else {
                continue;
            };
            let bytes = pages * page_size;
            let metric_name = match k.trim() {
                "Pages free" => Some("free_bytes"),
                "Pages active" => Some("active_bytes"),
                "Pages inactive" => Some("inactive_bytes"),
                "Pages speculative" => Some("speculative_bytes"),
                "Pages throttled" => Some("throttled_bytes"),
                "Pages wired down" => Some("wired_bytes"),
                "Pages occupied by compressor" => Some("compressed_bytes"),
                _ => None,
            };
            if let Some(name) = metric_name {
                push_metric(out, "memory", name, bytes, "bytes", "vm_stat");
            }
        }
    }
}

/// Capture a best-effort telemetry snapshot.
pub fn collect_telemetry_snapshot() -> TelemetrySnapshot {
    let (load1, load5, load15) = collect_loadavg();
    let mut metrics = Vec::new();

    #[cfg(target_os = "linux")]
    {
        collect_linux_proc_metrics(&mut metrics);
        collect_linux_freq_metrics(&mut metrics);
        collect_linux_hwmon_metrics(&mut metrics);
    }
    #[cfg(target_os = "macos")]
    {
        collect_macos_metrics(&mut metrics);
    }

    metrics.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then(a.name.cmp(&b.name))
            .then(a.source.cmp(&b.source))
            .then(a.unit.cmp(&b.unit))
    });

    TelemetrySnapshot {
        model_id: MODEL_ID.to_string(),
        model_version: MODEL_VERSION,
        collected_unix_ms: unix_ms_now(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1),
        loadavg_1m: load1,
        loadavg_5m: load5,
        loadavg_15m: load15,
        metrics,
    }
}

fn delta_key(metric: &TelemetryMetric) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        metric.domain, metric.name, metric.unit, metric.source
    )
}

/// Build a start/end telemetry report and aligned metric deltas.
pub fn build_telemetry_window(
    start: TelemetrySnapshot,
    end: TelemetrySnapshot,
) -> TelemetryWindowReport {
    let end_map: HashMap<String, &TelemetryMetric> =
        end.metrics.iter().map(|m| (delta_key(m), m)).collect();
    let mut deltas = Vec::new();

    for sm in &start.metrics {
        if let Some(em) = end_map.get(&delta_key(sm)) {
            deltas.push(TelemetryMetricDelta {
                domain: sm.domain.clone(),
                name: sm.name.clone(),
                unit: sm.unit.clone(),
                source: sm.source.clone(),
                start_value: sm.value,
                end_value: em.value,
                delta_value: em.value - sm.value,
            });
        }
    }

    deltas.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then(a.name.cmp(&b.name))
            .then(a.source.cmp(&b.source))
    });

    TelemetryWindowReport {
        model_id: MODEL_ID.to_string(),
        model_version: MODEL_VERSION,
        elapsed_ms: end
            .collected_unix_ms
            .saturating_sub(start.collected_unix_ms),
        start,
        end,
        deltas,
    }
}

/// Capture the current end snapshot and compute a telemetry window.
pub fn collect_telemetry_window(start: TelemetrySnapshot) -> TelemetryWindowReport {
    let end = collect_telemetry_snapshot();
    build_telemetry_window(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_has_identity() {
        let s = collect_telemetry_snapshot();
        assert_eq!(s.model_id, MODEL_ID);
        assert_eq!(s.model_version, MODEL_VERSION);
        assert!(s.collected_unix_ms > 0);
        assert!(s.cpu_count >= 1);
    }

    #[test]
    fn window_delta_aligns_metrics() {
        let start = TelemetrySnapshot {
            model_id: MODEL_ID.to_string(),
            model_version: MODEL_VERSION,
            collected_unix_ms: 1000,
            os: "test".to_string(),
            arch: "test".to_string(),
            cpu_count: 1,
            loadavg_1m: None,
            loadavg_5m: None,
            loadavg_15m: None,
            metrics: vec![TelemetryMetric {
                domain: "memory".to_string(),
                name: "free_bytes".to_string(),
                value: 100.0,
                unit: "bytes".to_string(),
                source: "test".to_string(),
            }],
        };
        let mut end = start.clone();
        end.collected_unix_ms = 1500;
        end.metrics[0].value = 85.0;
        let w = build_telemetry_window(start, end);
        assert_eq!(w.elapsed_ms, 500);
        assert_eq!(w.deltas.len(), 1);
        assert!((w.deltas[0].delta_value + 15.0).abs() < 1e-9);
    }

    #[test]
    fn window_delta_keeps_distinct_sources() {
        let start = TelemetrySnapshot {
            model_id: MODEL_ID.to_string(),
            model_version: MODEL_VERSION,
            collected_unix_ms: 1000,
            os: "test".to_string(),
            arch: "test".to_string(),
            cpu_count: 1,
            loadavg_1m: None,
            loadavg_5m: None,
            loadavg_15m: None,
            metrics: vec![
                TelemetryMetric {
                    domain: "thermal".to_string(),
                    name: "sensor".to_string(),
                    value: 40.0,
                    unit: "C".to_string(),
                    source: "a".to_string(),
                },
                TelemetryMetric {
                    domain: "thermal".to_string(),
                    name: "sensor".to_string(),
                    value: 50.0,
                    unit: "C".to_string(),
                    source: "b".to_string(),
                },
            ],
        };
        let mut end = start.clone();
        end.collected_unix_ms = 1200;
        end.metrics[0].value = 42.0;
        end.metrics[1].value = 52.0;
        let w = build_telemetry_window(start, end);
        assert_eq!(w.deltas.len(), 2);
        assert!(
            w.deltas
                .iter()
                .any(|d| d.source == "a" && (d.delta_value - 2.0).abs() < 1e-9)
        );
        assert!(
            w.deltas
                .iter()
                .any(|d| d.source == "b" && (d.delta_value - 2.0).abs() < 1e-9)
        );
    }
}
