use std::collections::HashMap;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use openentropy_core::TelemetryWindowReport;
use openentropy_core::metrics::experimental::quantum_proxy_v3::{
    MODEL_ID as QUANTUM_MODEL_ID, MODEL_VERSION as QUANTUM_MODEL_VERSION, PriorCalibration,
    QuantumAssessmentConfig, QuantumBatchReport, QuantumSourceInput, StressSweepConfig,
    StressSweepReport, TelemetryConfoundConfig,
    assess_batch_from_streams_with_calibration_and_telemetry, collect_stress_sweep,
    default_calibration, estimate_stress_sensitivity_from_streams, load_calibration_from_path,
    parse_source_category, quality_factor_from_analysis,
};
use openentropy_core::metrics::standard::EntropyMeasurements;
use openentropy_core::metrics::streams::{collect_named_source_stream_samples, to_named_streams};
use openentropy_core::platform::detect_available_sources;
use openentropy_core::pool::EntropyPool;
use serde::Serialize;

#[derive(Clone, Copy, Debug)]
enum BenchProfile {
    Quick,
    Standard,
    Deep,
}

#[derive(Clone, Copy, Debug)]
enum RankBy {
    Balanced,
    MinEntropy,
    Throughput,
    Quantum,
}

#[derive(Clone, Copy, Debug)]
struct BenchSettings {
    samples_per_round: usize,
    rounds: usize,
    warmup_rounds: usize,
    timeout_sec: f64,
}

#[derive(Default)]
struct SourceAccumulator {
    success_rounds: usize,
    failures: u64,
    shannon_sum: f64,
    min_entropy_sum: f64,
    throughput_sum: f64,
    min_entropy_values: Vec<f64>,
}

#[derive(Clone)]
struct BenchRow {
    name: String,
    composite: bool,
    healthy: bool,
    success_rounds: usize,
    failures: u64,
    avg_shannon: f64,
    avg_min_entropy: f64,
    avg_throughput_bps: f64,
    stability: f64,
    quantum_score: f64,
    quantum_min_entropy_bits: f64,
    quantum_to_classical: Option<f64>,
    rank_score: f64,
    score: f64,
}

#[derive(Serialize)]
struct BenchReport {
    generated_unix: u64,
    profile: String,
    conditioning: String,
    rank_by: String,
    settings: BenchSettingsJson,
    standard: BenchStandardSection,
    #[serde(skip_serializing_if = "Option::is_none")]
    experimental: Option<BenchExperimentalSection>,
}

#[derive(Serialize)]
struct BenchSettingsJson {
    samples_per_round: usize,
    rounds: usize,
    warmup_rounds: usize,
    timeout_sec: f64,
}

#[derive(Serialize)]
struct BenchSourceReport {
    name: String,
    composite: bool,
    healthy: bool,
    success_rounds: usize,
    failures: u64,
    avg_shannon: f64,
    avg_min_entropy: f64,
    avg_throughput_bps: f64,
    stability: f64,
    grade: char,
    score: f64,
}

#[derive(Serialize)]
struct BenchStandardSection {
    sources: Vec<BenchSourceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pool: Option<PoolQualityReport>,
}

#[derive(Serialize)]
struct BenchExperimentalSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_v1: Option<TelemetryWindowReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantum_proxy_v3: Option<BenchQuantumReport>,
}

#[derive(Serialize, Clone)]
struct PoolQualityReport {
    bytes: usize,
    shannon_entropy: f64,
    min_entropy: f64,
    healthy_sources: usize,
    total_sources: usize,
}

#[derive(Serialize, Clone)]
struct BenchQuantumReport {
    model_id: &'static str,
    model_version: u32,
    sample_bytes: usize,
    calibration_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stress_sweep: Option<StressSweepReport>,
    report: QuantumBatchReport,
}

