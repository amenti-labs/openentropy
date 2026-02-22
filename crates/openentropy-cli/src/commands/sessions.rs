//! `openentropy sessions` — list and analyze recorded sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use openentropy_core::analysis;
use openentropy_core::conditioning::min_entropy_estimate;
use openentropy_core::session::SessionMeta;

/// Run the sessions command.
pub fn run(
    session_path: Option<&str>,
    dir: &str,
    do_analyze: bool,
    do_entropy: bool,
    output: Option<&str>,
    include_telemetry: bool,
) {
    if let Some(path) = session_path {
        // Single session mode
        let session_dir = PathBuf::from(path);
        if !session_dir.join("session.json").exists() {
            eprintln!("Not a session directory: {path}");
            eprintln!("Expected session.json in that directory.");
            std::process::exit(1);
        }

        show_session(&session_dir);

        if do_analyze || do_entropy {
            analyze_session(&session_dir, do_entropy, output, include_telemetry);
        }
    } else {
        // List mode
        list_sessions(dir);
    }
}

/// List all sessions in a directory.
fn list_sessions(dir: &str) {
    let sessions_dir = PathBuf::from(dir);
    if !sessions_dir.exists() {
        println!("No sessions directory found at {dir}");
        println!("Record a session first: openentropy record --sources <name>");
        return;
    }

    let mut sessions: Vec<(PathBuf, SessionMeta)> = Vec::new();

    let entries = match std::fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to read {dir}: {e}");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let json_path = path.join("session.json");
        if !json_path.exists() {
            continue;
        }
        match std::fs::read_to_string(&json_path) {
            Ok(contents) => match serde_json::from_str::<SessionMeta>(&contents) {
                Ok(meta) => sessions.push((path, meta)),
                Err(_) => continue,
            },
            Err(_) => continue,
        }
    }

    if sessions.is_empty() {
        println!("No sessions found in {dir}/");
        println!("Record a session first: openentropy record --sources <name>");
        return;
    }

    // Sort by start time (newest first)
    sessions.sort_by(|a, b| b.1.started_at.cmp(&a.1.started_at));

    println!(
        "{:<50} {:<25} {:>8} {:>10}",
        "Session", "Sources", "Samples", "Duration"
    );
    println!("{}", "-".repeat(97));

    for (path, meta) in &sessions {
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let sources_str = if meta.sources.len() <= 2 {
            meta.sources.join(", ")
        } else {
            format!("{}, +{} more", meta.sources[0], meta.sources.len() - 1)
        };

        let duration_str = format_duration_ms(meta.duration_ms);

        // Show embedded analysis summary if available
        let analysis_hint = if meta.analysis.is_some() {
            " [analyzed]"
        } else {
            ""
        };

        println!(
            "{:<50} {:<25} {:>8} {:>10}{}",
            truncate(&dir_name, 50),
            truncate(&sources_str, 25),
            meta.total_samples,
            duration_str,
            analysis_hint,
        );
    }

    println!("\n{} session(s) in {dir}/", sessions.len());
    println!("Run: openentropy sessions <path> --analyze  for full statistical analysis");
}

/// Show summary info for a single session.
fn show_session(session_dir: &Path) {
    let meta = read_session_meta(session_dir);

    println!("Session: {}", session_dir.display());
    println!("  ID:           {}", meta.id);
    println!("  Started:      {}", meta.started_at);
    println!("  Ended:        {}", meta.ended_at);
    println!("  Duration:     {}", format_duration_ms(meta.duration_ms));
    println!("  Sources:      {}", meta.sources.join(", "));
    println!("  Conditioning: {}", meta.conditioning);
    println!("  Samples:      {}", meta.total_samples);
    println!(
        "  Machine:      {} ({}, {})",
        meta.machine.chip, meta.machine.arch, meta.machine.os
    );
    println!("  Version:      {}", meta.openentropy_version);

    if !meta.tags.is_empty() {
        let tags: Vec<String> = meta.tags.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        println!("  Tags:         {}", tags.join(", "));
    }
    if let Some(note) = &meta.note {
        println!("  Note:         {note}");
    }
    if let Some(telemetry) = &meta.telemetry_v1 {
        println!(
            "  Telemetry v1: {} ({:.1}s, {} metrics)",
            telemetry.model_id,
            telemetry.elapsed_ms as f64 / 1000.0,
            telemetry.end.metrics.len()
        );
    }

    // Per-source sample counts
    if meta.samples_per_source.len() > 1 {
        println!("\n  Per-source samples:");
        for (name, count) in &meta.samples_per_source {
            println!("    {name:<25} {count}");
        }
    }

    // Show embedded analysis if present
    if let Some(ref analysis_map) = meta.analysis {
        println!("\n  Embedded analysis (from recording):");
        println!(
            "    {:<25} {:>8} {:>8} {:>8} {:>6}",
            "Source", "Flatness", "Bias", "KS_p", "Stat?"
        );
        println!("    {}", "-".repeat(60));
        for (name, sa) in analysis_map {
            let stat = if sa.stationarity_is_stationary {
                "ok"
            } else {
                "!"
            };
            println!(
                "    {:<25} {:>7.3} {:>7.4} {:>7.4} {:>6}",
                name, sa.spectral_flatness, sa.bit_bias_max, sa.distribution_ks_p, stat,
            );
        }
    }

    println!();
}

