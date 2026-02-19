//! Basic entropy collection example.
//!
//! Auto-detects available hardware entropy sources, collects bytes,
//! and prints them as hex.
//!
//! Run: `cargo run --example basic`

use openentropy_core::EntropyPool;

fn main() {
    // Create a pool with all available sources on this machine
    let pool = EntropyPool::auto();

    println!("Sources registered: {}", pool.source_count());

    // Collect from all sources (parallel, 10s timeout)
    let bytes_collected = pool.collect_all();
    println!("Raw bytes collected: {bytes_collected}");

    // Get 64 bytes of conditioned (SHA-256) random output
    let random = pool.get_random_bytes(64);
    print!("Random bytes (hex): ");
    for b in &random {
        print!("{b:02x}");
    }
    println!();

    // Print health report
    let health = pool.health_report();
    println!(
        "\nPool health: {}/{} sources healthy, {} raw bytes total",
        health.healthy, health.total, health.raw_bytes
    );
}
