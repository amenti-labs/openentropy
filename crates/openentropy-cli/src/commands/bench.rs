use std::collections::HashMap;
use std::time::Instant;

use openentropy_core::TelemetryWindowReport;
use openentropy_core::conditioning::{
    quick_autocorrelation_lag1, quick_min_entropy, quick_quality, quick_shannon,
};
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
    autocorrelation_sum: f64,
    min_entropy_values: Vec<f64>,
    collection_times_ms: Vec<f64>,
}

#[derive(Clone)]
struct BenchRow {
    name: String,
    composite: bool,
    success_rounds: usize,
    failures: u64,
    avg_shannon: f64,
    avg_min_entropy: f64,
    avg_throughput_bps: f64,
    avg_autocorrelation: f64,
    p99_latency_ms: f64,
    stability: f64,
    score: f64,
}

#[derive(Serialize)]
struct BenchReport {
    generated_unix: u64,
    profile: String,
    conditioning: String,
    rank_by: String,
    settings: BenchSettingsJson,
    sources: Vec<BenchSourceReport>,
    pool: Option<PoolQualityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_v1: Option<TelemetryWindowReport>,
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
    avg_autocorrelation: f64,
    p99_latency_ms: f64,
    stability: f64,
    grade: char,
    score: f64,
}

#[derive(Serialize, Clone)]
struct PoolQualityReport {
    bytes: usize,
    shannon_entropy: f64,
    min_entropy: f64,
    healthy_sources: usize,
    total_sources: usize,
}

pub struct BenchArgs {
    pub positional: Vec<String>,
    pub all: bool,
    pub conditioning: String,
    pub profile: String,
    pub samples_per_round: Option<usize>,
    pub rounds: Option<usize>,
    pub warmup_rounds: Option<usize>,
    pub timeout_sec: Option<f64>,
    pub rank_by: String,
    pub output: Option<String>,
    pub no_pool: bool,
    pub include_telemetry: bool,
}

