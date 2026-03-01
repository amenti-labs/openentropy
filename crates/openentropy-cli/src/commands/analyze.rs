use std::time::Instant;

use openentropy_core::analysis;
use openentropy_core::conditioning::{ConditioningMode, condition, min_entropy_estimate};

pub struct AnalyzeArgs {
    pub positional: Vec<String>,
    pub all: bool,
    pub profile: String,
    pub samples: Option<usize>,
    pub output: Option<String>,
    pub cross_correlation: bool,
    pub entropy: bool,
    pub conditioning: Option<String>,
    pub include_telemetry: bool,
    pub report: bool,
}

/// Resolved values after merging profile defaults with explicit flags.
struct Resolved {
    samples: usize,
    conditioning: String,
    entropy: bool,
    report: bool,
    cross_correlation: bool,
}

pub fn run(args: AnalyzeArgs) {
    let profile = super::AnalysisProfile::parse(&args.profile);
    let defaults = profile.analyze_defaults();

    let resolved = Resolved {
        samples: args.samples.unwrap_or(defaults.samples),
        conditioning: args
            .conditioning
            .as_deref()
            .unwrap_or(defaults.conditioning)
            .to_string(),
        entropy: args.entropy || defaults.entropy,
        report: args.report || defaults.report,
        cross_correlation: args.cross_correlation || defaults.cross_correlation,
    };

    println!(
        "Profile: {} | Samples: {} | Conditioning: {}",
        args.profile, resolved.samples, resolved.conditioning
    );

    if resolved.report {
        run_report(&args, &resolved);
    } else {
        run_analysis(&args, &resolved);
    }
}

// ---------------------------------------------------------------------------
// Forensic analysis path (default)
// ---------------------------------------------------------------------------

fn run_analysis(args: &AnalyzeArgs, resolved: &Resolved) {
    let telemetry = super::telemetry::TelemetryCapture::start(args.include_telemetry);
    let mode = super::parse_conditioning(&resolved.conditioning);

    let sources = super::resolve_sources(&args.positional, args.all);

    println!("Forensic analysis — spectral, bias, stationarity, runs, distribution");
    println!("(For throughput/stability ranking, use `bench` instead.)\n");
    println!(
        "Analyzing {} source(s), {} samples each...\n",
        sources.len(),
        resolved.samples,
    );

    let mut all_results = Vec::new();
    let mut all_data: Vec<(String, Vec<u8>)> = Vec::new();

    for source in &sources {
        let name = source.name().to_string();
        print!("  {name}...");
        let t0 = Instant::now();
        let mut data = source.collect(resolved.samples);
        if data.is_empty() {
            // Retry once — USB/hardware sources may need reconnection time.
            std::thread::sleep(std::time::Duration::from_secs(1));
            data = source.collect(resolved.samples);
        }
        let collect_time = t0.elapsed();

        if data.is_empty() {
            println!(" (no data, skipped)");
            continue;
        }

        let result = analysis::full_analysis(&name, &data);
        println!(" {:.2}s, {} bytes", collect_time.as_secs_f64(), data.len());

        print_source_forensics(&result);

        if resolved.entropy {
            let entropy_input = if mode == ConditioningMode::Raw {
                data.clone()
            } else {
                condition(&data, data.len(), mode)
            };
            let report = min_entropy_estimate(&entropy_input);
            let report_str = format!("{report}");
            println!(
                "  ├─ Min-Entropy Breakdown (conditioning: {}, {} bytes)",
                resolved.conditioning,
                entropy_input.len()
            );
            for line in report_str.lines() {
                println!("  │  {line}");
            }
            println!("  └─");
        }

        all_results.push(result);

        if resolved.cross_correlation {
            all_data.push((name, data));
        }
    }

    // Cross-correlation matrix.
    let cross_matrix = if resolved.cross_correlation && all_data.len() >= 2 {
        Some(analysis::cross_correlation_matrix(&all_data))
    } else {
        None
    };

    if let Some(ref matrix) = cross_matrix {
        super::print_cross_correlation(matrix, all_data.len());
    }

    let telemetry_report = telemetry.finish();
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("analyze", window);
    }

    // JSON output.
    if let Some(ref path) = args.output {
        let mut json = if let Some(matrix) = cross_matrix {
            serde_json::json!({
                "sources": all_results,
                "cross_correlation": matrix,
            })
        } else {
            serde_json::json!({ "sources": all_results })
        };
        if let Some(window) = telemetry_report {
            json["telemetry_v1"] = serde_json::json!(window);
        }

        super::write_json(&json, path, "Results");
    }
}

