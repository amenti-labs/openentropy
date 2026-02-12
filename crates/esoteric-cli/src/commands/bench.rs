use esoteric_core::conditioning::quick_quality;
use esoteric_core::platform::detect_available_sources;
use std::time::Instant;

pub fn run() {
    let sources = detect_available_sources();
    println!("Benchmarking {} sources...\n", sources.len());

    let mut results = Vec::new();

    for src in &sources {
        let info = src.info();
        let t0 = Instant::now();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| src.collect(5000))) {
            Ok(data) if !data.is_empty() => {
                let elapsed = t0.elapsed().as_secs_f64();
                let q = quick_quality(&data);
                println!(
                    "  {} {:<25} H={:.3}  {:.2}s",
                    q.grade, info.name, q.shannon_entropy, elapsed
                );
                results.push((info.name, q, elapsed));
            }
            Ok(_) => {
                println!("  ✗ {:<25} no data", info.name);
            }
            Err(_) => {
                println!("  ✗ {:<25} error", info.name);
            }
        }
    }

    results.sort_by(|a, b| b.1.quality_score.partial_cmp(&a.1.quality_score).unwrap());

    println!("\n{}", "=".repeat(60));
    println!(
        "{:<25} {:>5} {:>6} {:>8} {:>9}",
        "Source", "Grade", "Score", "Shannon", "Compress"
    );
    println!("{}", "-".repeat(60));
    for (name, q, _) in &results {
        println!(
            "{:<25} {:>5} {:>6.1} {:>7.3} {:>8.3}",
            name, q.grade, q.quality_score, q.shannon_entropy, q.compression_ratio
        );
    }
}