pub struct BenchCommandConfig<'a> {
    pub source_filter: Option<&'a str>,
    pub conditioning: &'a str,
    pub source: Option<&'a str>,
    pub profile: &'a str,
    pub samples_per_round: Option<usize>,
    pub rounds: Option<usize>,
    pub warmup_rounds: Option<usize>,
    pub timeout_sec: Option<f64>,
    pub rank_by: &'a str,
    pub output_path: Option<&'a str>,
    pub include_pool_quality: bool,
    pub include_quantum: bool,
    pub include_telemetry: bool,
    pub quantum_live_stress: bool,
    pub quantum_calibration_path: Option<&'a str>,
}

pub fn run(cfg: BenchCommandConfig<'_>) {
    // Single-source mode (replaces `probe`)
    if let Some(source_name) = cfg.source {
        run_single_source(source_name);
        return;
    }

    let profile = BenchProfile::parse(cfg.profile);
    let rank_by = RankBy::parse(cfg.rank_by);
    let include_quantum = cfg.include_quantum || matches!(rank_by, RankBy::Quantum);
    let telemetry = super::telemetry::TelemetryCapture::start(cfg.include_telemetry);
    let mode = super::parse_conditioning(cfg.conditioning);
    let mut settings = profile.defaults();
    if let Some(v) = cfg.samples_per_round {
        settings.samples_per_round = v.max(1);
    }
    if let Some(v) = cfg.rounds {
        settings.rounds = v.max(1);
    }
    if let Some(v) = cfg.warmup_rounds {
        settings.warmup_rounds = v;
    }
    if let Some(v) = cfg.timeout_sec {
        settings.timeout_sec = v.max(0.1);
    }

    let pool_instance = super::make_pool(cfg.source_filter);
    let infos = pool_instance.source_infos();
    let count = infos.len();

    if cfg.source_filter.is_none() {
        println!("Benchmarking {count} fast sources...");
    } else {
        println!("Benchmarking {count} sources...");
    }
    println!(
        "Profile={} rounds={} warmup={} samples/round={} timeout={:.1}s rank-by={}",
        profile.as_str(),
        settings.rounds,
        settings.warmup_rounds,
        settings.samples_per_round,
        settings.timeout_sec,
        rank_by.as_str()
    );
    if matches!(rank_by, RankBy::Quantum) {
        println!(
            "Ranking uses experimental quantum_proxy_v3; `standard.score` remains non-experimental."
        );
    }
    println!();

    for i in 0..settings.warmup_rounds {
        let _ =
            pool_instance.collect_all_parallel_n(settings.timeout_sec, settings.samples_per_round);
        println!("Warmup round {}/{}", i + 1, settings.warmup_rounds);
    }
    if settings.warmup_rounds > 0 {
        println!();
    }

    let mut prev = snapshot_counters(&pool_instance.health_report().sources);
    let mut accum: HashMap<String, SourceAccumulator> = HashMap::new();

    for round_idx in 0..settings.rounds {
        let t0 = Instant::now();
        let collected =
            pool_instance.collect_all_parallel_n(settings.timeout_sec, settings.samples_per_round);
        let wall = t0.elapsed().as_secs_f64();
        let health = pool_instance.health_report();

        for src in &health.sources {
            let (prev_bytes, prev_failures) = prev
                .get(&src.name)
                .copied()
                .unwrap_or((src.bytes, src.failures));
            let bytes_delta = src.bytes.saturating_sub(prev_bytes);
            let failures_delta = src.failures.saturating_sub(prev_failures);

            let entry = accum.entry(src.name.clone()).or_default();
            entry.failures += failures_delta;

            if bytes_delta > 0 {
                entry.success_rounds += 1;
                entry.shannon_sum += src.entropy;
                entry.min_entropy_sum += src.min_entropy;
                entry.min_entropy_values.push(src.min_entropy);
                if src.time > 0.0 {
                    entry.throughput_sum += bytes_delta as f64 / src.time;
                }
            }

            prev.insert(src.name.clone(), (src.bytes, src.failures));
        }

        println!(
            "Round {}/{} complete: collected {} bytes in {:.2}s",
            round_idx + 1,
            settings.rounds,
            collected,
            wall
        );
    }

    let final_health = pool_instance.health_report();
    let health_by_name: HashMap<String, bool> = final_health
        .sources
        .iter()
        .map(|s| (s.name.clone(), s.healthy))
        .collect();

    let mut rows: Vec<BenchRow> = infos
        .iter()
        .map(|info| {
            let (
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                stability,
            ) = if let Some(src_acc) = accum.get(&info.name) {
                let success_rounds = src_acc.success_rounds;
                if success_rounds > 0 {
                    (
                        success_rounds,
                        src_acc.failures,
                        src_acc.shannon_sum / success_rounds as f64,
                        src_acc.min_entropy_sum / success_rounds as f64,
                        src_acc.throughput_sum / success_rounds as f64,
                        stability_index(&src_acc.min_entropy_values),
                    )
                } else {
                    (0, src_acc.failures, 0.0, 0.0, 0.0, 0.0)
                }
            } else {
                (0, 0, 0.0, 0.0, 0.0, 0.0)
            };

            BenchRow {
                name: info.name.clone(),
                composite: info.composite,
                healthy: health_by_name.get(&info.name).copied().unwrap_or(false),
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                stability,
                quantum_score: 0.0,
                quantum_min_entropy_bits: 0.0,
                quantum_to_classical: None,
                rank_score: 0.0,
                score: 0.0,
            }
        })
        .collect();

    let telemetry_report = telemetry.finish();
    let quantum_sample_bytes = settings.samples_per_round.clamp(512, 4096);
    let quantum_report = if include_quantum {
        build_quantum_report(
            &pool_instance,
            &rows,
            quantum_sample_bytes,
            settings.timeout_sec,
            cfg.quantum_live_stress,
            cfg.quantum_calibration_path,
            telemetry_report.as_ref(),
        )
    } else {
        None
    };
    if let Some(ref qr) = quantum_report {
        let quantum_by_source: HashMap<&str, (f64, f64, Option<f64>)> = qr
            .report
            .sources
            .iter()
            .map(|s| {
                let ratio = if s.classical_min_entropy_bits > 0.0 {
                    Some(s.quantum_min_entropy_bits / s.classical_min_entropy_bits)
                } else if s.quantum_min_entropy_bits > 0.0 {
                    Some(f64::INFINITY)
                } else {
                    Some(0.0)
                };
                (
                    s.name.as_str(),
                    (s.quantum_score, s.quantum_min_entropy_bits, ratio),
                )
            })
            .collect();
        for row in &mut rows {
            if let Some((q, q_bits, q_to_c)) = quantum_by_source.get(row.name.as_str()) {
                row.quantum_score = *q;
                row.quantum_min_entropy_bits = *q_bits;
                row.quantum_to_classical = *q_to_c;
            }
        }
    }

    let max_throughput = rows
        .iter()
        .map(|r| r.avg_throughput_bps)
        .fold(0.0_f64, f64::max);

    for row in &mut rows {
        row.rank_score = rank_score_for_mode(row, rank_by, max_throughput);

        // Keep `standard.score` free of experimental terms even when ranking
        // is requested by quantum diagnostics.
        row.score = standard_score_for_mode(row, rank_by, max_throughput);

        if row.success_rounds < settings.rounds || row.failures > 0 {
            row.rank_score *= 0.8;
            row.score *= 0.8;
        }
    }

    rows.sort_by(|a, b| {
        b.rank_score
            .partial_cmp(&a.rank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if include_quantum {
        println!("\n{}", "=".repeat(128));
        println!(
            "{:<25} {:>5} {:>7} {:>7} {:>6} {:>8} {:>8} {:>10} {:>8} {:>10} {:>6} {:>9}",
            "Source",
            "Grade",
            "H",
            "H∞",
            "Q",
            "Qbits",
            "Q:C",
            "KB/s",
            "Stability",
            "Rounds",
            "Fail",
            "State"
        );
        println!("{}", "-".repeat(128));
    } else {
        println!("\n{}", "=".repeat(96));
        println!(
            "{:<25} {:>5} {:>7} {:>7} {:>10} {:>8} {:>10} {:>6} {:>9}",
            "Source", "Grade", "H", "H∞", "KB/s", "Stability", "Rounds", "Fail", "State"
        );
        println!("{}", "-".repeat(96));
    }
    for row in &rows {
        let grade = openentropy_core::grade_min_entropy(row.avg_min_entropy.max(0.0));
        let state = if row.success_rounds == 0 || row.failures > 0 {
            "UNSTABLE"
        } else {
            "OK"
        };
        let composite = if row.composite { " [C]" } else { "" };
        if include_quantum {
            let q_to_c = row
                .quantum_to_classical
                .map(|v| format!("{v:.3}"))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{:<25} {:>5} {:>7.3} {:>7.3} {:>6.3} {:>8.3} {:>8} {:>10.1} {:>8.2} {:>6}/{} {:>6} {:>9}{}",
                row.name,
                grade,
                row.avg_shannon,
                row.avg_min_entropy,
                row.quantum_score,
                row.quantum_min_entropy_bits,
                q_to_c,
                row.avg_throughput_bps / 1024.0,
                row.stability,
                row.success_rounds,
                settings.rounds,
                row.failures,
                state,
                composite
            );
        } else {
            println!(
                "{:<25} {:>5} {:>7.3} {:>7.3} {:>10.1} {:>8.2} {:>6}/{} {:>6} {:>9}{}",
                row.name,
                grade,
                row.avg_shannon,
                row.avg_min_entropy,
                row.avg_throughput_bps / 1024.0,
                row.stability,
                row.success_rounds,
                settings.rounds,
                row.failures,
                state,
                composite
            );
        }
    }
    println!();
    println!("Grade is based on min-entropy (H∞), not Shannon.");
    if include_quantum {
        println!("Q and Qbits are quantum proxy score and quantum-attributed min-entropy bits.");
        println!("Q:C is per-source quantum-to-classical min-entropy ratio.");
    }
    println!("Stability is derived from run-to-run min-entropy consistency (1.0 = most stable).");

    if let Some(ref qr) = quantum_report {
        println!("\n{}", "=".repeat(68));
        println!("Quantum:Classical Contribution Proxy");
        println!(
            "  Aggregate Q:C = {:.3}:{:.3} (Q fraction {:.1}%)",
            qr.report.aggregate.quantum_bits,
            qr.report.aggregate.classical_bits,
            qr.report.aggregate.quantum_fraction * 100.0
        );
        println!(
            "  Aggregate CI95 Q fraction: {:.1}% .. {:.1}%   Q:C ratio: {:.3} .. {:.3}",
            qr.report.aggregate.quantum_fraction_ci_low * 100.0,
            qr.report.aggregate.quantum_fraction_ci_high * 100.0,
            qr.report.aggregate.quantum_to_classical_ci_low,
            qr.report.aggregate.quantum_to_classical_ci_high
        );
        println!("  Calibration: {}", qr.calibration_source);
        if let Some(stress) = &qr.stress_sweep {
            println!("  Stress sweep: enabled ({} ms)", stress.elapsed_ms);
        } else {
            println!("  Stress sweep: stream-variability estimate");
        }
        println!(
            "  Coupling significance: BH-FDR alpha={:.3}, null_rounds={}, hard_gate={}",
            qr.report.config.coupling_fdr_alpha,
            qr.report.config.coupling_null_rounds,
            qr.report.config.coupling_use_fdr_gate
        );
        for src in qr.report.sources.iter().take(10) {
            println!(
                "  {:20} q={:.3} q_bits={:.3} (prior={:.2} quality={:.2} stress={:.2} coupling={:.2} sig={:.0}%)",
                src.name,
                src.quantum_score,
                src.quantum_min_entropy_bits,
                src.physics_prior,
                src.quality_factor,
                src.stress_sensitivity,
                src.coupling_penalty,
                src.coupling_significant_pair_fraction_any * 100.0
            );
        }
    }

    let pool_report = if cfg.include_pool_quality {
        let bytes = 65_536usize;
        let output = pool_instance.get_bytes(bytes, mode);
        let health = pool_instance.health_report();
        let metrics = EntropyMeasurements::from_bytes(&output, None);
        let report = PoolQualityReport {
            bytes: output.len(),
            shannon_entropy: metrics.shannon_entropy,
            min_entropy: metrics.min_entropy,
            healthy_sources: health.healthy,
            total_sources: health.total,
        };

        println!("\n{}", "=".repeat(68));
        println!("Pool Output Quality (conditioning: {})\n", cfg.conditioning);
        println!("  Conditioned output: {} bytes", report.bytes);
        println!(
            "  Shannon entropy: {:.4} / 8.0 bits/byte",
            report.shannon_entropy
        );
        println!(
            "  Min-entropy H∞:  {:.4} / 8.0 bits/byte",
            report.min_entropy
        );
        println!(
            "\n  {}/{} sources healthy",
            report.healthy_sources, report.total_sources
        );

        Some(report)
    } else {
        None
    };
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("bench", window);
    }

    if let Some(path) = cfg.output_path {
        let experimental = if quantum_report.is_some() || telemetry_report.is_some() {
            Some(BenchExperimentalSection {
                telemetry_v1: telemetry_report.clone(),
                quantum_proxy_v3: quantum_report.clone(),
            })
        } else {
            None
        };
        let report = BenchReport {
            generated_unix: unix_timestamp_now(),
            profile: profile.as_str().to_string(),
            conditioning: cfg.conditioning.to_string(),
            rank_by: rank_by.as_str().to_string(),
            settings: BenchSettingsJson {
                samples_per_round: settings.samples_per_round,
                rounds: settings.rounds,
                warmup_rounds: settings.warmup_rounds,
                timeout_sec: settings.timeout_sec,
            },
            standard: BenchStandardSection {
                sources: rows
                    .iter()
                    .map(|row| BenchSourceReport {
                        name: row.name.clone(),
                        composite: row.composite,
                        healthy: row.healthy,
                        success_rounds: row.success_rounds,
                        failures: row.failures,
                        avg_shannon: row.avg_shannon,
                        avg_min_entropy: row.avg_min_entropy,
                        avg_throughput_bps: row.avg_throughput_bps,
                        stability: row.stability,
                        grade: openentropy_core::grade_min_entropy(row.avg_min_entropy.max(0.0)),
                        score: row.score,
                    })
                    .collect(),
                pool: pool_report.clone(),
            },
            experimental,
        };

        match std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()) {
            Ok(()) => println!("\nBenchmark report written to {path}"),
            Err(e) => eprintln!("\nFailed to write benchmark report to {path}: {e}"),
        }
    }
}

