use openentropy_core::conditioning::min_entropy_estimate;

pub fn run(source_filter: Option<&str>, conditioning: &str) {
    let mode = super::parse_conditioning(conditioning);
    let pool = super::make_pool(source_filter);
    println!(
        "Collecting entropy from {} sources...\n",
        pool.source_count()
    );

    pool.collect_all();
    let output = pool.get_bytes(4096, mode);

    let report = min_entropy_estimate(&output);
    println!(
        "Conditioned pool output ({} bytes, mode: {conditioning}):\n",
        output.len()
    );
    print!("{report}");

    println!("\n─────────────────────────────────");
    println!("Per-source min-entropy (raw, unconditioned):\n");

    let health = pool.health_report();
    let mut sources: Vec<_> = health.sources.iter().collect();
    sources.sort_by(|a, b| b.min_entropy.partial_cmp(&a.min_entropy).unwrap());

    println!("  {:<25} {:>8} {:>8}", "Source", "Shannon", "Min-H∞");
    println!("  {}", "-".repeat(45));
    for src in &sources {
        let min_h = src.min_entropy.max(0.0);
        println!("  {:<25} {:>7.3} {:>7.3}", src.name, src.entropy, min_h);
    }

    println!("\nNote: Min-entropy (H∞) is always ≤ Shannon entropy (H).");
    println!("H∞ reflects the probability of guessing the most likely byte value.");
    println!("For security applications, use H∞ as the conservative bound.");
}