pub fn run(args: BenchArgs) {
    // Single-source detail mode: exactly one positional arg (and no multi-source flags)
    if args.positional.len() == 1 && !args.all {
        if args.rounds.is_some() || args.warmup_rounds.is_some() || args.output.is_some() {
            eprintln!(
                "Note: --rounds, --warmup-rounds, and --output are ignored in single-source probe mode."
            );
        }
        let samples = args.samples_per_round.unwrap_or(5000);
        run_single_source(&args.positional[0], samples);
        return;
    }

    let profile = BenchProfile::parse(&args.profile);
    let rank_by = RankBy::parse(&args.rank_by);
    let telemetry = super::telemetry::TelemetryCapture::start(args.include_telemetry);
    let mode = super::parse_conditioning(&args.conditioning);
    let mut settings = profile.defaults();
    if let Some(v) = args.samples_per_round {
        settings.samples_per_round = v.max(1);
    }
    if let Some(v) = args.rounds {
        settings.rounds = v.max(1);
    }
    if let Some(v) = args.warmup_rounds {
        settings.warmup_rounds = v;
    }
    if let Some(v) = args.timeout_sec {
        settings.timeout_sec = v.max(0.1);
    }

    // When --all is used, slow sources (GPU, network, sensor) need more time.
    // Double the per-batch timeout unless the user explicitly set --timeout.
    if args.all && args.timeout_sec.is_none() {
        settings.timeout_sec *= 2.0;
    }

    // Build pool from positional args, --all, or default fast sources
    let source_filter = if args.all {
        Some("all".to_string())
    } else if !args.positional.is_empty() {
        Some(args.positional.join(","))
    } else {
        None
    };
    let pool_instance = super::make_pool(source_filter.as_deref());
    let infos = pool_instance.source_infos();
    let count = infos.len();
    let is_filtered = source_filter.is_some();

    if !is_filtered {
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
                entry.autocorrelation_sum += src.autocorrelation;
                entry.min_entropy_values.push(src.min_entropy);
                entry.collection_times_ms.push(src.time * 1000.0);
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

    let mut rows: Vec<BenchRow> = infos
        .iter()
        .map(|info| {
            let (
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                avg_autocorrelation,
                p99_latency_ms,
                stability,
            ) = if let Some(src_acc) = accum.get(&info.name) {
                let success_rounds = src_acc.success_rounds;
                if success_rounds > 0 {
                    let n = success_rounds as f64;
                    (
                        success_rounds,
                        src_acc.failures,
                        src_acc.shannon_sum / n,
                        src_acc.min_entropy_sum / n,
                        src_acc.throughput_sum / n,
                        src_acc.autocorrelation_sum / n,
                        percentile(&src_acc.collection_times_ms, 99.0),
                        stability_index(&src_acc.min_entropy_values),
                    )
                } else {
                    (0, src_acc.failures, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            } else {
                (0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            };

            BenchRow {
                name: info.name.clone(),
                composite: info.composite,
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                avg_autocorrelation,
                p99_latency_ms,
                stability,
                score: 0.0,
            }
        })
        .collect();

    let max_throughput = rows
        .iter()
        .map(|r| r.avg_throughput_bps)
        .fold(0.0_f64, f64::max);

    for row in &mut rows {
        row.score = match rank_by {
            RankBy::MinEntropy => row.avg_min_entropy,
            RankBy::Throughput => row.avg_throughput_bps,
            RankBy::Balanced => {
                let min_h_term = (row.avg_min_entropy / 8.0).clamp(0.0, 1.0);
                let throughput_term = if max_throughput > 0.0 {
                    (row.avg_throughput_bps / max_throughput).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                0.7 * min_h_term + 0.2 * throughput_term + 0.1 * row.stability
            }
        };

        // Graduated reliability penalty: scale with failure rate instead of binary 0.8×.
        let missed = settings.rounds.saturating_sub(row.success_rounds) as f64;
        let total_issues = missed + row.failures as f64;
        let expected = settings.rounds as f64;
        if total_issues > 0.0 && expected > 0.0 {
            let failure_rate = (total_issues / expected).clamp(0.0, 1.0);
            row.score *= 1.0 - 0.5 * failure_rate;
        }
    }

    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("\n{}", "=".repeat(120));
    println!(
        "{:<26} {:>5} {:>7} {:>7} {:>10} {:>6} {:>9} {:>9} {:>10} {:>6} {:>8}",
        "Source",
        "Grade",
        "H",
        "H∞",
        "KB/s",
        "r₁",
        "p99(ms)",
        "Stability",
        "Rounds",
        "Fail",
        "State"
    );
    println!("{}", "-".repeat(120));
    for row in &rows {
        let grade = openentropy_core::grade_min_entropy(row.avg_min_entropy.max(0.0));
        let state = if row.success_rounds == 0 {
            "UNSTABLE"
        } else if row.success_rounds < settings.rounds {
            "SLOW"
        } else {
            "OK"
        };
        let display_name = if row.composite {
            format!("{} [C]", row.name)
        } else {
            row.name.clone()
        };
        let rounds_str = format!("{}/{}", row.success_rounds, settings.rounds);
        println!(
            "{:<26} {:>5} {:>7.3} {:>7.3} {:>10.1} {:>6.3} {:>9.1} {:>9.2} {:>10} {:>6} {:>8}",
            display_name,
            grade,
            row.avg_shannon,
            row.avg_min_entropy,
            row.avg_throughput_bps / 1024.0,
            row.avg_autocorrelation,
            row.p99_latency_ms,
            row.stability,
            rounds_str,
            row.failures,
            state,
        );
    }
    println!();
    println!("Grade is based on min-entropy (H∞), not Shannon.");
    println!("r₁ = lag-1 autocorrelation (0 = ideal, ±1 = fully correlated).");
    println!("Stability is derived from run-to-run min-entropy consistency (1.0 = most stable).");

    // Mode comparison for sources with on-device conditioning (e.g. QCicada).
    let configurable: Vec<String> = infos
        .iter()
        .filter(|info| info.config.iter().any(|(k, _)| *k == "mode"))
        .map(|info| info.name.clone())
        .collect();
    for src_name in &configurable {
        let original_mode = pool_instance
            .with_source(src_name, |s| {
                s.config_options()
                    .iter()
                    .find(|(k, _)| *k == "mode")
                    .map(|(_, v)| v.clone())
            })
            .flatten()
            .unwrap_or_else(|| "raw".into());

        println!("\n{}", "=".repeat(68));
        println!("{src_name} — on-device mode comparison\n");
        println!(
            "  {:<12} {:>5} {:>7} {:>7} {:>6} {:>10}",
            "Mode", "Grade", "H", "H∞", "r₁", "KB/s"
        );
        println!("  {}", "-".repeat(55));

        let mode_samples = settings.samples_per_round;
        for mode_name in &["raw", "sha256", "samples"] {
            let set_ok = pool_instance
                .with_source(src_name, |s| s.set_config("mode", mode_name).is_ok())
                .unwrap_or(false);
            if !set_ok {
                continue;
            }

            // Give the device time to process the mode-switch command, then
            // flush any stale bytes from the previous mode's buffer.
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = pool_instance.get_source_raw_bytes(src_name, 64);

            let t0 = Instant::now();
            let data = pool_instance
                .get_source_raw_bytes(src_name, mode_samples)
                .unwrap_or_default();
            let elapsed = t0.elapsed();

            if data.is_empty() {
                println!("  {:<12} (no data)", mode_name);
                continue;
            }

            let shannon = quick_shannon(&data);
            let min_h = quick_min_entropy(&data);
            let autocorr = quick_autocorrelation_lag1(&data);
            let grade = openentropy_core::grade_min_entropy(min_h.max(0.0));
            let kbps = data.len() as f64 / elapsed.as_secs_f64() / 1024.0;

            println!(
                "  {:<12} {:>5} {:>7.3} {:>7.3} {:>6.3} {:>10.1}",
                mode_name, grade, shannon, min_h, autocorr, kbps
            );
        }

        // Restore original mode.
        let _ = pool_instance.with_source(src_name, |s| s.set_config("mode", &original_mode));

        println!();
        println!("  raw     = health-tested noise, no hashing (device-side)");
        println!("  sha256  = NIST SP 800-90B SHA-256 conditioning (device-side)");
        println!("  samples = raw ADC readings, no processing");
    }

    let pool_report = if !args.no_pool {
        let bytes = 65_536usize;
        let output = pool_instance.get_bytes(bytes, mode);
        let health = pool_instance.health_report();
        let report = PoolQualityReport {
            bytes: output.len(),
            shannon_entropy: quick_shannon(&output),
            min_entropy: quick_min_entropy(&output),
            healthy_sources: health.healthy,
            total_sources: health.total,
        };

        println!("\n{}", "=".repeat(68));
        println!(
            "Pool Output Quality (conditioning: {})\n",
            args.conditioning
        );
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

    let telemetry_report = telemetry.finish();
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("bench", window);
    }

    if let Some(path) = args.output.as_deref() {
        let report = BenchReport {
            generated_unix: super::unix_timestamp_now(),
            profile: profile.as_str().to_string(),
            conditioning: args.conditioning.to_string(),
            rank_by: rank_by.as_str().to_string(),
            settings: BenchSettingsJson {
                samples_per_round: settings.samples_per_round,
                rounds: settings.rounds,
                warmup_rounds: settings.warmup_rounds,
                timeout_sec: settings.timeout_sec,
            },
            sources: rows
                .iter()
                .map(|row| BenchSourceReport {
                    name: row.name.clone(),
                    composite: row.composite,
                    healthy: row.avg_min_entropy > 1.0 && row.failures == 0,
                    success_rounds: row.success_rounds,
                    failures: row.failures,
                    avg_shannon: row.avg_shannon,
                    avg_min_entropy: row.avg_min_entropy,
                    avg_throughput_bps: row.avg_throughput_bps,
                    avg_autocorrelation: row.avg_autocorrelation,
                    p99_latency_ms: row.p99_latency_ms,
                    stability: row.stability,
                    grade: openentropy_core::grade_min_entropy(row.avg_min_entropy.max(0.0)),
                    score: row.score,
                })
                .collect(),
            pool: pool_report,
            telemetry_v1: telemetry_report,
        };

        super::write_json(&report, path, "Benchmark report");
    }
}

fn snapshot_counters(sources: &[openentropy_core::SourceHealth]) -> HashMap<String, (u64, u64)> {
    sources
        .iter()
        .map(|s| (s.name.clone(), (s.bytes, s.failures)))
        .collect()
}

/// Compute the p-th percentile of a float slice using nearest-rank.
fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p / 100.0 * sorted.len() as f64).ceil() as usize).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

fn stability_index(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return 1.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let stddev = var.sqrt();
    // If mean ≈ 0 and stddev ≈ 0, all values are consistently near zero — perfectly stable.
    if mean.abs() < f64::EPSILON {
        return if stddev < f64::EPSILON { 1.0 } else { 0.0 };
    }
    let cv = (stddev / mean.abs()).min(1.0);
    (1.0 - cv).clamp(0.0, 1.0)
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
            _ => Self::Balanced,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::MinEntropy => "min_entropy",
            Self::Throughput => "throughput",
        }
    }
}

fn run_single_source(source_name: &str, samples: usize) {
    let src: std::sync::Arc<dyn openentropy_core::EntropySource> =
        match super::find_source(source_name) {
            Some(s) => std::sync::Arc::from(s),
            None => {
                eprintln!(
                    "Source '{source_name}' not found. Run 'openentropy scan' to list sources."
                );
                std::process::exit(1);
            }
        };
    let info = src.info();
    println!("Probing: {}", info.name);
    println!("  {}", info.description);
    println!();

    // If the source has configurable modes, benchmark each one.
    let config = src.config_options();
    let mode_config = config.iter().find(|(k, _)| *k == "mode");

    if let Some((_, current_mode)) = mode_config {
        let modes = ["raw", "sha256", "samples"];
        let original_mode = current_mode.clone();

        println!(
            "  Source has on-device conditioning modes — benchmarking each.\n\
             \x20 No additional conditioning applied (measuring device output directly).\n"
        );
        println!(
            "  {:<12} {:>5} {:>7} {:>7} {:>6} {:>10} {:>6} {:>8}",
            "Mode", "Grade", "H", "H∞", "r₁", "KB/s", "Compr", "Unique"
        );
        println!("  {}", "-".repeat(68));

        for mode_name in &modes {
            if src.set_config("mode", mode_name).is_err() {
                continue;
            }

            let t0 = Instant::now();
            let data = src.collect(samples);
            let elapsed = t0.elapsed();

            if data.is_empty() {
                println!("  {:<12} (no data)", mode_name);
                continue;
            }

            let shannon = quick_shannon(&data);
            let min_h = quick_min_entropy(&data);
            let autocorr = quick_autocorrelation_lag1(&data);
            let grade = openentropy_core::grade_min_entropy(min_h.max(0.0));
            let quality = quick_quality(&data);
            let kbps = data.len() as f64 / elapsed.as_secs_f64() / 1024.0;

            println!(
                "  {:<12} {:>5} {:>7.3} {:>7.3} {:>6.3} {:>10.1} {:>6.4} {:>8}",
                mode_name,
                grade,
                shannon,
                min_h,
                autocorr,
                kbps,
                quality.compression_ratio,
                quality.unique_values
            );
        }

        // Restore original mode.
        let _ = src.set_config("mode", &original_mode);

        println!();
        println!("  raw     = health-tested noise, no hashing (device-side)");
        println!("  sha256  = NIST SP 800-90B SHA-256 conditioning (device-side)");
        println!("  samples = raw ADC readings from photodiode, no processing");
    } else {
        // Standard single-source probe: measure raw output, no extra conditioning.
        // Wrap in a thread with timeout to avoid hanging on slow/stuck sources.
        let t0 = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let src_clone = std::sync::Arc::clone(&src);
        std::thread::spawn(move || {
            let data = src_clone.collect(samples);
            let _ = tx.send(data);
        });
        let raw_data = match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(data) => data,
            Err(_) => {
                println!("  Source timed out after 30s.");
                return;
            }
        };
        let elapsed = t0.elapsed();

        if raw_data.is_empty() {
            println!("  No data collected.");
            return;
        }

        let shannon = quick_shannon(&raw_data);
        let min_h = quick_min_entropy(&raw_data);
        let autocorr = quick_autocorrelation_lag1(&raw_data);
        let grade = openentropy_core::grade_min_entropy(min_h.max(0.0));
        let quality = quick_quality(&raw_data);

        println!("  Grade:           {} (based on H∞)", grade);
        println!("  Samples:         {}", raw_data.len());
        println!("  Shannon entropy: {:.4} / 8.0 bits", shannon);
        println!("  Min-entropy H∞:  {:.4} / 8.0 bits", min_h);
        println!("  Autocorr r₁:    {:.4}", autocorr);
        println!("  Compression:     {:.4}", quality.compression_ratio);
        println!("  Unique values:   {}", quality.unique_values);
        println!("  Latency:         {:.1}ms", elapsed.as_secs_f64() * 1000.0);
    }
}
