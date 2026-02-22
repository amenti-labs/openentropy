//! Shared telemetry helpers used by multiple CLI commands, plus the standalone
//! `openentropy telemetry` subcommand implementation.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use openentropy_core::{
    TelemetryMetricDelta, TelemetrySnapshot, TelemetryWindowReport, collect_telemetry_snapshot,
    collect_telemetry_window,
};

/// Telemetry capture lifecycle helper shared by command handlers.
pub struct TelemetryCapture {
    start: Option<TelemetrySnapshot>,
}

impl TelemetryCapture {
    /// Start capture only when enabled.
    pub fn start(enabled: bool) -> Self {
        Self {
            start: enabled.then(collect_telemetry_snapshot),
        }
    }

    /// Finish capture and return a start/end window.
    pub fn finish(self) -> Option<TelemetryWindowReport> {
        self.start.map(collect_telemetry_window)
    }

    /// Finish capture and print a standardized summary.
    pub fn finish_and_print(self, label: &str) -> Option<TelemetryWindowReport> {
        let report = self.finish();
        if let Some(ref window) = report {
            print_window_summary(label, window);
        }
        report
    }
}

fn domain_counts(snapshot: &TelemetrySnapshot) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in &snapshot.metrics {
        *counts.entry(m.domain.clone()).or_insert(0) += 1;
    }
    let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rows
}