fn snapshot_counters(sources: &[openentropy_core::SourceHealth]) -> HashMap<String, (u64, u64)> {
    sources
        .iter()
        .map(|s| (s.name.clone(), (s.bytes, s.failures)))
        .collect()
}

fn balanced_score(row: &BenchRow, max_throughput: f64) -> f64 {
    let min_h_term = (row.avg_min_entropy / 8.0).clamp(0.0, 1.0);
    let throughput_term = if max_throughput > 0.0 {
        (row.avg_throughput_bps / max_throughput).clamp(0.0, 1.0)
    } else {
        0.0
    };
    0.7 * min_h_term + 0.2 * throughput_term + 0.1 * row.stability
}

fn rank_score_for_mode(row: &BenchRow, rank_by: RankBy, max_throughput: f64) -> f64 {
    match rank_by {
        RankBy::Balanced => balanced_score(row, max_throughput),
        RankBy::MinEntropy => row.avg_min_entropy,
        RankBy::Throughput => row.avg_throughput_bps,
        RankBy::Quantum => row.quantum_score,
    }
}

fn standard_score_for_mode(row: &BenchRow, rank_by: RankBy, max_throughput: f64) -> f64 {
    match rank_by {
        RankBy::Quantum => balanced_score(row, max_throughput),
        _ => rank_score_for_mode(row, rank_by, max_throughput),
    }
}