/// Print forensic test results for a single source as a compact table.
fn print_source_forensics(r: &analysis::SourceAnalysis) {
    let grade = openentropy_core::grade_min_entropy(r.min_entropy.max(0.0));

    println!();
    println!(
        "  {} — H={:.3} H∞={:.3} (grade {}) — {} bytes",
        r.source_name, r.shannon_entropy, r.min_entropy, grade, r.sample_size
    );

    // 7 forensic tests, each with a verdict and key metric.
    let ac = &r.autocorrelation;
    let sp = &r.spectral;
    let bb = &r.bit_bias;
    let d = &r.distribution;
    let st = &r.stationarity;
    let ru = &r.runs;

    let tests: Vec<(&str, &str, String)> = vec![
        (
            "Autocorrelation",
            verdict_autocorr(ac.max_abs_correlation),
            format!(
                "max|r|={:.4} at lag {}, {}/{} violations",
                ac.max_abs_correlation,
                ac.max_abs_lag,
                ac.violations,
                ac.lags.len()
            ),
        ),
        (
            "Spectral flatness",
            verdict_spectral(sp.flatness),
            format!(
                "flatness={:.4} (1.0=white noise), dominant freq={:.4}",
                sp.flatness, sp.dominant_frequency
            ),
        ),
        (
            "Bit bias",
            verdict_bias(bb.overall_bias, bb.has_significant_bias),
            format!(
                "overall={:.4}, bits=[{}]",
                bb.overall_bias,
                bb.bit_probabilities
                    .iter()
                    .map(|p| format!("{:.3}", p))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ),
        (
            "Distribution",
            verdict_distribution(d.ks_p_value),
            format!(
                "KS p={:.4}, mean={:.1}, skew={:.3}, kurt={:.3}",
                d.ks_p_value, d.mean, d.skewness, d.kurtosis
            ),
        ),
        (
            "Stationarity",
            verdict_stationarity(st.f_statistic, st.is_stationary),
            format!("F={:.2} ({} windows)", st.f_statistic, st.n_windows),
        ),
        (
            "Runs",
            verdict_runs(ru, r.sample_size),
            format!(
                "longest={} (expect {:.0}), total={} (expect {:.0})",
                ru.longest_run, ru.expected_longest_run, ru.total_runs, ru.expected_runs
            ),
        ),
    ];

    for (name, verdict, detail) in &tests {
        println!("    {:<20} {:>4}  {}", name, verdict, detail);
    }
    println!();
}

fn verdict_autocorr(max_abs: f64) -> &'static str {
    if max_abs > 0.15 {
        "FAIL"
    } else if max_abs > 0.05 {
        "WARN"
    } else {
        "PASS"
    }
}

fn verdict_spectral(flatness: f64) -> &'static str {
    if flatness < 0.5 {
        "FAIL"
    } else if flatness < 0.75 {
        "WARN"
    } else {
        "PASS"
    }
}

fn verdict_bias(overall: f64, has_significant: bool) -> &'static str {
    if overall > 0.02 {
        "FAIL"
    } else if has_significant {
        "WARN"
    } else {
        "PASS"
    }
}

fn verdict_distribution(ks_p: f64) -> &'static str {
    if ks_p < 0.001 {
        "FAIL"
    } else if ks_p < 0.01 {
        "WARN"
    } else {
        "PASS"
    }
}

fn verdict_stationarity(f_stat: f64, is_stationary: bool) -> &'static str {
    if f_stat > 3.0 {
        "FAIL"
    } else if !is_stationary {
        "WARN"
    } else {
        "PASS"
    }
}

fn verdict_runs(ru: &analysis::RunsResult, _sample_size: usize) -> &'static str {
    let longest_ratio = if ru.expected_longest_run > 0.0 {
        ru.longest_run as f64 / ru.expected_longest_run
    } else {
        1.0
    };
    let runs_dev = if ru.expected_runs > 0.0 {
        (ru.total_runs as f64 - ru.expected_runs).abs() / ru.expected_runs
    } else {
        0.0
    };
    if longest_ratio > 3.0 || runs_dev > 0.4 {
        "FAIL"
    } else if longest_ratio > 2.0 || runs_dev > 0.2 {
        "WARN"
    } else {
        "PASS"
    }
}

// ---------------------------------------------------------------------------
// NIST test battery path (--report)
// ---------------------------------------------------------------------------