/// Run full analysis on a recorded session's raw data.
fn analyze_session(
    session_dir: &Path,
    do_entropy: bool,
    output: Option<&str>,
    include_telemetry: bool,
) {
    let telemetry = super::telemetry::TelemetryCapture::start(include_telemetry);
    let meta = read_session_meta(session_dir);

    // Read raw_index.csv to group bytes by source
    let index_path = session_dir.join("raw_index.csv");
    let raw_path = session_dir.join("raw.bin");

    if !index_path.exists() || !raw_path.exists() {
        eprintln!("Missing raw.bin or raw_index.csv in session directory.");
        std::process::exit(1);
    }

    let raw_data = match std::fs::read(&raw_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read raw.bin: {e}");
            std::process::exit(1);
        }
    };

    let index_csv = match std::fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read raw_index.csv: {e}");
            std::process::exit(1);
        }
    };

    // Parse index and group raw bytes by source
    let mut source_bytes: HashMap<String, Vec<u8>> = HashMap::new();

    for line in index_csv.lines().skip(1) {
        // Format: offset,length,timestamp_ns,source
        let parts: Vec<&str> = line.splitn(4, ',').collect();
        if parts.len() < 4 {
            continue;
        }
        let offset: usize = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let length: usize = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let source = parts[3].to_string();

        if offset + length <= raw_data.len() {
            source_bytes
                .entry(source)
                .or_default()
                .extend_from_slice(&raw_data[offset..offset + length]);
        }
    }

    if source_bytes.is_empty() {
        println!("No data found in session.");
        return;
    }

    println!(
        "Analyzing {} source(s) from recorded session...\n",
        source_bytes.len()
    );

    let mut all_results = Vec::new();
    let mut all_data: Vec<(String, Vec<u8>)> = Vec::new();

    // Sort sources for consistent output
    let mut sources: Vec<(String, Vec<u8>)> = source_bytes.into_iter().collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, data) in sources {
        if data.is_empty() {
            println!("  {name}: (no data, skipped)");
            continue;
        }

        println!("  {name}: {} bytes", data.len());

        let result = analysis::full_analysis(&name, &data);
        print_source_report(&result);

        if do_entropy {
            let report = min_entropy_estimate(&data);
            let report_str = format!("{report}");
            println!("  ┌─ Min-Entropy Breakdown ({name})");
            for line in report_str.lines() {
                println!("  │ {line}");
            }
            println!("  └─");
        }

        all_data.push((name, data));
        all_results.push(result);
    }

    // Cross-correlation if multiple sources
    let cross_matrix = if all_data.len() >= 2 {
        Some(analysis::cross_correlation_matrix(&all_data))
    } else {
        None
    };

    if let Some(ref matrix) = cross_matrix {
        super::print_cross_correlation(matrix, all_data.len());
    }

    let telemetry_report = telemetry.finish();
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("sessions-analyze", window);
    }

    // JSON output
    if let Some(path) = output {
        let mut json = if let Some(matrix) = cross_matrix {
            serde_json::json!({
                "session": meta.id,
                "sources": all_results,
                "cross_correlation": matrix,
            })
        } else {
            serde_json::json!({
                "session": meta.id,
                "sources": all_results,
            })
        };
        if let Some(window) = telemetry_report {
            json["telemetry_v1"] = serde_json::json!(window);
        }

        super::write_json(&json, path, "Results");
    }
}

fn print_source_report(r: &analysis::SourceAnalysis) {
    println!();
    println!("  ┌─ {} ({} bytes)", r.source_name, r.sample_size);

    let ac = &r.autocorrelation;
    let ac_flag = if ac.max_abs_correlation > 0.05 {
        " !"
    } else {
        " ok"
    };
    println!(
        "  │ Autocorrelation:  max|r|={:.4} (lag {}), {}/{} violations{}",
        ac.max_abs_correlation,
        ac.max_abs_lag,
        ac.violations,
        ac.lags.len(),
        ac_flag
    );

    let sp = &r.spectral;
    println!(
        "  │ Spectral:         flatness={:.4} (1.0=white noise), dominant_freq={:.4}",
        sp.flatness, sp.dominant_frequency
    );

    let bb = &r.bit_bias;
    let bias_flag = if bb.has_significant_bias { " !" } else { " ok" };
    let bits_str: Vec<String> = bb
        .bit_probabilities
        .iter()
        .map(|&p| format!("{:.3}", p))
        .collect();
    println!(
        "  │ Bit bias:         [{}] overall={:.4}{bias_flag}",
        bits_str.join(" "),
        bb.overall_bias,
    );

    let d = &r.distribution;
    println!(
        "  │ Distribution:     mean={:.1} std={:.1} skew={:.3} kurt={:.3} KS_p={:.4}",
        d.mean, d.std_dev, d.skewness, d.kurtosis, d.ks_p_value
    );

    let st = &r.stationarity;
    let stat_flag = if st.is_stationary { "ok" } else { "!" };
    println!("  │ Stationarity*:    F={:.2} {stat_flag}", st.f_statistic);

    let ru = &r.runs;
    println!(
        "  │ Runs:             longest={} (expected {:.1}), total={} (expected {:.0})",
        ru.longest_run, ru.expected_longest_run, ru.total_runs, ru.expected_runs
    );
    println!("  │ *stationarity is a heuristic windowed F-test");

    println!("  └─");
}

fn read_session_meta(session_dir: &Path) -> SessionMeta {
    let json_path = session_dir.join("session.json");
    let contents = match std::fs::read_to_string(&json_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read session.json: {e}");
            std::process::exit(1);
        }
    };
    match serde_json::from_str(&contents) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to parse session.json: {e}");
            std::process::exit(1);
        }
    }
}

fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