fn stability_index(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return 1.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean.abs() < f64::EPSILON {
        return 0.0;
    }
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let stddev = var.sqrt();
    let cv = (stddev / mean.abs()).min(1.0);
    (1.0 - cv).clamp(0.0, 1.0)
}

fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn build_quantum_report(
    pool: &EntropyPool,
    rows: &[BenchRow],
    sample_bytes: usize,
    timeout_sec: f64,
    live_stress: bool,
    calibration_path: Option<&str>,
    telemetry_window: Option<&TelemetryWindowReport>,
) -> Option<BenchQuantumReport> {
    let selected_names: Vec<String> = rows
        .iter()
        .filter(|row| row.success_rounds > 0 && row.avg_min_entropy > 0.0)
        .map(|row| row.name.clone())
        .collect();
    if selected_names.is_empty() {
        return None;
    }
    let sampled = collect_named_source_stream_samples(pool, &selected_names, sample_bytes, 64);
    if sampled.is_empty() {
        return None;
    }

    let streams = to_named_streams(&sampled);
    let qcfg = QuantumAssessmentConfig::default();
    let mut stress_by_name = estimate_stress_sensitivity_from_streams(&streams, qcfg);

    let stress_sweep = if live_stress {
        let sweep = collect_stress_sweep(
            pool,
            &selected_names,
            sample_bytes,
            64,
            timeout_sec,
            qcfg,
            StressSweepConfig::default(),
        );
        for (name, row) in &sweep.by_source {
            stress_by_name.insert(name.clone(), row.stress_sensitivity);
        }
        Some(sweep)
    } else {
        None
    };

    let (calibration, calibration_source) = load_calibration_for_bench(calibration_path);

    let row_by_name: HashMap<&str, &BenchRow> = rows.iter().map(|r| (r.name.as_str(), r)).collect();
    let mut inputs: Vec<QuantumSourceInput> = Vec::new();
    for sample in &sampled {
        let Some(row) = row_by_name.get(sample.name.as_str()) else {
            continue;
        };
        let analysis = openentropy_core::analysis::full_analysis(&sample.name, &sample.data);
        let quality = quality_factor_from_analysis(&analysis);
        let category = parse_source_category(&sample.category);
        inputs.push(QuantumSourceInput {
            name: sample.name.clone(),
            category,
            min_entropy_bits: row.avg_min_entropy,
            quality_factor: quality,
            stress_sensitivity: stress_by_name.get(&sample.name).copied().unwrap_or(0.0),
            physics_prior_override: None,
        });
    }

    if inputs.is_empty() {
        return None;
    }

    let report = assess_batch_from_streams_with_calibration_and_telemetry(
        &inputs,
        &streams,
        qcfg,
        64,
        &calibration,
        telemetry_window,
        TelemetryConfoundConfig::default(),
    );
    Some(BenchQuantumReport {
        model_id: QUANTUM_MODEL_ID,
        model_version: QUANTUM_MODEL_VERSION,
        sample_bytes,
        calibration_source,
        stress_sweep,
        report,
    })
}

