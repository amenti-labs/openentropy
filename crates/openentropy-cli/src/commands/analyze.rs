use std::time::Instant;

use openentropy_core::analysis;
use openentropy_core::conditioning::{ConditioningMode, condition, min_entropy_estimate};
use openentropy_core::metrics::experimental::quantum_proxy_v3::{
    MODEL_ID as QUANTUM_MODEL_ID, QuantumAssessmentConfig, QuantumBatchReport, QuantumSourceInput,
    TelemetryConfoundConfig, assess_batch_from_streams_with_telemetry,
    estimate_stress_sensitivity_from_streams, quality_factor_from_analysis,
};

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
    pub quantum_ratio: bool,
    pub include_telemetry: bool,
}

pub fn run(cfg: AnalyzeCommandConfig<'_>) {
    let telemetry = super::telemetry::TelemetryCapture::start(cfg.include_telemetry);
    let all_sources = openentropy_core::platform::detect_available_sources();
    let mode = super::parse_conditioning(cfg.conditioning);
    let view = AnalyzeView::parse(cfg.view);

    let sources: Vec<_> = if let Some(filter) = cfg.source_filter {
        if filter == "all" {
            all_sources
        } else {
            let names: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
            all_sources
                .into_iter()
                .filter(|s| {
                    let src_name = s.name().to_lowercase();
                    names.iter().any(|n| src_name.contains(&n.to_lowercase()))
                })
                .collect()
        }
    } else {
        // Default: fast sources only.
        let fast = super::FAST_SOURCES;
        all_sources
            .into_iter()
            .filter(|s| fast.contains(&s.name()))
            .collect()
    };

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
    let need_pair_metrics = cfg.cross_correlation || cfg.quantum_ratio;
    let mut all_data: Vec<(String, Vec<u8>)> = Vec::new();
    let mut quantum_inputs = Vec::<QuantumSourceInput>::new();
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
            // Use the same sampled dataset we just analyzed to keep reports
            // comparable. Conditioning (if selected) is applied to this sample,
            // not to a separately recollected stream.
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

        let category = source.info().category;
        let quality_factor = quality_factor_from_analysis(&result);
        let min_entropy = if cfg.quantum_ratio {
            openentropy_core::metrics::standard::EntropyMeasurements::from_bytes(
                &data,
                Some(collect_time.as_secs_f64()),
            )
            .min_entropy
        } else {
            0.0
        };
        all_results.push(result);

        if need_pair_metrics {
            all_data.push((name.clone(), data));
        }
        if cfg.quantum_ratio {
            quantum_inputs.push(QuantumSourceInput {
                name,
                category: Some(category),
                min_entropy_bits: min_entropy,
                quality_factor,
                stress_sensitivity: 0.0,
                physics_prior_override: None,
            });
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
    if cfg.cross_correlation && all_data.len() >= 2 {
        println!("\n{:=<68}", "");
        println!("Cross-Correlation Matrix ({} sources)", all_data.len());
        println!("{:=<68}", "");

        let matrix = analysis::cross_correlation_matrix(&all_data);

        if matrix.flagged_count > 0 {
            println!("\n  {} pair(s) with |r| > 0.3:\n", matrix.flagged_count);
        }

        for pair in &matrix.pairs {
            let flag = if pair.flagged { " !" } else { "" };
            if pair.flagged || pair.correlation.abs() > 0.1 {
                println!(
                    "  {:20} x {:20}  r = {:+.4}{}",
                    pair.source_a, pair.source_b, pair.correlation, flag
                );
            }
        }

        if matrix.flagged_count == 0 {
            println!("  All pairs below r=0.3 threshold — no strong linear correlation detected.");
        }
    }

    let telemetry_report = telemetry.finish();
    let quantum_report: Option<QuantumBatchReport> = if cfg.quantum_ratio {
        let qcfg = QuantumAssessmentConfig::default();
        let stress_map = estimate_stress_sensitivity_from_streams(&all_data, qcfg);
        for input in &mut quantum_inputs {
            input.stress_sensitivity = stress_map.get(&input.name).copied().unwrap_or(0.0);
        }
        let report = assess_batch_from_streams_with_telemetry(
            &quantum_inputs,
            &all_data,
            qcfg,
            64,
            telemetry_report.as_ref(),
            TelemetryConfoundConfig::default(),
        );
        println!("\n{:=<68}", "");
        println!("Quantum:Classical Contribution Proxy ({QUANTUM_MODEL_ID})");
        println!("{:=<68}", "");
        println!(
            "  Aggregate Q:C = {:.3}:{:.3} (Q fraction {:.1}%)",
            report.aggregate.quantum_bits,
            report.aggregate.classical_bits,
            report.aggregate.quantum_fraction * 100.0
        );
        for row in &report.sources {
            println!(
                "  {:20} q={:.3} q_bits={:.3} (prior={:.2} quality={:.2} coupling={:.2})",
                row.name,
                row.quantum_score,
                row.quantum_min_entropy_bits,
                row.physics_prior,
                row.quality_factor,
                row.coupling_penalty
            );
        }
        Some(report)
    } else {
        None
    };
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("analyze", window);
    }

    // JSON output.
    if let Some(path) = cfg.output_path {
        let cross = if cfg.cross_correlation && all_data.len() >= 2 {
            Some(analysis::cross_correlation_matrix(&all_data))
        } else {
            None
        };
        let mut experimental = serde_json::Map::new();
        super::telemetry::insert_quantum_proxy_report(&mut experimental, quantum_report.as_ref());
        super::telemetry::insert_telemetry_window(&mut experimental, telemetry_report.as_ref());

        let mut root = serde_json::Map::new();
        root.insert(
            "standard".to_string(),
            serde_json::json!({
                "sources": &all_results,
                "cross_correlation": &cross,
            }),
        );
        if let Some(exp) = super::telemetry::finalize_experimental(experimental) {
            root.insert("experimental".to_string(), exp);
        }
        let json = serde_json::Value::Object(root);

        match std::fs::write(path, serde_json::to_string_pretty(&json).unwrap()) {
            Ok(()) => println!("\nResults written to {path}"),
            Err(e) => eprintln!("\nFailed to write {path}: {e}"),
        }
    }
}

fn print_source_summary(r: &analysis::SourceAnalysis, i: &SourceInterpretation) {
    println!();
    println!("  ┌─ {} ({} bytes)", r.source_name, r.sample_size);
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
