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

#[cfg(target_os = "linux")]
fn read_first_f64(path: &Path) -> Option<f64> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
}

#[cfg(target_os = "linux")]
fn linux_clk_tck() -> f64 {
    // SAFETY: `sysconf` is thread-safe for this query and has no side effects.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz > 0 { hz as f64 } else { 100.0 }
}

#[cfg(target_os = "linux")]
fn parse_psi_line(resource: &str, line: &str, out: &mut Vec<TelemetryMetric>) {
    let mut parts = line.split_whitespace();
    let Some(scope) = parts.next() else {
        return;
    };
    if scope != "some" && scope != "full" {
        return;
    }
    for item in parts {
        let Some((k, v)) = item.split_once('=') else {
            continue;
        };
        let Some(value) = v.parse::<f64>().ok() else {
            continue;
        };
        let name = format!("{resource}_{scope}_{k}");
        let unit = if k.starts_with("avg") { "pct" } else { "us" };
        push_metric(out, "pressure", name, value, unit, "psi");
    }
}

#[cfg(target_os = "linux")]
fn is_likely_disk_device(name: &str) -> bool {
    if name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("dm-")
        || name.starts_with("md")
        || name.starts_with("zram")
        || name.starts_with("sr")
        || name.starts_with("fd")
        || name.starts_with("nbd")
    {
        return false;
    }
    if name.starts_with("nvme") {
        return !name.contains('p');
    }
    if name.starts_with("mmcblk") {
        return !name.contains('p');
    }
    if name.starts_with("sd")
        || name.starts_with("hd")
        || name.starts_with("vd")
        || name.starts_with("xvd")
    {
        return !name.chars().last().is_some_and(|c| c.is_ascii_digit());
    }
    !name.chars().last().is_some_and(|c| c.is_ascii_digit())
}

#[cfg(target_os = "linux")]
fn parse_microunit_supply(
    out: &mut Vec<TelemetryMetric>,
    dir: &Path,
    metric_prefix: &str,
    key: &str,
    unit: &str,
    scale: f64,
) {
    let path = dir.join(key);
    if let Some(raw) = read_first_f64(&path) {
        push_metric(
            out,
            "power",
            format!("{metric_prefix}.{key}"),
            raw / scale,
            unit,
            "linux_power_supply",
        );
    }
}

