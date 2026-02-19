//! Compare Raw, VonNeumann, and Sha256 conditioning modes.
//!
//! Shows the difference between unconditioned hardware noise and
//! cryptographically conditioned output.
//!
//! Run: `cargo run --example raw_vs_conditioned`

use openentropy_core::{ConditioningMode, EntropyPool, quick_shannon};

fn main() {
    let pool = EntropyPool::auto();
    println!("Sources: {}\n", pool.source_count());

    // Collect entropy first
    pool.collect_all();

    let n = 256;

    // Raw — XOR-combined only, no whitening
    let raw = pool.get_bytes(n, ConditioningMode::Raw);
    let raw_shannon = quick_shannon(&raw);
    println!("Raw (unconditioned):");
    print_hex_line(&raw, 32);
    println!("  Shannon entropy: {raw_shannon:.3} bits/byte\n");

    // VonNeumann — debiased but structure-preserving
    let vn = pool.get_bytes(n, ConditioningMode::VonNeumann);
    let vn_shannon = quick_shannon(&vn);
    println!("VonNeumann (debiased):");
    print_hex_line(&vn, 32);
    println!("  Shannon entropy: {vn_shannon:.3} bits/byte\n");

    // SHA-256 — full cryptographic conditioning
    let sha = pool.get_bytes(n, ConditioningMode::Sha256);
    let sha_shannon = quick_shannon(&sha);
    println!("SHA-256 (conditioned):");
    print_hex_line(&sha, 32);
    println!("  Shannon entropy: {sha_shannon:.3} bits/byte\n");

    println!("---");
    println!("Raw preserves the actual hardware noise signal.");
    println!("SHA-256 produces cryptographic-quality output (H ≈ 8.0).");
}

fn print_hex_line(data: &[u8], max: usize) {
    let show = data.len().min(max);
    print!("  ");
    for b in &data[..show] {
        print!("{b:02x}");
    }
    if data.len() > max {
        print!("...");
    }
    println!();
}
