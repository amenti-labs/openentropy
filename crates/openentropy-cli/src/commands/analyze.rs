use std::time::Instant;

use openentropy_core::analysis;
use openentropy_core::conditioning::{ConditioningMode, condition, min_entropy_estimate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnalyzeView {
    Summary,
    Detailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnalyzeStatus {
    Good,
    Warning,
    Critical,
}

struct SourceInterpretation {
    status: AnalyzeStatus,
    findings: Vec<String>,
    strengths: Vec<String>,
    meaning: &'static str,
}

pub struct AnalyzeCommandConfig<'a> {
    pub source_filter: Option<&'a str>,
    pub output_path: Option<&'a str>,
    pub samples: usize,
    pub cross_correlation: bool,
    pub entropy: bool,
    pub conditioning: &'a str,
    pub view: &'a str,
    pub include_telemetry: bool,
    pub report: bool,
}

pub fn run(cfg: AnalyzeCommandConfig<'_>) {
    if cfg.report {
        if cfg.entropy || cfg.cross_correlation || cfg.view != "summary" {
            eprintln!(
                "Note: --report mode runs the NIST test battery; \
                 --entropy, --cross-correlation, and --view are ignored."
            );
        }
        run_report(&cfg);
    } else {
        run_analysis(&cfg);
    }
}

// ---------------------------------------------------------------------------
// Statistical analysis path (default)
// ---------------------------------------------------------------------------

fn run_analysis(cfg: &AnalyzeCommandConfig<'_>) {
    let telemetry = super::telemetry::TelemetryCapture::start(cfg.include_telemetry);
    let all_sources = openentropy_core::platform::detect_available_sources();
    let mode = super::parse_conditioning(cfg.conditioning);
    let view = AnalyzeView::parse(cfg.view);

    let sources: Vec<_> = super::filter_sources(all_sources, cfg.source_filter);

    if sources.is_empty() {
        eprintln!("No sources matched filter.");
        std::process::exit(1);
    }

    println!(
        "Analyzing {} source(s), {} samples each (view: {})...\n",
        sources.len(),
        cfg.samples,
        view.as_str()
    );

    let mut all_results = Vec::new();
    let mut all_data: Vec<(String, Vec<u8>)> = Vec::new();
    let mut status_counts = [0usize; 3];

    for source in &sources {
        let name = source.name().to_string();
        print!("  {name}...");
        let t0 = Instant::now();
        let data = source.collect(cfg.samples);
        let collect_time = t0.elapsed();

        if data.is_empty() {
            println!(" (no data, skipped)");
            continue;
        }

        let result = analysis::full_analysis(&name, &data);
        println!(" {:.2}s, {} bytes", collect_time.as_secs_f64(), data.len());

        let interpretation = interpret_source(&result);
        match interpretation.status {
            AnalyzeStatus::Good => status_counts[0] += 1,
            AnalyzeStatus::Warning => status_counts[1] += 1,
            AnalyzeStatus::Critical => status_counts[2] += 1,
        }

        match view {
            AnalyzeView::Summary => print_source_summary(&result, &interpretation),
            AnalyzeView::Detailed => print_source_detailed(&result, &interpretation),
        }

        // Min-entropy breakdown (MCV primary + diagnostic estimators)
        if cfg.entropy {
            let entropy_input = if mode == ConditioningMode::Raw {
                data.clone()
            } else {
                condition(&data, data.len(), mode)
            };
            let report = min_entropy_estimate(&entropy_input);
            let report_str = format!("{report}");
            println!(
                "  ┌─ Min-Entropy Breakdown ({name}, conditioning: {}, {} bytes)",
                cfg.conditioning,
                entropy_input.len()
            );
            for line in report_str.lines() {
                println!("  │ {line}");
            }
            println!("  └─");
        }

        all_results.push(result);

        if cfg.cross_correlation {
            all_data.push((name, data));
        }
    }

    println!("\n{:=<68}", "");
    println!(
        "Analysis Summary: {} good, {} warning, {} critical",
        status_counts[0], status_counts[1], status_counts[2]
    );
    println!("{:=<68}", "");
    if status_counts[2] > 0 {
        println!("Recommendation: exclude critical sources from default pool selection.");
    } else if status_counts[1] > 0 {
        println!("Recommendation: warning sources can remain in pool with strong conditioning.");
    } else {
        println!("Recommendation: all analyzed sources are good candidates for pool inclusion.");
    }

    // Cross-correlation matrix.
    let cross_matrix = if cfg.cross_correlation && all_data.len() >= 2 {
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
    if let Some(path) = cfg.output_path {
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

// ---------------------------------------------------------------------------
// NIST-inspired test battery path (--report)
// ---------------------------------------------------------------------------

fn run_report(cfg: &AnalyzeCommandConfig<'_>) {
    let telemetry = super::telemetry::TelemetryCapture::start(cfg.include_telemetry);
    let mode = super::parse_conditioning(cfg.conditioning);
    let all_sources = openentropy_core::platform::detect_available_sources();

    let sources: Vec<_> = super::filter_sources(all_sources, cfg.source_filter);

    if sources.is_empty() {
        eprintln!("No sources matched filter.");
        std::process::exit(1);
    }

    println!(
        "Running NIST test battery on {} source(s), {} samples each...\n",
        sources.len(),
        cfg.samples
    );

    let mut all_results = Vec::new();

    for src in &sources {
        let info = src.info();
        print!("  Collecting from {}...", info.name);

        let t0 = Instant::now();
        let raw_data = src.collect(cfg.samples);
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
    if let Some(path) = cfg.output_path {
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

fn print_source_summary(r: &analysis::SourceAnalysis, i: &SourceInterpretation) {
    println!();
    println!("  ┌─ {} ({} bytes)", r.source_name, r.sample_size);
    println!(
        "  │ Entropy: H={:.3} H∞={:.3} (grade {})",
        r.shannon_entropy,
        r.min_entropy,
        openentropy_core::grade_min_entropy(r.min_entropy.max(0.0))
    );
    println!(
        "  │ Status: {} ({} finding(s))",
        i.status.as_str(),
        i.findings.len()
    );

    if i.findings.is_empty() {
        println!("  │ Findings: none");
    } else {
        for finding in &i.findings {
            println!("  │ Finding: {finding}");
        }
    }

    if !i.strengths.is_empty() {
        for strength in &i.strengths {
            println!("  │ Strength: {strength}");
        }
    }

    println!("  │ What this means: {}", i.meaning);
    println!("  └─");
}

fn print_source_detailed(r: &analysis::SourceAnalysis, i: &SourceInterpretation) {
    println!();
    println!("  ┌─ {} ({} bytes)", r.source_name, r.sample_size);
    println!(
        "  │ Entropy:         H={:.4} H∞={:.4} (grade {})",
        r.shannon_entropy,
        r.min_entropy,
        openentropy_core::grade_min_entropy(r.min_entropy.max(0.0))
    );
    println!("  │ Status: {}", i.status.as_str());

    // Autocorrelation
    let ac = &r.autocorrelation;
    let ac_flag = if ac.max_abs_correlation > 0.15 {
        " critical"
    } else if ac.max_abs_correlation > 0.05 {
        " warning"
    } else {
        " ok"
    };
    println!(
        "  │ Autocorrelation:  max|r|={:.4} (lag {}), {}/{} violations [{}]",
        ac.max_abs_correlation,
        ac.max_abs_lag,
        ac.violations,
        ac.lags.len(),
        ac_flag
    );

    // Spectral
    let sp = &r.spectral;
    let sp_flag = if sp.flatness < 0.5 {
        "critical"
    } else if sp.flatness < 0.75 {
        "warning"
    } else {
        "ok"
    };
    println!(
        "  │ Spectral:         flatness={:.4} (1.0=white noise), dominant_freq={:.4} [{}]",
        sp.flatness, sp.dominant_frequency, sp_flag
    );

    // Bit bias
    let bb = &r.bit_bias;
    let bias_flag = if bb.overall_bias > 0.02 {
        "critical"
    } else if bb.has_significant_bias {
        "warning"
    } else {
        "ok"
    };
    let bits_str: Vec<String> = bb
        .bit_probabilities
        .iter()
        .map(|&p| format!("{:.3}", p))
        .collect();
    println!(
        "  │ Bit bias:         [{}] overall={:.4} [{}]",
        bits_str.join(" "),
        bb.overall_bias,
        bias_flag
    );

    // Distribution
    let d = &r.distribution;
    let dist_flag = if d.ks_p_value < 0.001 {
        "critical"
    } else if d.ks_p_value < 0.01 {
        "warning"
    } else {
        "ok"
    };
    println!(
        "  │ Distribution:     mean={:.1} std={:.1} skew={:.3} kurt={:.3} KS_p={:.4} [{}]",
        d.mean, d.std_dev, d.skewness, d.kurtosis, d.ks_p_value, dist_flag
    );

    // Stationarity
    let st = &r.stationarity;
    let stat_flag = if st.f_statistic > 3.0 {
        "critical"
    } else if st.is_stationary {
        "ok"
    } else {
        "warning"
    };
    println!(
        "  │ Stationarity*:    F={:.2} [{}]",
        st.f_statistic, stat_flag
    );

    // Runs
    let ru = &r.runs;
    let longest_ratio = if ru.expected_longest_run > 0.0 {
        ru.longest_run as f64 / ru.expected_longest_run
    } else {
        1.0
    };
    let runs_dev_ratio = if ru.expected_runs > 0.0 {
        ((ru.total_runs as f64 - ru.expected_runs).abs() / ru.expected_runs).abs()
    } else {
        0.0
    };
    let runs_flag = if longest_ratio > 3.0 || runs_dev_ratio > 0.4 {
        "critical"
    } else if longest_ratio > 2.0 || runs_dev_ratio > 0.2 {
        "warning"
    } else {
        "ok"
    };
    println!(
        "  │ Runs:             longest={} (expected {:.1}), total={} (expected {:.0}) [{}]",
        ru.longest_run, ru.expected_longest_run, ru.total_runs, ru.expected_runs, runs_flag
    );
    println!("  │ *stationarity is a heuristic windowed F-test");
    println!("  │ What this means: {}", i.meaning);

    println!("  └─");
}

fn interpret_source(r: &analysis::SourceAnalysis) -> SourceInterpretation {
    let mut warnings = 0usize;
    let mut criticals = 0usize;
    let mut findings = Vec::new();
    let mut strengths = Vec::new();

    // Entropy gate: min-entropy is the most fundamental quality indicator.
    let min_h = r.min_entropy;
    if min_h < 1.0 {
        criticals += 1;
        findings.push(format!(
            "Very low min-entropy (H∞={min_h:.3}); source provides negligible randomness."
        ));
    } else if min_h < 4.0 {
        warnings += 1;
        findings.push(format!(
            "Below-average min-entropy (H∞={min_h:.3}); requires strong conditioning."
        ));
    } else {
        strengths.push(format!("Min-entropy is healthy (H∞={min_h:.3})."));
    }

    let ac = r.autocorrelation.max_abs_correlation;
    if ac > 0.15 {
        criticals += 1;
        findings.push(format!(
            "High autocorrelation (max|r|={ac:.3}) indicates strong sequential dependence."
        ));
    } else if ac > 0.05 {
        warnings += 1;
        findings.push(format!(
            "Autocorrelation above heuristic threshold (max|r|={ac:.3})."
        ));
    } else {
        strengths.push(format!("Low autocorrelation (max|r|={ac:.3})."));
    }

    let flatness = r.spectral.flatness;
    if flatness < 0.5 {
        criticals += 1;
        findings.push(format!(
            "Low spectral flatness ({flatness:.3}) suggests tonal structure."
        ));
    } else if flatness < 0.75 {
        warnings += 1;
        findings.push(format!(
            "Spectral flatness ({flatness:.3}) is below ideal white-noise range."
        ));
    } else {
        strengths.push(format!("Spectral flatness is healthy ({flatness:.3})."));
    }

    let bias = r.bit_bias.overall_bias;
    if bias > 0.02 {
        criticals += 1;
        findings.push(format!("Significant overall bit bias ({bias:.4})."));
    } else if bias > 0.01 {
        warnings += 1;
        findings.push(format!("Noticeable bit bias ({bias:.4})."));
    } else {
        strengths.push(format!("Bit bias is low ({bias:.4})."));
    }

    let ks_p = r.distribution.ks_p_value;
    if ks_p < 0.001 {
        criticals += 1;
        findings.push(format!("Distribution KS p-value is very low ({ks_p:.4})."));
    } else if ks_p < 0.01 {
        warnings += 1;
        findings.push(format!("Distribution KS p-value is low ({ks_p:.4})."));
    } else {
        strengths.push(format!(
            "Distribution check is acceptable (KS p={ks_p:.4})."
        ));
    }

    let f_stat = r.stationarity.f_statistic;
    if f_stat > 3.0 {
        criticals += 1;
        findings.push(format!(
            "Strong non-stationarity signal (windowed F={f_stat:.2})."
        ));
    } else if !r.stationarity.is_stationary {
        warnings += 1;
        findings.push(format!(
            "Potential non-stationarity in windowed test (F={f_stat:.2})."
        ));
    } else {
        strengths.push(format!("Stationarity heuristic is stable (F={f_stat:.2})."));
    }

    let longest_ratio = if r.runs.expected_longest_run > 0.0 {
        r.runs.longest_run as f64 / r.runs.expected_longest_run
    } else {
        1.0
    };
    let runs_dev_ratio = if r.runs.expected_runs > 0.0 {
        ((r.runs.total_runs as f64 - r.runs.expected_runs).abs() / r.runs.expected_runs).abs()
    } else {
        0.0
    };
    if longest_ratio > 3.0 || runs_dev_ratio > 0.4 {
        criticals += 1;
        findings.push(format!(
            "Runs pattern is far from random expectation (longest ratio={longest_ratio:.2}, total deviation={:.1}%).",
            runs_dev_ratio * 100.0
        ));
    } else if longest_ratio > 2.0 || runs_dev_ratio > 0.2 {
        warnings += 1;
        findings.push(format!(
            "Runs pattern moderately deviates from expectation (longest ratio={longest_ratio:.2}, total deviation={:.1}%).",
            runs_dev_ratio * 100.0
        ));
    } else {
        strengths.push("Runs behavior is close to random expectation.".to_string());
    }

    let (status, meaning) = if criticals > 0 {
        (
            AnalyzeStatus::Critical,
            "High-risk source for standalone use; exclude from default pool or require strong conditioning.",
        )
    } else if warnings > 0 {
        (
            AnalyzeStatus::Warning,
            "Usable in a multi-source pool with strong conditioning and monitoring.",
        )
    } else {
        (
            AnalyzeStatus::Good,
            "Good standalone characteristics and strong candidate for pooled entropy collection.",
        )
    };

    SourceInterpretation {
        status,
        findings,
        strengths,
        meaning,
    }
}

impl AnalyzeView {
    fn parse(s: &str) -> Self {
        match s {
            "detailed" => Self::Detailed,
            _ => Self::Summary,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Detailed => "detailed",
        }
    }
}

impl AnalyzeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Good => "GOOD",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
}
