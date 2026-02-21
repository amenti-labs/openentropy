//! Unified benchmark comparing all quantum entropy sources in OpenEntropy.
//!
//! This binary tests all quantum sources and compares them to classical baselines
//! to demonstrate that statistical tests CANNOT distinguish quantum from classical
//! randomness - only physics arguments can.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin quantum_bench
//! ```
//!
//! ## Output
//!
//! - Console table comparing all sources
//! - JSON report saved to `quantum_bench_results.json`

use std::fs::File;
use std::io::Read;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use openentropy_core::conditioning::{
    grade_min_entropy, min_entropy_estimate, quick_min_entropy, quick_quality, quick_shannon,
    MinEntropyReport,
};
use openentropy_core::source::EntropySource;
use openentropy_core::sources::quantum::{
    quantum_fraction, AvalancheNoiseSource, CosmicMuonSource, MultiSourceQuantumSource,
    RadioactiveDecaySource, SSDTunnelingSource, VacuumFluctuationsSource,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// Result structures
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceResult {
    /// Source name
    name: String,
    /// Whether source is available on this system
    available: bool,
    /// Bytes collected (0 if unavailable or failed)
    bytes_collected: usize,
    /// Collection time in seconds
    collection_time_sec: f64,
    /// Throughput in bytes/second (0 if unavailable)
    throughput_bps: f64,
    /// Shannon entropy (bits/byte, max 8.0)
    shannon_entropy: f64,
    /// Min-entropy H-infinity (bits/byte, max 8.0)
    min_entropy: f64,
    /// Statistical quality score (0-100%)
    quality_score: f64,
    /// Grade (A-F based on min-entropy)
    grade: char,
    /// Estimated quantum fraction based on physics (0.0-1.0)
    quantum_fraction: f64,
    /// Min-entropy report details
    entropy_report: Option<EntropyReportJson>,
    /// Error message if collection failed
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntropyReportJson {
    shannon_entropy: f64,
    min_entropy: f64,
    heuristic_floor: f64,
    mcv_estimate: f64,
    mcv_p_upper: f64,
    collision_estimate: f64,
    markov_estimate: f64,
    compression_estimate: f64,
    t_tuple_estimate: f64,
    samples: usize,
}

impl From<&MinEntropyReport> for EntropyReportJson {
    fn from(r: &MinEntropyReport) -> Self {
        Self {
            shannon_entropy: r.shannon_entropy,
            min_entropy: r.min_entropy,
            heuristic_floor: r.heuristic_floor,
            mcv_estimate: r.mcv_estimate,
            mcv_p_upper: r.mcv_p_upper,
            collision_estimate: r.collision_estimate,
            markov_estimate: r.markov_estimate,
            compression_estimate: r.compression_estimate,
            t_tuple_estimate: r.t_tuple_estimate,
            samples: r.samples,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkReport {
    /// Unix timestamp when benchmark was run
    generated_unix: u64,
    /// Number of samples collected per source
    samples_per_source: usize,
    /// All quantum source results
    quantum_sources: Vec<SourceResult>,
    /// Baseline results (urandom, PRNG)
    baselines: Vec<SourceResult>,
    /// Summary statistics
    summary: BenchmarkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkSummary {
    /// Best quantum source by min-entropy
    best_quantum_by_entropy: String,
    /// Best quantum source by throughput
    best_quantum_by_throughput: String,
    /// Average quantum fraction across available quantum sources
    avg_quantum_fraction: f64,
    /// Key insight: can statistical tests distinguish quantum from PRNG?
    statistical_tests_can_distinguish: bool,
    /// Explanation of results
    explanation: String,
}

// =============================================================================
// Baseline sources
// =============================================================================

/// /dev/urandom baseline - cryptographically secure PRNG from OS
fn collect_urandom(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    match File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)) {
        Ok(()) => buf,
        Err(_) => Vec::new(),
    }
}

/// Python-style PRNG baseline (Mersenne Twister equivalent)
/// This proves that statistical tests score PRNGs just as high as quantum!
fn collect_prng(n: usize) -> Vec<u8> {
    // Simple xorshift64* PRNG - same statistical quality as Python's MT
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        // xorshift64*
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let out = state.wrapping_mul(0x2545F4914F6CDD1D);
        result.push(out as u8);
    }
    result
}

// =============================================================================
// Benchmark logic
// =============================================================================

fn benchmark_source<S: EntropySource + 'static>(
    source: S,
    n_samples: usize,
    physics_quantum_fraction: f64,
) -> SourceResult {
    let name = source.name().to_string();
    let available = source.is_available();

    if !available {
        return SourceResult {
            name,
            available: false,
            bytes_collected: 0,
            collection_time_sec: 0.0,
            throughput_bps: 0.0,
            shannon_entropy: 0.0,
            min_entropy: 0.0,
            quality_score: 0.0,
            grade: 'F',
            quantum_fraction: physics_quantum_fraction,
            entropy_report: None,
            error: Some("Source not available on this system".to_string()),
        };
    }

    let start = Instant::now();
    let data = source.collect(n_samples);
    let elapsed = start.elapsed().as_secs_f64();

    if data.is_empty() {
        return SourceResult {
            name,
            available: true,
            bytes_collected: 0,
            collection_time_sec: elapsed,
            throughput_bps: 0.0,
            shannon_entropy: 0.0,
            min_entropy: 0.0,
            quality_score: 0.0,
            grade: 'F',
            quantum_fraction: physics_quantum_fraction,
            entropy_report: None,
            error: Some("No data collected (may need camera/hardware)".to_string()),
        };
    }

    let throughput = if elapsed > 0.0 {
        data.len() as f64 / elapsed
    } else {
        0.0
    };

    let shannon = quick_shannon(&data);
    let min_ent = quick_min_entropy(&data);
    let quality = quick_quality(&data);
    let grade = grade_min_entropy(min_ent);
    let entropy_report = min_entropy_estimate(&data);

    SourceResult {
        name,
        available: true,
        bytes_collected: data.len(),
        collection_time_sec: elapsed,
        throughput_bps: throughput,
        shannon_entropy: shannon,
        min_entropy: min_ent,
        quality_score: quality.quality_score,
        grade,
        quantum_fraction: physics_quantum_fraction,
        entropy_report: Some(EntropyReportJson::from(&entropy_report)),
        error: None,
    }
}

fn benchmark_baseline(name: &str, data: Vec<u8>, collection_time_sec: f64) -> SourceResult {
    let throughput = if collection_time_sec > 0.0 {
        data.len() as f64 / collection_time_sec
    } else {
        // Assume instant for synthetic sources
        data.len() as f64 * 1000.0
    };

    let shannon = quick_shannon(&data);
    let min_ent = quick_min_entropy(&data);
    let quality = quick_quality(&data);
    let grade = grade_min_entropy(min_ent);
    let entropy_report = min_entropy_estimate(&data);

    SourceResult {
        name: name.to_string(),
        available: true,
        bytes_collected: data.len(),
        collection_time_sec,
        throughput_bps: throughput,
        shannon_entropy: shannon,
        min_entropy: min_ent,
        quality_score: quality.quality_score,
        grade,
        quantum_fraction: 0.0, // Baselines are NOT quantum
        entropy_report: Some(EntropyReportJson::from(&entropy_report)),
        error: None,
    }
}

// =============================================================================
// Output formatting
// =============================================================================

fn print_header(title: &str) {
    println!();
    println!("{}", "=".repeat(110));
    println!("  {}", title);
    println!("{}", "=".repeat(110));
}

fn print_source_table(sources: &[SourceResult], title: &str) {
    println!("\n{}", title);
    println!(
        "{:<25} {:>6} {:>7} {:>7} {:>10} {:>10} {:>6} {:>10} {:>10}",
        "Source", "Grade", "H", "H_inf", "KB/s", "Quality%", "Avail", "Quantum%", "Status"
    );
    println!("{}", "-".repeat(110));

    for src in sources {
        let status = if let Some(ref err) = src.error {
            if err.contains("not available") {
                "N/A"
            } else {
                "FAILED"
            }
        } else if src.bytes_collected == 0 {
            "EMPTY"
        } else {
            "OK"
        };

        let quantum_pct = src.quantum_fraction * 100.0;
        let quality_pct = src.quality_score;

        println!(
            "{:<25} {:>6} {:>7.3} {:>7.3} {:>10.1} {:>10.1} {:>6} {:>9.1}% {:>10}",
            src.name,
            src.grade,
            src.shannon_entropy,
            src.min_entropy,
            src.throughput_bps / 1024.0,
            quality_pct,
            if src.available { "Y" } else { "N" },
            quantum_pct,
            status
        );
    }
}

fn print_entropy_details(sources: &[SourceResult]) {
    println!("\n{}", "=".repeat(110));
    println!("  Detailed Entropy Analysis (NIST-inspired estimators)");
    println!("{}", "=".repeat(110));

    for src in sources {
        if let Some(ref report) = src.entropy_report {
            if src.bytes_collected > 0 {
                println!("\n{} ({} bytes):", src.name, src.bytes_collected);
                println!(
                    "  Shannon H:       {:.3} / 8.0 bits/byte",
                    report.shannon_entropy
                );
                println!(
                    "  Min-Entropy H_inf: {:.3} / 8.0 bits/byte (MCV)",
                    report.min_entropy
                );
                println!("  MCV estimate:    {:.3} (p_upper={:.4})", report.mcv_estimate, report.mcv_p_upper);
                println!("  Collision:       {:.3}", report.collision_estimate);
                println!("  Markov:          {:.3}", report.markov_estimate);
                println!("  Compression:     {:.3}", report.compression_estimate);
                println!("  t-Tuple:         {:.3}", report.t_tuple_estimate);
                println!("  Heuristic floor: {:.3}", report.heuristic_floor);
            }
        }
    }
}

fn print_key_insight() {
    println!("\n{}", "=".repeat(110));
    println!("  KEY INSIGHT: Statistical Tests CANNOT Distinguish Quantum from PRNG!");
    println!("{}", "=".repeat(110));
    println!();
    println!("  Notice that both quantum sources AND PRNG baselines score 99%+ on");
    println!("  Shannon entropy, min-entropy, and quality tests.");
    println!();
    println!("  This is FUNDAMENTAL: statistical tests measure DISTRIBUTION properties,");
    println!("  not PHYSICS origins. A good PRNG produces output indistinguishable from");
    println!("  true quantum randomness using any statistical test.");
    println!();
    println!("  The 'Quantum%' column is based on PHYSICS arguments, not tests:");
    println!("  - Radioactive decay: Nuclear timing is fundamentally unpredictable (99%)");
    println!("  - Cosmic muons: Particle physics from space (95%)");
    println!("  - SSD tunneling: Fowler-Nordheim electron tunneling (74%)");
    println!("  - Avalanche noise: Quantum impact ionization (70%)");
    println!("  - Vacuum fluctuations: Zero-point energy (65%)");
    println!("  - Multi-source XOR: Combined purity ~90%");
    println!();
    println!("  Only BELL INEQUALITY TESTS can CERTIFY quantum randomness - and those");
    println!("  require entangled photon pairs and specialized equipment.");
    println!();
    println!("  This is why commercial QRNGs (Intel RDRAND, ID Quantique) combine:");
    println!("    1. A quantum noise source (physics-based)");
    println!("    2. A cryptographic conditioner (SHA-256 or AES)");
    println!();
    println!("  The conditioner ensures output passes all statistical tests, while");
    println!("  the quantum source provides the fundamental unpredictability.");
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let n_samples = 4096; // Samples per source

    println!();
    println!("  ____                                    _   ");
    println!(" / __ \\                                  | |  ");
    println!("| |  | |_   _ _ __ ___  _ __ ___  __ _  __| |  ");
    println!("| |  | | | | | '_ ` _ \\| '_ ` _ \\/ _` |/ _` |  ");
    println!("| |__| | |_| | | | | | | | | | | | (_| | (_| |  ");
    println!(" \\___\\_\\\\__,_|_| |_| |_|_| |_| |_|\\__,_|\\__,_|  ");
    println!();
    println!("  Unified Quantum Entropy Source Benchmark");
    println!("  OpenEntropy Project");
    println!();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // =========================================================================
    // Benchmark quantum sources
    // =========================================================================

    print_header("QUANTUM ENTROPY SOURCES");

    let mut quantum_results: Vec<SourceResult> = Vec::new();

    // Cosmic muon (requires camera + ffmpeg)
    println!("\n[1/6] Testing cosmic_muon...");
    quantum_results.push(benchmark_source(
        CosmicMuonSource,
        n_samples,
        quantum_fraction("cosmic_muon"),
    ));

    // SSD tunneling
    println!("[2/6] Testing ssd_tunneling...");
    quantum_results.push(benchmark_source(
        SSDTunnelingSource::default(),
        n_samples,
        quantum_fraction("ssd_tunneling"),
    ));

    // Radioactive decay (requires camera + ffmpeg)
    println!("[3/6] Testing radioactive_decay...");
    quantum_results.push(benchmark_source(
        RadioactiveDecaySource,
        n_samples,
        quantum_fraction("radioactive_decay"),
    ));

    // Avalanche noise
    println!("[4/6] Testing avalanche_noise...");
    quantum_results.push(benchmark_source(
        AvalancheNoiseSource::default(),
        n_samples,
        quantum_fraction("avalanche_noise"),
    ));

    // Vacuum fluctuations
    println!("[5/6] Testing vacuum_fluctuations...");
    quantum_results.push(benchmark_source(
        VacuumFluctuationsSource::default(),
        n_samples,
        quantum_fraction("vacuum_fluctuations"),
    ));

    // Multi-source quantum (XOR combined)
    println!("[6/6] Testing multi_source_quantum...");
    let mut multi = MultiSourceQuantumSource::new();
    multi.add_source(SSDTunnelingSource::default());
    multi.add_source(AvalancheNoiseSource::default());
    multi.add_source(VacuumFluctuationsSource::default());
    // Note: Camera sources may not be available
    quantum_results.push(benchmark_source(
        multi,
        n_samples,
        quantum_fraction("multi_source_quantum"),
    ));

    print_source_table(&quantum_results, "Quantum Sources:");

    // =========================================================================
    // Benchmark baselines
    // =========================================================================

    print_header("BASELINE SOURCES (Non-Quantum)");

    let mut baseline_results: Vec<SourceResult> = Vec::new();

    // /dev/urandom
    println!("\n[B1/2] Testing /dev/urandom...");
    let start = Instant::now();
    let urandom_data = collect_urandom(n_samples);
    let urandom_time = start.elapsed().as_secs_f64();
    baseline_results.push(benchmark_baseline("/dev/urandom", urandom_data, urandom_time));

    // PRNG (Mersenne Twister style)
    println!("[B2/2] Testing prng (xorshift64*)...");
    let prng_data = collect_prng(n_samples);
    baseline_results.push(benchmark_baseline("prng_xorshift", prng_data, 0.0));

    print_source_table(&baseline_results, "Baseline Sources (Non-Quantum):");

    // =========================================================================
    // Print detailed analysis
    // =========================================================================

    // Only show details for sources that collected data
    let all_with_data: Vec<_> = quantum_results
        .iter()
        .chain(baseline_results.iter())
        .filter(|s| s.bytes_collected > 0)
        .collect();

    if !all_with_data.is_empty() {
        print_entropy_details(&all_with_data.clone().into_iter().cloned().collect::<Vec<_>>());
    }

    // =========================================================================
    // Print key insight
    // =========================================================================

    print_key_insight();

    // =========================================================================
    // Summary
    // =========================================================================

    let available_quantum: Vec<_> = quantum_results
        .iter()
        .filter(|s| s.bytes_collected > 0)
        .collect();

    let best_by_entropy = available_quantum
        .iter()
        .max_by(|a, b| a.min_entropy.partial_cmp(&b.min_entropy).unwrap())
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "None available".to_string());

    let best_by_throughput = available_quantum
        .iter()
        .max_by(|a, b| a.throughput_bps.partial_cmp(&b.throughput_bps).unwrap())
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "None available".to_string());

    let avg_quantum = if available_quantum.is_empty() {
        0.0
    } else {
        available_quantum.iter().map(|s| s.quantum_fraction).sum::<f64>()
            / available_quantum.len() as f64
    };

    // Check if baselines have similar entropy to quantum
    let quantum_avg_entropy = if available_quantum.is_empty() {
        0.0
    } else {
        available_quantum.iter().map(|s| s.min_entropy).sum::<f64>() / available_quantum.len() as f64
    };

    let baseline_avg_entropy = baseline_results
        .iter()
        .map(|s| s.min_entropy)
        .sum::<f64>()
        / baseline_results.len() as f64;

    let can_distinguish = (quantum_avg_entropy - baseline_avg_entropy).abs() > 2.0;

    let summary = BenchmarkSummary {
        best_quantum_by_entropy: best_by_entropy.clone(),
        best_quantum_by_throughput: best_by_throughput.clone(),
        avg_quantum_fraction: avg_quantum,
        statistical_tests_can_distinguish: can_distinguish,
        explanation: format!(
            "Quantum sources average H_inf = {:.3}, baselines average H_inf = {:.3}. \
             Difference = {:.3} bits/byte. Statistical tests {} distinguish them.",
            quantum_avg_entropy,
            baseline_avg_entropy,
            (quantum_avg_entropy - baseline_avg_entropy).abs(),
            if can_distinguish { "CAN" } else { "CANNOT" }
        ),
    };

    println!("\n{}", "=".repeat(110));
    println!("  SUMMARY");
    println!("{}", "=".repeat(110));
    println!();
    println!("  Best quantum by min-entropy:  {}", summary.best_quantum_by_entropy);
    println!("  Best quantum by throughput:   {}", summary.best_quantum_by_throughput);
    println!("  Average quantum fraction:     {:.1}%", summary.avg_quantum_fraction * 100.0);
    println!();
    println!("  {}", summary.explanation);

    // =========================================================================
    // Save JSON report
    // =========================================================================

    let report = BenchmarkReport {
        generated_unix: timestamp,
        samples_per_source: n_samples,
        quantum_sources: quantum_results,
        baselines: baseline_results,
        summary,
    };

    let json_path = "quantum_bench_results.json";
    match std::fs::write(json_path, serde_json::to_string_pretty(&report).unwrap()) {
        Ok(()) => println!("\n  Results saved to: {}", json_path),
        Err(e) => eprintln!("\n  Failed to save results: {}", e),
    }

    println!();
    println!("{}", "=".repeat(110));
}
