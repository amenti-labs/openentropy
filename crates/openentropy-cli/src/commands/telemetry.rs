use std::collections::HashMap;
use std::time::Duration;

use openentropy_core::{
    QUANTUM_PROXY_MODEL_ID, QUANTUM_PROXY_MODEL_VERSION, QuantumBatchReport, TelemetrySnapshot,
    TelemetryWindowReport, collect_telemetry_snapshot, collect_telemetry_window,
};
use serde::Serialize;
use serde_json::{Map, Value};

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

/// Build a versioned model object with `model_id`, `model_version`, and `report`.
pub fn versioned_report<T: Serialize>(model_id: &str, model_version: u32, report: &T) -> Value {
    serde_json::json!({
        "model_id": model_id,
        "model_version": model_version,
        "report": report,
    })
}

/// Insert an arbitrary model payload into an `experimental` map if present.
pub fn insert_experimental_model<T: Serialize>(
    experimental: &mut Map<String, Value>,
    key: &str,
    model: Option<&T>,
) {
    let Some(model) = model else {
        return;
    };
    match serde_json::to_value(model) {
        Ok(value) => {
            experimental.insert(key.to_string(), value);
        }
        Err(e) => {
            eprintln!("Warning: failed to serialize experimental model '{key}': {e}");
        }
    }
}

/// Insert `quantum_proxy_v3` into an `experimental` map.
pub fn insert_quantum_proxy_report(
    experimental: &mut Map<String, Value>,
    report: Option<&QuantumBatchReport>,
) {
    let Some(report) = report else {
        return;
    };
    experimental.insert(
        "quantum_proxy_v3".to_string(),
        versioned_report(QUANTUM_PROXY_MODEL_ID, QUANTUM_PROXY_MODEL_VERSION, report),
    );
}

/// Insert `telemetry_v1` window report into an `experimental` map.
pub fn insert_telemetry_window(
    experimental: &mut Map<String, Value>,
    report: Option<&TelemetryWindowReport>,
) {
    insert_experimental_model(experimental, "telemetry_v1", report);
}

/// Convert a map into optional JSON value.
pub fn finalize_experimental(experimental: Map<String, Value>) -> Option<Value> {
    if experimental.is_empty() {
        None
    } else {
        Some(Value::Object(experimental))
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
            match serde_json::to_string_pretty(&report) {
                Ok(json) => match std::fs::write(path, json) {
                    Ok(()) => println!("\nTelemetry window written to {path}"),
                    Err(e) => eprintln!("Failed to write telemetry to {path}: {e}"),
                },
                Err(e) => eprintln!("Failed to serialize telemetry report: {e}"),
            }
        }
    } else {
        let snapshot = collect_telemetry_snapshot();
        print_snapshot_summary("telemetry", &snapshot);
        if let Some(path) = output_path {
            match serde_json::to_string_pretty(&snapshot) {
                Ok(json) => match std::fs::write(path, json) {
                    Ok(()) => println!("\nTelemetry snapshot written to {path}"),
                    Err(e) => eprintln!("Failed to write telemetry to {path}: {e}"),
                },
                Err(e) => eprintln!("Failed to serialize telemetry snapshot: {e}"),
            }
        }
    }
}