fn format_value(value: f64, unit: &str) -> String {
    match unit {
        "bytes" => format_bytes(value),
        "Hz" => format_scaled(value, 1000.0, &["Hz", "kHz", "MHz", "GHz", "THz"]),
        "count" => format!("{value:.0}"),
        "s" => format!("{value:.2}s"),
        "ms" => format!("{value:.1}ms"),
        "us" => format!("{value:.1}us"),
        "pct" | "percent" => format!("{value:.2}%"),
        "bool" => {
            if value >= 0.5 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        _ => format!("{value:.3} {unit}"),
    }
}

fn format_delta(value: f64, unit: &str) -> String {
    match unit {
        "bytes" => {
            let sign = if value >= 0.0 { "+" } else { "" };
            format!("{sign}{}", format_bytes(value))
        }
        "Hz" => {
            let sign = if value >= 0.0 { "+" } else { "" };
            format!(
                "{sign}{}",
                format_scaled(value, 1000.0, &["Hz", "kHz", "MHz", "GHz", "THz"])
            )
        }
        "count" => format!("{value:+.0}"),
        "s" => format!("{value:+.3}s"),
        "ms" => format!("{value:+.2}ms"),
        "us" => format!("{value:+.1}us"),
        "pct" | "percent" => format!("{value:+.2}%"),
        _ => format!("{value:+.3} {unit}"),
    }
}

fn format_bytes(value: f64) -> String {
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let mut v = value.abs();
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut idx = 0usize;
    while v >= 1024.0 && idx < units.len() - 1 {
        v /= 1024.0;
        idx += 1;
    }
    format!("{sign}{v:.2}{}", units[idx])
}

fn format_scaled(value: f64, base: f64, units: &[&str]) -> String {
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let mut v = value.abs();
    let mut idx = 0usize;
    while v >= base && idx < units.len() - 1 {
        v /= base;
        idx += 1;
    }
    format!("{sign}{v:.2}{}", units[idx])
}

fn sample_metrics(snapshot: &TelemetrySnapshot, max: usize) -> Vec<String> {
    use std::collections::HashSet;

    let mut seen_domains: HashSet<String> = HashSet::new();
    let mut seen_keys: HashSet<(String, String, String)> = HashSet::new();
    let mut picked: Vec<String> = Vec::new();

    for metric in &snapshot.metrics {
        if seen_domains.insert(metric.domain.clone()) {
            seen_keys.insert((
                metric.domain.clone(),
                metric.name.clone(),
                metric.source.clone(),
            ));
            picked.push(format!(
                "{}.{}={}",
                metric.domain,
                metric.name,
                format_value(metric.value, &metric.unit)
            ));
            if picked.len() == max {
                return picked;
            }
        }
    }

    for metric in &snapshot.metrics {
        let key = (
            metric.domain.clone(),
            metric.name.clone(),
            metric.source.clone(),
        );
        if !seen_keys.insert(key) {
            continue;
        }
        picked.push(format!(
            "{}.{}={}",
            metric.domain,
            metric.name,
            format_value(metric.value, &metric.unit)
        ));
        if picked.len() == max {
            break;
        }
    }

    picked
}

fn top_deltas(window: &TelemetryWindowReport, max: usize) -> Vec<&TelemetryMetricDelta> {
    let mut rows: Vec<&TelemetryMetricDelta> = window
        .deltas
        .iter()
        .filter(|d| d.delta_value.is_finite() && d.delta_value.abs() > 0.0)
        .collect();
    rows.sort_by(|a, b| {
        b.delta_value
            .abs()
            .partial_cmp(&a.delta_value.abs())
            .unwrap_or(Ordering::Equal)
    });
    rows.truncate(max);
    rows
}

/// Print a concise point-in-time snapshot summary.
pub fn print_snapshot_summary(label: &str, snapshot: &TelemetrySnapshot) {
    println!("\n{:=<68}", "");
    println!("Telemetry ({label})");
    println!("{:=<68}", "");
    println!(
        "  host: {}/{}   cpu_count: {}",
        snapshot.os, snapshot.arch, snapshot.cpu_count
    );
    match (
        snapshot.loadavg_1m,
        snapshot.loadavg_5m,
        snapshot.loadavg_15m,
    ) {
        (Some(l1), Some(l5), Some(l15)) => {
            println!("  loadavg: 1m {:.2}  5m {:.2}  15m {:.2}", l1, l5, l15);
        }
        _ => println!("  loadavg: unavailable"),
    }
    let counts = domain_counts(snapshot);
    if counts.is_empty() {
        println!("  metrics: none available on this host");
    } else {
        let summary = counts
            .iter()
            .take(6)
            .map(|(domain, count)| format!("{domain}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  metrics: {} total [{}]", snapshot.metrics.len(), summary);
        let samples = sample_metrics(snapshot, 5);
        if !samples.is_empty() {
            println!("  sample:  {}", samples.join(", "));
        }
    }
}

/// Print a concise telemetry summary for CLI output.
pub fn print_window_summary(label: &str, window: &TelemetryWindowReport) {
    println!("\n{:=<68}", "");
    println!("Telemetry ({label})");
    println!("{:=<68}", "");
    println!(
        "  elapsed: {:.2}s   host: {}/{}   cpu_count: {}",
        window.elapsed_ms as f64 / 1000.0,
        window.end.os,
        window.end.arch,
        window.end.cpu_count
    );

    match (
        window.start.loadavg_1m,
        window.end.loadavg_1m,
        window.start.loadavg_5m,
        window.end.loadavg_5m,
        window.start.loadavg_15m,
        window.end.loadavg_15m,
    ) {
        (Some(s1), Some(e1), Some(s5), Some(e5), Some(s15), Some(e15)) => {
            println!(
                "  loadavg: 1m {:.2}->{:.2}  5m {:.2}->{:.2}  15m {:.2}->{:.2}",
                s1, e1, s5, e5, s15, e15
            );
        }
        _ => println!("  loadavg: unavailable"),
    }

    let counts = domain_counts(&window.end);
    if counts.is_empty() {
        println!("  metrics: none available on this host");
    } else {
        let summary = counts
            .iter()
            .take(6)
            .map(|(domain, count)| format!("{domain}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  metrics: {} total [{}]",
            window.end.metrics.len(),
            summary
        );
        let samples = sample_metrics(&window.end, 5);
        if !samples.is_empty() {
            println!("  sample:  {}", samples.join(", "));
        }
    }

    let deltas = top_deltas(window, 5);
    if !deltas.is_empty() {
        println!("  top changes:");
        for d in deltas {
            println!(
                "    {}.{}: {} -> {} (delta {})",
                d.domain,
                d.name,
                format_value(d.start_value, &d.unit),
                format_value(d.end_value, &d.unit),
                format_delta(d.delta_value, &d.unit),
            );
        }
    }
}

/// Capture and print a snapshot if telemetry is enabled.
pub fn print_snapshot_if_enabled(enabled: bool, label: &str) -> Option<TelemetrySnapshot> {
    if !enabled {
        return None;
    }
    let snapshot = collect_telemetry_snapshot();
    print_snapshot_summary(label, &snapshot);
    Some(snapshot)
}

/// Standalone telemetry command.
pub fn run(window_sec: f64, output_path: Option<&str>) {
    if !window_sec.is_finite() || window_sec < 0.0 {
        eprintln!("Invalid --window-sec value: {window_sec}. Expected a finite value >= 0.");
        std::process::exit(2);
    }
    let window_sec = window_sec.min(86_400.0);
    if window_sec > 0.0 {
        println!("Collecting telemetry window for {:.2}s...", window_sec);
        let start = collect_telemetry_snapshot();
        std::thread::sleep(Duration::from_secs_f64(window_sec));
        let report = collect_telemetry_window(start);
        print_window_summary("telemetry", &report);
        if let Some(path) = output_path {
            super::write_json(&report, path, "Telemetry window");
        }
    } else {
        let snapshot = collect_telemetry_snapshot();
        print_snapshot_summary("telemetry", &snapshot);
        if let Some(path) = output_path {
            super::write_json(&snapshot, path, "Telemetry snapshot");
        }
    }
}
