use esoteric_core::conditioning::quick_shannon;

pub fn run() {
    let pool = esoteric_core::EntropyPool::auto();
    println!("Pool created with {} sources", pool.source_count());
    println!("Collecting entropy...");

    let raw = pool.collect_all();
    println!("Raw entropy: {} bytes", raw);

    let output = pool.get_random_bytes(1024);
    let h = quick_shannon(&output);
    println!("\nConditioned output: 1024 bytes");
    println!("  Shannon entropy: {:.4} / 8.0 bits/byte", h);

    pool.print_health();
}