fn run_report(args: &AnalyzeArgs, resolved: &Resolved) {
    let telemetry = super::telemetry::TelemetryCapture::start(args.include_telemetry);
    let mode = super::parse_conditioning(&resolved.conditioning);

    let sources = super::resolve_sources(&args.positional, args.all);

    if sources.is_empty() {
        eprintln!("No sources matched filter.");
        std::process::exit(1);
    }

    println!("NIST randomness test battery — formal pass/fail with p-values");
    println!("(For throughput/stability ranking, use `bench` instead.)\n");
    println!(
        "Testing {} source(s), {} samples each...\n",
        sources.len(),
        resolved.samples
    );

    let mut all_results = Vec::new();

    for src in &sources {
        let info = src.info();
        print!("  {}...", info.name);

        let t0 = Instant::now();
        let mut raw_data = src.collect(resolved.samples);
        if raw_data.is_empty() {
            // Retry once — USB/hardware sources may need reconnection time.
            std::thread::sleep(std::time::Duration::from_secs(1));
            raw_data = src.collect(resolved.samples);
        }
        let data = condition(&raw_data, raw_data.len(), mode);
        print!(" {} bytes", data.len());

        if data.is_empty() {
            println!(" (no data)");
            continue;
        }

        let results = openentropy_tests::run_all_tests(&data);
        let elapsed = t0.elapsed().as_secs_f64();
        let score = openentropy_tests::calculate_quality_score(&results);
        let passed = results.iter().filter(|r| r.passed).count();

        println!(
            " -> {:.0}/100 ({}/{} passed) [{:.1}s]",
            score,
            passed,
            results.len(),
            elapsed
        );

        all_results.push((info.name.to_string(), data, results));
    }

    if all_results.is_empty() {
        eprintln!("No sources produced data.");
        std::process::exit(1);
    }

    // Summary table
    println!("\n{}", "=".repeat(60));
    println!(
        "{:<25} {:>6} {:>6} {:>8}",
        "Source", "Score", "Grade", "Pass"
    );
    println!("{}", "-".repeat(60));

    let mut sorted_indices: Vec<usize> = (0..all_results.len()).collect();
    sorted_indices.sort_by(|&a, &b| {
        let sa = openentropy_tests::calculate_quality_score(&all_results[a].2);
        let sb = openentropy_tests::calculate_quality_score(&all_results[b].2);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    for &idx in &sorted_indices {
        let (ref name, _, ref results) = all_results[idx];
        let score = openentropy_tests::calculate_quality_score(results);
        let grade = if score >= 80.0 {
            'A'
        } else if score >= 60.0 {
            'B'
        } else if score >= 40.0 {
            'C'
        } else if score >= 20.0 {
            'D'
        } else {
            'F'
        };
        let passed = results.iter().filter(|r| r.passed).count();
        println!(
            "  {:<23} {:>5.1} {:>6} {:>4}/{}",
            name,
            score,
            grade,
            passed,
            results.len()
        );
    }

    let telemetry_report = telemetry.finish_and_print("analyze --report");

    // Markdown output.
    if let Some(ref path) = args.output {
        let report = generate_markdown_report(&all_results, telemetry_report.as_ref());
        if let Err(e) = std::fs::write(path, &report) {
            eprintln!("Failed to write report to {path}: {e}");
        } else {
            println!("\nReport saved to: {path}");
        }
    }
}

fn generate_markdown_report(
    results: &[(String, Vec<u8>, Vec<openentropy_tests::TestResult>)],
    telemetry: Option<&openentropy_core::TelemetryWindowReport>,
) -> String {
    let mut report = String::new();
    report.push_str("# OpenEntropy — NIST Randomness Test Report\n\n");
    report.push_str(&format!(
        "Generated: Unix timestamp: {}\n\n",
        super::unix_timestamp_now()
    ));
    if let Some(t) = telemetry {
        report.push_str("## Telemetry Context (`telemetry_v1`)\n\n");
        report.push_str(&format!(
            "- Elapsed: {:.2}s\n- Host: {}/{}\n- CPU count: {}\n- Metrics observed: {}\n\n",
            t.elapsed_ms as f64 / 1000.0,
            t.end.os,
            t.end.arch,
            t.end.cpu_count,
            t.end.metrics.len()
        ));
    }

    for (name, data, tests) in results {
        let score = openentropy_tests::calculate_quality_score(tests);
        let passed = tests.iter().filter(|r| r.passed).count();
        report.push_str(&format!("## {name}\n\n"));
        report.push_str(&format!(
            "- Samples: {} bytes\n- Score: {:.1}/100\n- Passed: {}/{}\n\n",
            data.len(),
            score,
            passed,
            tests.len()
        ));

        report.push_str("| Test | P | Grade | p-value | Statistic | Details |\n");
        report.push_str("|------|---|-------|---------|-----------|--------|\n");
        for t in tests {
            let ok = if t.passed { "Y" } else { "N" };
            let pval = t
                .p_value
                .map(|p| format!("{p:.6}"))
                .unwrap_or_else(|| "—".to_string());
            report.push_str(&format!(
                "| {} | {} | {} | {} | {:.4} | {} |\n",
                t.name, ok, t.grade, pval, t.statistic, t.details
            ));
        }
        report.push_str("\n---\n\n");
    }

    report
}
