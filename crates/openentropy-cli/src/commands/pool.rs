use openentropy_core::conditioning::{quick_shannon, quick_min_entropy};

pub fn run(source_filter: Option<&str>, conditioning: &str) {
    let mode = super::parse_conditioning(conditioning);
    let pool = super::make_pool(source_filter);
    println!("Pool created with {} sources (conditioning: {conditioning})", pool.source_count());
    println!("Collecting entropy...");

    let raw = pool.collect_all();
    println!("Raw entropy: {} bytes", raw);

    let output = pool.get_bytes(1024, mode);
    let h = quick_shannon(&output);
    let h_min = quick_min_entropy(&output);
    println!("\nConditioned output: 1024 bytes");
    println!("  Shannon entropy: {:.4} / 8.0 bits/byte", h);
    println!("  Min-entropy H∞:  {:.4} / 8.0 bits/byte", h_min);

    let health = pool.health_report();
    println!("\n{}/{} sources healthy", health.healthy, health.total);
    println!(
        "  {:<25} {:>5} {:>7} {:>7} {:>7}",
        "Source", "Grade", "H", "H∞", "Time"
    );
    println!("  {}", "-".repeat(55));
    for src in &health.sources {
        let min_h = src.min_entropy.max(0.0);
        let grade = if min_h >= 6.0 { "A" } else if min_h >= 4.0 { "B" } else if min_h >= 2.0 { "C" } else if min_h >= 1.0 { "D" } else { "F" };
        let status = if src.healthy { "✓" } else { "✗" };
        println!(
            "  {} {} {:<25} {:>5.3} {:>5.3} {:>6.2}s",
            status, grade, src.name, src.entropy, min_h, src.time
        );
    }
}
