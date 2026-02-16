use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use openentropy_core::conditioning::{quick_min_entropy, quick_quality, quick_shannon};
use openentropy_core::platform::detect_available_sources;
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

#[derive(Serialize, Clone)]
struct PoolQualityReport {
    bytes: usize,
    shannon_entropy: f64,
    min_entropy: f64,
    healthy_sources: usize,
    total_sources: usize,
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
}

pub fn run(cfg: BenchCommandConfig<'_>) {
    // Single-source mode (replaces `probe`)
    if let Some(source_name) = cfg.source {
        run_single_source(source_name);
        return;
    }

    let profile = BenchProfile::parse(cfg.profile);
    let rank_by = RankBy::parse(cfg.rank_by);
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

        if row.success_rounds < settings.rounds || row.failures > 0 {
            row.score *= 0.8;
        }
    }

    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("\n{}", "=".repeat(96));
    println!(
        "{:<25} {:>5} {:>7} {:>7} {:>10} {:>8} {:>10} {:>6} {:>9}",
        "Source", "Grade", "H", "H∞", "KB/s", "Stability", "Rounds", "Fail", "State"
    );
    println!("{}", "-".repeat(96));
    for row in &rows {
        let grade = openentropy_core::grade_min_entropy(row.avg_min_entropy.max(0.0));
        let state = if row.success_rounds == 0 || row.failures > 0 {
            "UNSTABLE"
        } else {
            "OK"
        };
        let composite = if row.composite { " [C]" } else { "" };
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
    println!();
    println!("Grade is based on min-entropy (H∞), not Shannon.");
    println!("Stability is derived from run-to-run min-entropy consistency (1.0 = most stable).");

    let pool_report = if cfg.include_pool_quality {
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

    if let Some(path) = cfg.output_path {
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
            pool: pool_report,
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

    let quality = quick_quality(&data);
    println!("  Grade:           {}", quality.grade);
    println!("  Samples:         {}", quality.samples);
    println!(
        "  Shannon entropy: {:.4} / 8.0 bits",
        quality.shannon_entropy
    );
    println!("  Compression:     {:.4}", quality.compression_ratio);
    println!("  Unique values:   {}", quality.unique_values);
    println!("  Time:            {:.3}s", elapsed.as_secs_f64());
}