fn load_calibration_for_bench(path: Option<&str>) -> (PriorCalibration, String) {
    let Some(path) = path else {
        return (default_calibration(), "default_seeded".to_string());
    };

    let p = Path::new(path);
    match load_calibration_from_path(p) {
        Ok(c) => (c, p.display().to_string()),
        Err(e) => {
            eprintln!(
                "Warning: failed to load calibration from {} ({e}); using default seeded calibration",
                p.display()
            );
            (default_calibration(), "default_seeded".to_string())
        }
    }
}

impl BenchProfile {
    fn parse(s: &str) -> Self {
        match s {
            "quick" => Self::Quick,
            "deep" => Self::Deep,
            _ => Self::Standard,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }

    fn defaults(self) -> BenchSettings {
        match self {
            Self::Quick => BenchSettings {
                samples_per_round: 2048,
                rounds: 3,
                warmup_rounds: 1,
                timeout_sec: 2.0,
            },
            Self::Standard => BenchSettings {
                samples_per_round: 4096,
                rounds: 5,
                warmup_rounds: 1,
                timeout_sec: 3.0,
            },
            Self::Deep => BenchSettings {
                samples_per_round: 16384,
                rounds: 10,
                warmup_rounds: 2,
                timeout_sec: 6.0,
            },
        }
    }
}

impl RankBy {
    fn parse(s: &str) -> Self {
        match s {
            "min_entropy" => Self::MinEntropy,
            "throughput" => Self::Throughput,
            "quantum" => Self::Quantum,
            _ => Self::Balanced,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::MinEntropy => "min_entropy",
            Self::Throughput => "throughput",
            Self::Quantum => "quantum",
        }
    }
}

fn run_single_source(source_name: &str) {
    let sources = detect_available_sources();
    let matches: Vec<_> = sources
        .into_iter()
        .filter(|s| {
            s.name()
                .to_lowercase()
                .contains(&source_name.to_lowercase())
        })
        .collect();

    if matches.is_empty() {
        eprintln!(
            "Source '{}' not found. Run 'scan' to list sources.",
            source_name
        );
        std::process::exit(1);
    }

    let src = &matches[0];
    let info = src.info();
    println!("Probing: {}", info.name);
    println!("  {}", info.description);
    println!();

    let t0 = Instant::now();
    let data = src.collect(5000);
    let elapsed = t0.elapsed();

    if data.is_empty() {
        println!("  No data collected.");
        return;
    }

    let metrics = EntropyMeasurements::from_bytes(&data, Some(elapsed.as_secs_f64()));
    let grade = openentropy_core::grade_min_entropy(metrics.min_entropy.max(0.0));
    let mut uniq = [false; 256];
    for &b in &data {
        uniq[b as usize] = true;
    }
    let unique_values = uniq.into_iter().filter(|v| *v).count();
    println!("  Grade:           {}", grade);
    println!("  Samples:         {}", data.len());
    println!(
        "  Shannon entropy: {:.4} / 8.0 bits",
        metrics.shannon_entropy
    );
    println!("  Min-entropy H∞:  {:.4} / 8.0 bits", metrics.min_entropy);
    println!("  Compression:     {:.4}", metrics.compression_ratio);
    println!("  Unique values:   {}", unique_values);
    println!("  Time:            {:.3}s", elapsed.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> BenchRow {
        BenchRow {
            name: "sample".to_string(),
            composite: false,
            healthy: true,
            success_rounds: 3,
            failures: 0,
            avg_shannon: 7.8,
            avg_min_entropy: 6.0,
            avg_throughput_bps: 4_000.0,
            stability: 0.9,
            quantum_score: 0.25,
            quantum_min_entropy_bits: 1.5,
            quantum_to_classical: Some(0.33),
            rank_score: 0.0,
            score: 0.0,
        }
    }

    #[test]
    fn quantum_ranking_keeps_standard_score_non_experimental() {
        let row = sample_row();
        let max_throughput = 8_000.0;
        let rank = rank_score_for_mode(&row, RankBy::Quantum, max_throughput);
        let standard = standard_score_for_mode(&row, RankBy::Quantum, max_throughput);
        let balanced = balanced_score(&row, max_throughput);

        assert!((rank - row.quantum_score).abs() < 1e-12);
        assert!((standard - balanced).abs() < 1e-12);
        assert!((standard - row.quantum_score).abs() > 1e-12);
    }
}
