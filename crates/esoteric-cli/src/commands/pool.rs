use esoteric_core::conditioning::quick_shannon;

pub fn run(source_filter: Option<&str>) {
    let pool = super::make_pool(source_filter);
    println!("Pool created with {} sources", pool.source_count());
    println!("Collecting entropy...");

    let raw = pool.collect_all();
    println!("Raw entropy: {} bytes", raw);

    let output = pool.get_random_bytes(1024);
    let h = quick_shannon(&output);
    println!("\nConditioned output: 1024 bytes");
    println!("  Shannon entropy: {:.4} / 8.0 bits/byte", h);

    let health = pool.health_report();
    println!("\n{}/{} sources healthy", health.healthy, health.total);
    for src in &health.sources {
        let grade = if src.entropy >= 7.5 { "A" } else if src.entropy >= 6.0 { "B" } else if src.entropy >= 4.0 { "C" } else if src.entropy >= 2.0 { "D" } else { "F" };
        let status = if src.healthy { "✓" } else { "✗" };
        println!("  {} {} {:<25} H={:.3}  {:.2}s", status, grade, src.name, src.entropy, src.time);
    }
}
