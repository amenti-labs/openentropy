use std::time::Instant;

use openentropy_core::TelemetryWindowReport;
use openentropy_core::benchmark::{self, BenchConfig, RankBy};
use openentropy_core::conditioning::{
    quick_autocorrelation_lag1, quick_min_entropy, quick_quality, quick_shannon,
};
use serde::Serialize;

/// CLI-specific JSON report wrapping the core benchmark results with CLI metadata.
#[derive(Serialize)]
struct CliBenchReport {
    generated_unix: u64,
    profile: String,
    conditioning: String,
    rank_by: String,
    settings: CliBenchSettings,
    sources: Vec<benchmark::BenchSourceReport>,
    pool: Option<benchmark::PoolQualityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_v1: Option<TelemetryWindowReport>,
}

#[derive(Serialize)]
struct CliBenchSettings {
    samples_per_round: usize,
    rounds: usize,
    warmup_rounds: usize,
    timeout_sec: f64,
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

    let profile_str = parse_profile_name(&args.profile);
    let rank_by = parse_rank_by(&args.rank_by);
    let telemetry = super::telemetry::TelemetryCapture::start(args.include_telemetry);
    let mode = super::parse_conditioning(&args.conditioning);

    let (def_spr, def_rounds, def_warmup, def_timeout) = profile_defaults(&args.profile);
    let samples_per_round = args.samples_per_round.map(|v| v.max(1)).unwrap_or(def_spr);
    let rounds = args.rounds.map(|v| v.max(1)).unwrap_or(def_rounds);
    let warmup_rounds = args.warmup_rounds.unwrap_or(def_warmup);
    let mut timeout_sec = args.timeout_sec.map(|v| v.max(0.1)).unwrap_or(def_timeout);

    // When --all is used, slow sources (GPU, network, sensor) need more time.
    // Double the per-batch timeout unless the user explicitly set --timeout.
    if args.all && args.timeout_sec.is_none() {
        timeout_sec *= 2.0;
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
        profile_str,
        rounds,
        warmup_rounds,
        samples_per_round,
        timeout_sec,
        rank_by_name(rank_by)
    );
    println!();

    let config = BenchConfig {
        samples_per_round,
        rounds,
        warmup_rounds,
        timeout_sec,
        rank_by,
        include_pool_quality: !args.no_pool,
        pool_quality_bytes: 65_536,
        conditioning: mode,
    };

    let report = match benchmark::benchmark_sources(&pool_instance, &config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Benchmark failed: {e}");
            std::process::exit(1);
        }
    };

    print_bench_table(&report.sources, config.rounds);

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

        let mode_samples = config.samples_per_round;
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

    if let Some(ref pool_quality) = report.pool {
        println!("\n{}", "=".repeat(68));
        println!(
            "Pool Output Quality (conditioning: {})\n",
            args.conditioning
        );
        println!("  Conditioned output: {} bytes", pool_quality.bytes);
        println!(
            "  Shannon entropy: {:.4} / 8.0 bits/byte",
            pool_quality.shannon_entropy
        );
        println!(
            "  Min-entropy H∞:  {:.4} / 8.0 bits/byte",
            pool_quality.min_entropy
        );
        println!(
            "\n  {}/{} sources healthy",
            pool_quality.healthy_sources, pool_quality.total_sources
        );
    }

    let telemetry_report = telemetry.finish();
    if let Some(ref window) = telemetry_report {
        super::telemetry::print_window_summary("bench", window);
    }

    if let Some(path) = args.output.as_deref() {
        let cli_report = CliBenchReport {
            generated_unix: report.generated_unix,
            profile: profile_str.to_string(),
            conditioning: args.conditioning.to_string(),
            rank_by: rank_by_name(config.rank_by).to_string(),
            settings: CliBenchSettings {
                samples_per_round: config.samples_per_round,
                rounds: config.rounds,
                warmup_rounds: config.warmup_rounds,
                timeout_sec: config.timeout_sec,
            },
            sources: report.sources,
            pool: report.pool,
            telemetry_v1: telemetry_report,
        };

        super::write_json(&cli_report, path, "Benchmark report");
    }
}

fn print_bench_table(sources: &[benchmark::BenchSourceReport], total_rounds: usize) {
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
    for src in sources {
        let state = if src.success_rounds == 0 {
            "UNSTABLE"
        } else if src.success_rounds < total_rounds {
            "SLOW"
        } else {
            "OK"
        };
        let display_name = if src.composite {
            format!("{} [C]", src.name)
        } else {
            src.name.clone()
        };
        let rounds_str = format!("{}/{}", src.success_rounds, total_rounds);
        println!(
            "{:<26} {:>5} {:>7.3} {:>7.3} {:>10.1} {:>6.3} {:>9.1} {:>9.2} {:>10} {:>6} {:>8}",
            display_name,
            src.grade,
            src.avg_shannon,
            src.avg_min_entropy,
            src.avg_throughput_bps / 1024.0,
            src.avg_autocorrelation,
            src.p99_latency_ms,
            src.stability,
            rounds_str,
            src.failures,
            state,
        );
    }
    println!();
    println!("Grade is based on min-entropy (H∞), not Shannon.");
    println!("r₁ = lag-1 autocorrelation (0 = ideal, ±1 = fully correlated).");
    println!("Stability is derived from run-to-run min-entropy consistency (1.0 = most stable).");
}

fn parse_profile_name(s: &str) -> &'static str {
    match s {
        "quick" => "quick",
        "deep" => "deep",
        _ => "standard",
    }
}

fn profile_defaults(s: &str) -> (usize, usize, usize, f64) {
    match s {
        "quick" => (2048, 3, 1, 2.0),
        "deep" => (16384, 10, 2, 6.0),
        _ => (4096, 5, 1, 3.0),
    }
}

fn parse_rank_by(s: &str) -> RankBy {
    match s {
        "min_entropy" => RankBy::MinEntropy,
        "throughput" => RankBy::Throughput,
        _ => RankBy::Balanced,
    }
}

fn rank_by_name(r: RankBy) -> &'static str {
    match r {
        RankBy::Balanced => "balanced",
        RankBy::MinEntropy => "min_entropy",
        RankBy::Throughput => "throughput",
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