#[cfg(target_os = "linux")]
fn parse_power_supply_state(out: &mut Vec<TelemetryMetric>, dir: &Path, metric_prefix: &str) {
    let status_path = dir.join("status");
    let Some(status) = read_trimmed(&status_path) else {
        return;
    };
    let lower = status.to_ascii_lowercase();
    let flags = [
        ("is_charging", lower == "charging"),
        ("is_discharging", lower == "discharging"),
        ("is_full", lower == "full"),
        ("is_not_charging", lower == "not charging"),
    ];
    for (label, enabled) in flags {
        push_metric(
            out,
            "power",
            format!("{metric_prefix}.{label}"),
            if enabled { 1.0 } else { 0.0 },
            "bool",
            "linux_power_supply",
        );
    }
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

#[cfg(target_os = "macos")]
fn parse_size_with_suffix(token: &str) -> Option<f64> {
    let suffix = token.chars().last()?;
    let (number, multiplier) = match suffix {
        'K' | 'k' => (token.get(..token.len().saturating_sub(1))?, 1024.0),
        'M' | 'm' => (token.get(..token.len().saturating_sub(1))?, 1024.0 * 1024.0),
        'G' | 'g' => (
            token.get(..token.len().saturating_sub(1))?,
            1024.0 * 1024.0 * 1024.0,
        ),
        'T' | 't' => (
            token.get(..token.len().saturating_sub(1))?,
            1024.0 * 1024.0 * 1024.0 * 1024.0,
        ),
        'P' | 'p' => (
            token.get(..token.len().saturating_sub(1))?,
            1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        ),
        _ => (token, 1.0),
    };
    number.parse::<f64>().ok().map(|v| v * multiplier)
}

#[cfg(target_os = "macos")]
fn parse_vm_stat_value(raw: &str) -> Option<f64> {
    let cleaned = raw.replace(['.', ','], "");
    cleaned
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
}

#[cfg(target_os = "macos")]
fn parse_macos_uptime_piece(raw: &str) -> Option<f64> {
    let piece = raw.trim().trim_start_matches("up ").trim();
    if piece.is_empty() {
        return None;
    }
    if let Some((h, m)) = piece.split_once(':')
        && let (Ok(hours), Ok(minutes)) = (h.trim().parse::<f64>(), m.trim().parse::<f64>())
    {
        return Some(hours * 3600.0 + minutes * 60.0);
    }

    let mut parts = piece.split_whitespace();
    let value = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.next().unwrap_or_default().to_ascii_lowercase();
    if unit.starts_with("day") {
        Some(value * 86_400.0)
    } else if unit.starts_with("hr") || unit.starts_with("hour") {
        Some(value * 3600.0)
    } else if unit.starts_with("min") {
        Some(value * 60.0)
    } else if unit.starts_with("sec") {
        Some(value)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_uptime_metrics(out: &mut Vec<TelemetryMetric>) {
    let Some(raw) = run_command("uptime", &[]) else {
        return;
    };
    let left = raw
        .split("load average")
        .next()
        .unwrap_or(raw.as_str())
        .trim()
        .trim_end_matches(',')
        .trim();

    let uptime_section = raw
        .split_once(" up ")
        .map(|(_, rest)| {
            rest.split("load average")
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_end_matches(',')
                .trim()
        })
        .unwrap_or("");

    let mut uptime_seconds = 0.0;
    let mut users = None;
    for piece in left.split(',').map(str::trim) {
        if piece.contains("user") {
            users = piece.split_whitespace().find_map(|s| s.parse::<f64>().ok());
            continue;
        }
        if let Some(sec) = parse_macos_uptime_piece(piece) {
            uptime_seconds += sec;
        }
    }
    if uptime_seconds <= 0.0 {
        for piece in uptime_section.split(',').map(str::trim) {
            if let Some(sec) = parse_macos_uptime_piece(piece) {
                uptime_seconds += sec;
            }
        }
    }

    if uptime_seconds > 0.0 {
        push_metric(
            out,
            "system",
            "uptime_seconds",
            uptime_seconds,
            "s",
            "uptime",
        );
    }
    if let Some(users) = users {
        push_metric(out, "system", "logged_in_users", users, "count", "uptime");
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_sysctl_metrics(out: &mut Vec<TelemetryMetric>) {
    if let Some(tb_hz) = read_sysctl("hw.tbfrequency").and_then(|s| parse_first_f64(&s)) {
        push_metric(out, "frequency", "timebase_hz", tb_hz, "Hz", "sysctl");
    }
    if let Some(cpu_hz) = read_sysctl("hw.cpufrequency").and_then(|s| parse_first_f64(&s)) {
        push_metric(out, "frequency", "cpu_hz", cpu_hz, "Hz", "sysctl");
    }
    if let Some(total_bytes) = read_sysctl("hw.memsize").and_then(|s| parse_first_f64(&s)) {
        push_metric(out, "memory", "total_bytes", total_bytes, "bytes", "sysctl");
    }
    if let Some(active_cpu) = read_sysctl("hw.activecpu").and_then(|s| parse_first_f64(&s)) {
        push_metric(
            out,
            "scheduling",
            "active_cpu_count",
            active_cpu,
            "count",
            "sysctl",
        );
    }
    if let Some(num_tasks) = read_sysctl("kern.num_tasks").and_then(|s| parse_first_f64(&s)) {
        push_metric(
            out,
            "scheduling",
            "task_count",
            num_tasks,
            "count",
            "sysctl",
        );
    }
    if let Some(num_threads) = read_sysctl("kern.num_threads").and_then(|s| parse_first_f64(&s)) {
        push_metric(
            out,
            "scheduling",
            "thread_count",
            num_threads,
            "count",
            "sysctl",
        );
    }
    if let Some(pressure_level) =
        read_sysctl("kern.memorystatus_vm_pressure_level").and_then(|s| parse_first_f64(&s))
    {
        push_metric(
            out,
            "pressure",
            "memory_pressure_level",
            pressure_level,
            "level",
            "sysctl",
        );
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
    if let Some(swapusage) = read_sysctl("vm.swapusage") {
        for (label, metric_name) in [
            ("total", "swap_total_bytes"),
            ("used", "swap_used_bytes"),
            ("free", "swap_free_bytes"),
        ] {
            if let Some(value) = swapusage
                .split(&format!("{label} ="))
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(parse_size_with_suffix)
            {
                push_metric(out, "memory", metric_name, value, "bytes", "sysctl");
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_cp_time_metrics(out: &mut Vec<TelemetryMetric>) {
    let Some(raw) = read_sysctl("kern.cp_time") else {
        return;
    };
    let parts: Vec<f64> = raw
        .split_whitespace()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() < 5 {
        return;
    }

    let user = parts[0];
    let nice = parts[1];
    let system = parts[2];
    let idle = parts[3];
    let intr = parts[4];

    push_metric(
        out,
        "scheduling",
        "cpu_time_user_ticks",
        user,
        "ticks",
        "sysctl",
    );
    push_metric(
        out,
        "scheduling",
        "cpu_time_nice_ticks",
        nice,
        "ticks",
        "sysctl",
    );
    push_metric(
        out,
        "scheduling",
        "cpu_time_system_ticks",
        system,
        "ticks",
        "sysctl",
    );
    push_metric(
        out,
        "scheduling",
        "cpu_time_idle_ticks",
        idle,
        "ticks",
        "sysctl",
    );
    push_metric(
        out,
        "scheduling",
        "cpu_time_interrupt_ticks",
        intr,
        "ticks",
        "sysctl",
    );
    push_metric(
        out,
        "scheduling",
        "cpu_time_busy_ticks",
        user + nice + system + intr,
        "ticks",
        "sysctl",
    );
}

#[cfg(target_os = "macos")]
fn collect_macos_vm_stat_metrics(out: &mut Vec<TelemetryMetric>) {
    let Some(vm) = run_command("vm_stat", &[]) else {
        return;
    };
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
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let key = raw_key.trim().trim_matches('"');
        let Some(value) = parse_vm_stat_value(raw_value) else {
            continue;
        };

        if let Some(metric_name) = match key {
            "Pages free" => Some("free_bytes"),
            "Pages active" => Some("active_bytes"),
            "Pages inactive" => Some("inactive_bytes"),
            "Pages speculative" => Some("speculative_bytes"),
            "Pages throttled" => Some("throttled_bytes"),
            "Pages wired down" => Some("wired_bytes"),
            "Pages purgeable" => Some("purgeable_bytes"),
            "File-backed pages" => Some("file_backed_bytes"),
            "Anonymous pages" => Some("anonymous_bytes"),
            "Pages occupied by compressor" => Some("compressed_bytes"),
            "Pages stored in compressor" => Some("compressed_store_bytes"),
            _ => None,
        } {
            push_metric(
                out,
                "memory",
                metric_name,
                value * page_size,
                "bytes",
                "vm_stat",
            );
            continue;
        }

        if let Some(metric_name) = match key {
            "Translation faults" => Some("translation_faults_total"),
            "Pageins" => Some("pageins_total"),
            "Pageouts" => Some("pageouts_total"),
            "Swapins" => Some("swapins_total"),
            "Swapouts" => Some("swapouts_total"),
            "Cow faults" => Some("cow_faults_total"),
            "Reactivations" => Some("reactivations_total"),
            "Compressions" => Some("compressions_total"),
            "Decompressions" => Some("decompressions_total"),
            "Zero fill pages" => Some("zero_fill_pages_total"),
            "Purgeable count" => Some("purgeable_count_total"),
            _ => None,
        } {
            push_metric(out, "vm", metric_name, value, "count", "vm_stat");
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_network_metrics(out: &mut Vec<TelemetryMetric>) {
    let Some(raw) = run_command("netstat", &["-ibn"]) else {
        return;
    };
    let mut interface_count = 0.0;
    let mut rx_bytes = 0.0;
    let mut tx_bytes = 0.0;
    let mut rx_packets = 0.0;
    let mut tx_packets = 0.0;
    let mut rx_errors = 0.0;
    let mut tx_errors = 0.0;

    for line in raw.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.is_empty() || cols[0] == "Name" || cols.len() < 8 {
            continue;
        }
        let name = cols[0];
        if name.starts_with("lo") {
            continue;
        }

        // Most macOS netstat formats place these counters near the end.
        let ibytes = cols.iter().rev().nth(1).and_then(|s| s.parse::<f64>().ok());
        let obytes = cols.iter().next_back().and_then(|s| s.parse::<f64>().ok());
        let ipkts = cols.get(4).and_then(|s| s.parse::<f64>().ok());
        let opkts = cols.get(6).and_then(|s| s.parse::<f64>().ok());
        let ierrs = cols.get(5).and_then(|s| s.parse::<f64>().ok());
        let oerrs = cols.get(7).and_then(|s| s.parse::<f64>().ok());

        if let (Some(ibytes), Some(obytes)) = (ibytes, obytes) {
            interface_count += 1.0;
            rx_bytes += ibytes;
            tx_bytes += obytes;
            if let Some(v) = ipkts {
                rx_packets += v;
            }
            if let Some(v) = opkts {
                tx_packets += v;
            }
            if let Some(v) = ierrs {
                rx_errors += v;
            }
            if let Some(v) = oerrs {
                tx_errors += v;
            }
        }
    }

    if interface_count <= 0.0 {
        return;
    }
    push_metric(
        out,
        "network",
        "interface_rows_non_loopback",
        interface_count,
        "count",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "rx_bytes_total",
        rx_bytes,
        "bytes",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "tx_bytes_total",
        tx_bytes,
        "bytes",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "rx_packets_total",
        rx_packets,
        "count",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "tx_packets_total",
        tx_packets,
        "count",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "rx_errors_total",
        rx_errors,
        "count",
        "netstat",
    );
    push_metric(
        out,
        "network",
        "tx_errors_total",
        tx_errors,
        "count",
        "netstat",
    );
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
    if let Some(uptime) = read_first_f64(Path::new("/proc/uptime")) {
        push_metric(out, "system", "uptime_seconds", uptime, "s", "procfs");
    }

    if let Ok(loadavg) = std::fs::read_to_string("/proc/loadavg")
        && let Some(tasks) = loadavg.split_whitespace().nth(3)
        && let Some((running, total)) = tasks.split_once('/')
    {
        if let Ok(running) = running.parse::<f64>() {
            push_metric(
                out,
                "scheduling",
                "runnable_tasks",
                running,
                "count",
                "procfs",
            );
        }
        if let Ok(total) = total.parse::<f64>() {
            push_metric(
                out,
                "scheduling",
                "sched_entities",
                total,
                "count",
                "procfs",
            );
        }
    }

    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        for line in mem.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let Some(raw_value) = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
            else {
                continue;
            };
            let parsed = match key {
                "MemTotal" => Some(("total_bytes", raw_value * 1024.0, "bytes")),
                "MemAvailable" => Some(("available_bytes", raw_value * 1024.0, "bytes")),
                "MemFree" => Some(("free_bytes", raw_value * 1024.0, "bytes")),
                "Buffers" => Some(("buffers_bytes", raw_value * 1024.0, "bytes")),
                "Cached" => Some(("cached_bytes", raw_value * 1024.0, "bytes")),
                "Active" => Some(("active_bytes", raw_value * 1024.0, "bytes")),
                "Inactive" => Some(("inactive_bytes", raw_value * 1024.0, "bytes")),
                "AnonPages" => Some(("anon_pages_bytes", raw_value * 1024.0, "bytes")),
                "Mapped" => Some(("mapped_bytes", raw_value * 1024.0, "bytes")),
                "Shmem" => Some(("shmem_bytes", raw_value * 1024.0, "bytes")),
                "Slab" => Some(("slab_bytes", raw_value * 1024.0, "bytes")),
                "SReclaimable" => Some(("slab_reclaimable_bytes", raw_value * 1024.0, "bytes")),
                "SUnreclaim" => Some(("slab_unreclaimable_bytes", raw_value * 1024.0, "bytes")),
                "KernelStack" => Some(("kernel_stack_bytes", raw_value * 1024.0, "bytes")),
                "PageTables" => Some(("page_tables_bytes", raw_value * 1024.0, "bytes")),
                "Dirty" => Some(("dirty_bytes", raw_value * 1024.0, "bytes")),
                "Writeback" => Some(("writeback_bytes", raw_value * 1024.0, "bytes")),
                "SwapTotal" => Some(("swap_total_bytes", raw_value * 1024.0, "bytes")),
                "SwapFree" => Some(("swap_free_bytes", raw_value * 1024.0, "bytes")),
                "SwapCached" => Some(("swap_cached_bytes", raw_value * 1024.0, "bytes")),
                "Committed_AS" => Some(("committed_as_bytes", raw_value * 1024.0, "bytes")),
                "CommitLimit" => Some(("commit_limit_bytes", raw_value * 1024.0, "bytes")),
                "HugePages_Total" => Some(("hugepages_total", raw_value, "count")),
                "HugePages_Free" => Some(("hugepages_free", raw_value, "count")),
                "HugePages_Rsvd" => Some(("hugepages_reserved", raw_value, "count")),
                "Hugepagesize" => Some(("hugepage_size_bytes", raw_value * 1024.0, "bytes")),
                _ => None,
            };
            if let Some((name, value, unit)) = parsed {
                push_metric(out, "memory", name, value, unit, "procfs");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_proc_stat_metrics(out: &mut Vec<TelemetryMetric>) {
    let Ok(raw) = std::fs::read_to_string("/proc/stat") else {
        return;
    };
    let clk_tck = linux_clk_tck();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("cpu ") {
            let parts: Vec<f64> = rest
                .split_whitespace()
                .filter_map(|s| s.parse::<f64>().ok())
                .collect();
            if parts.len() < 4 {
                continue;
            }
            let user = parts.first().copied().unwrap_or(0.0) / clk_tck;
            let nice = parts.get(1).copied().unwrap_or(0.0) / clk_tck;
            let system = parts.get(2).copied().unwrap_or(0.0) / clk_tck;
            let idle = parts.get(3).copied().unwrap_or(0.0) / clk_tck;
            let iowait = parts.get(4).copied().unwrap_or(0.0) / clk_tck;
            let irq = parts.get(5).copied().unwrap_or(0.0) / clk_tck;
            let softirq = parts.get(6).copied().unwrap_or(0.0) / clk_tck;
            let steal = parts.get(7).copied().unwrap_or(0.0) / clk_tck;
            let guest = parts.get(8).copied().unwrap_or(0.0) / clk_tck;
            let guest_nice = parts.get(9).copied().unwrap_or(0.0) / clk_tck;

            push_metric(
                out,
                "scheduling",
                "cpu_time_user_seconds",
                user,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_nice_seconds",
                nice,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_system_seconds",
                system,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_idle_seconds",
                idle,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_iowait_seconds",
                iowait,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_irq_seconds",
                irq,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_softirq_seconds",
                softirq,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_steal_seconds",
                steal,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_guest_seconds",
                guest,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_guest_nice_seconds",
                guest_nice,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_busy_seconds",
                user + nice + system + irq + softirq + steal,
                "s",
                "procfs",
            );
            push_metric(
                out,
                "scheduling",
                "cpu_time_idle_iowait_seconds",
                idle + iowait,
                "s",
                "procfs",
            );
            continue;
        }

        if let Some(rest) = line.strip_prefix("ctxt ")
            && let Ok(value) = rest.trim().parse::<f64>()
        {
            push_metric(
                out,
                "scheduling",
                "context_switches_total",
                value,
                "count",
                "procfs",
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("intr ")
            && let Some(value) = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
        {
            push_metric(
                out,
                "scheduling",
                "interrupts_total",
                value,
                "count",
                "procfs",
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("processes ")
            && let Ok(value) = rest.trim().parse::<f64>()
        {
            push_metric(
                out,
                "scheduling",
                "processes_forked_total",
                value,
                "count",
                "procfs",
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("procs_running ")
            && let Ok(value) = rest.trim().parse::<f64>()
        {
            push_metric(
                out,
                "scheduling",
                "processes_running",
                value,
                "count",
                "procfs",
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("procs_blocked ")
            && let Ok(value) = rest.trim().parse::<f64>()
        {
            push_metric(
                out,
                "scheduling",
                "processes_blocked",
                value,
                "count",
                "procfs",
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("btime ")
            && let Ok(value) = rest.trim().parse::<f64>()
        {
            push_metric(out, "system", "boot_unix_seconds", value, "s", "procfs");
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_pressure_metrics(out: &mut Vec<TelemetryMetric>) {
    for resource in ["cpu", "io", "memory"] {
        let path = Path::new("/proc/pressure").join(resource);
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in raw.lines() {
            parse_psi_line(resource, line, out);
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_entropy_metrics(out: &mut Vec<TelemetryMetric>) {
    if let Some(value) = read_first_f64(Path::new("/proc/sys/kernel/random/entropy_avail")) {
        push_metric(
            out,
            "entropy",
            "kernel_pool_available_bits",
            value,
            "bits",
            "procfs",
        );
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_network_metrics(out: &mut Vec<TelemetryMetric>) {
    let Ok(raw) = std::fs::read_to_string("/proc/net/dev") else {
        return;
    };
    let mut iface_count = 0.0;
    let mut rx_bytes = 0.0;
    let mut rx_packets = 0.0;
    let mut rx_errors = 0.0;
    let mut rx_drops = 0.0;
    let mut tx_bytes = 0.0;
    let mut tx_packets = 0.0;
    let mut tx_errors = 0.0;
    let mut tx_drops = 0.0;

    let mut rx_bytes_non_lo = 0.0;
    let mut tx_bytes_non_lo = 0.0;

    for line in raw.lines().skip(2) {
        let Some((iface_raw, stats_raw)) = line.split_once(':') else {
            continue;
        };
        let iface = iface_raw.trim();
        let fields: Vec<f64> = stats_raw
            .split_whitespace()
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        if fields.len() < 16 {
            continue;
        }
        iface_count += 1.0;
        rx_bytes += fields[0];
        rx_packets += fields[1];
        rx_errors += fields[2];
        rx_drops += fields[3];
        tx_bytes += fields[8];
        tx_packets += fields[9];
        tx_errors += fields[10];
        tx_drops += fields[11];
        if iface != "lo" {
            rx_bytes_non_lo += fields[0];
            tx_bytes_non_lo += fields[8];
        }
    }

    if iface_count <= 0.0 {
        return;
    }
    push_metric(
        out,
        "network",
        "interface_count",
        iface_count,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "rx_bytes_total",
        rx_bytes,
        "bytes",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "tx_bytes_total",
        tx_bytes,
        "bytes",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "rx_packets_total",
        rx_packets,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "tx_packets_total",
        tx_packets,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "rx_errors_total",
        rx_errors,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "tx_errors_total",
        tx_errors,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "rx_drops_total",
        rx_drops,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "tx_drops_total",
        tx_drops,
        "count",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "rx_bytes_non_loopback_total",
        rx_bytes_non_lo,
        "bytes",
        "procfs_netdev",
    );
    push_metric(
        out,
        "network",
        "tx_bytes_non_loopback_total",
        tx_bytes_non_lo,
        "bytes",
        "procfs_netdev",
    );
}

#[cfg(target_os = "linux")]
fn collect_linux_disk_metrics(out: &mut Vec<TelemetryMetric>) {
    let Ok(raw) = std::fs::read_to_string("/proc/diskstats") else {
        return;
    };
    let mut disk_count = 0.0;
    let mut read_ios = 0.0;
    let mut read_merged = 0.0;
    let mut read_sectors = 0.0;
    let mut read_time_ms = 0.0;
    let mut write_ios = 0.0;
    let mut write_merged = 0.0;
    let mut write_sectors = 0.0;
    let mut write_time_ms = 0.0;
    let mut io_in_progress = 0.0;
    let mut io_time_ms = 0.0;
    let mut weighted_io_time_ms = 0.0;

    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 {
            continue;
        }
        let name = parts[2];
        if !is_likely_disk_device(name) {
            continue;
        }
        let parsed: Vec<f64> = parts[3..14]
            .iter()
            .filter_map(|v| v.parse::<f64>().ok())
            .collect();
        if parsed.len() < 11 {
            continue;
        }
        disk_count += 1.0;
        read_ios += parsed[0];
        read_merged += parsed[1];
        read_sectors += parsed[2];
        read_time_ms += parsed[3];
        write_ios += parsed[4];
        write_merged += parsed[5];
        write_sectors += parsed[6];
        write_time_ms += parsed[7];
        io_in_progress += parsed[8];
        io_time_ms += parsed[9];
        weighted_io_time_ms += parsed[10];
    }

    if disk_count <= 0.0 {
        return;
    }
    push_metric(
        out,
        "disk",
        "device_count",
        disk_count,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "read_ios_total",
        read_ios,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "write_ios_total",
        write_ios,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "read_merged_total",
        read_merged,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "write_merged_total",
        write_merged,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "read_sectors_total",
        read_sectors,
        "sectors",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "write_sectors_total",
        write_sectors,
        "sectors",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "read_time_ms_total",
        read_time_ms,
        "ms",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "write_time_ms_total",
        write_time_ms,
        "ms",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "io_in_progress_total",
        io_in_progress,
        "count",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "io_time_ms_total",
        io_time_ms,
        "ms",
        "procfs_diskstats",
    );
    push_metric(
        out,
        "disk",
        "weighted_io_time_ms_total",
        weighted_io_time_ms,
        "ms",
        "procfs_diskstats",
    );
}

#[cfg(target_os = "linux")]
fn collect_linux_power_supply_metrics(out: &mut Vec<TelemetryMetric>) {
    let root = Path::new("/sys/class/power_supply");
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(raw_name) = dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let name = normalize_key(raw_name);
        if name.is_empty() {
            continue;
        }

        if let Some(v) = read_first_f64(&dir.join("online")) {
            push_metric(
                out,
                "power",
                format!("{name}.is_online"),
                v,
                "bool",
                "linux_power_supply",
            );
        }
        if let Some(v) = read_first_f64(&dir.join("present")) {
            push_metric(
                out,
                "power",
                format!("{name}.is_present"),
                v,
                "bool",
                "linux_power_supply",
            );
        }
        if let Some(v) = read_first_f64(&dir.join("capacity")) {
            push_metric(
                out,
                "power",
                format!("{name}.capacity_percent"),
                v,
                "percent",
                "linux_power_supply",
            );
        }
        parse_microunit_supply(out, &dir, &name, "voltage_now", "V", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "current_now", "A", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "power_now", "W", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "energy_now", "Wh", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "energy_full", "Wh", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "energy_full_design", "Wh", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "charge_now", "Ah", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "charge_full", "Ah", 1_000_000.0);
        parse_microunit_supply(out, &dir, &name, "charge_full_design", "Ah", 1_000_000.0);

        if let Some(v) = read_first_f64(&dir.join("temp")) {
            // power_supply temp is typically reported in 0.1 C.
            push_metric(
                out,
                "thermal",
                format!("{name}.temperature_c"),
                v / 10.0,
                "C",
                "linux_power_supply",
            );
        }
        parse_power_supply_state(out, &dir, &name);
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_freq_metrics(out: &mut Vec<TelemetryMetric>) {
    let root = Path::new("/sys/devices/system/cpu");
    let mut values_hz: Vec<(usize, f64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.starts_with("cpu") {
                continue;
            }
            let Some(cpu_id) = name
                .strip_prefix("cpu")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                continue;
            };
            let cpufreq_dir = path.join("cpufreq");
            if !cpufreq_dir.is_dir() {
                continue;
            }
            for key in ["scaling_cur_freq", "cpuinfo_cur_freq"] {
                if let Some(khz) = read_first_f64(&cpufreq_dir.join(key)) {
                    values_hz.push((cpu_id, khz * 1000.0));
                    break;
                }
            }
        }
    }

    if values_hz.is_empty() {
        return;
    }
    values_hz.sort_by_key(|(id, _)| *id);

    if let Some((_, cpu0_hz)) = values_hz
        .iter()
        .find(|(id, _)| *id == 0)
        .or_else(|| values_hz.first())
    {
        push_metric(out, "frequency", "cpu0_hz", *cpu0_hz, "Hz", "cpufreq");
    }

    let mut min_hz = f64::INFINITY;
    let mut max_hz = 0.0;
    let mut sum_hz = 0.0;
    for (_, hz) in &values_hz {
        min_hz = min_hz.min(*hz);
        max_hz = max_hz.max(*hz);
        sum_hz += *hz;
    }
    let avg_hz = sum_hz / values_hz.len() as f64;
    push_metric(out, "frequency", "cpu_hz_avg", avg_hz, "Hz", "cpufreq");
    push_metric(out, "frequency", "cpu_hz_min", min_hz, "Hz", "cpufreq");
    push_metric(out, "frequency", "cpu_hz_max", max_hz, "Hz", "cpufreq");
    push_metric(
        out,
        "frequency",
        "cpu_hz_sampled_cores",
        values_hz.len() as f64,
        "count",
        "cpufreq",
    );
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
    collect_macos_uptime_metrics(out);
    collect_macos_sysctl_metrics(out);
    collect_macos_cp_time_metrics(out);
    collect_macos_vm_stat_metrics(out);
    collect_macos_network_metrics(out);
}

/// Capture a best-effort telemetry snapshot.
pub fn collect_telemetry_snapshot() -> TelemetrySnapshot {
    let (load1, load5, load15) = collect_loadavg();
    let mut metrics = Vec::new();

    #[cfg(target_os = "linux")]
    {
        collect_linux_proc_metrics(&mut metrics);
        collect_linux_proc_stat_metrics(&mut metrics);
        collect_linux_pressure_metrics(&mut metrics);
        collect_linux_entropy_metrics(&mut metrics);
        collect_linux_network_metrics(&mut metrics);
        collect_linux_disk_metrics(&mut metrics);
        collect_linux_power_supply_metrics(&mut metrics);
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
