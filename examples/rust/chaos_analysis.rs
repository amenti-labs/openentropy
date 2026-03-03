//! Chaos theory analysis example.
//!
//! Collects raw entropy from the default pool and runs five chaos theory
//! methods to check whether the output looks like true randomness or
//! deterministic chaos.
//!
//! Run: `cargo run --example chaos_analysis`

use openentropy_core::EntropyPool;
use openentropy_core::chaos::chaos_analysis;

fn main() {
    let pool = EntropyPool::auto();
    let _ = pool.collect_all();
    let data = pool.get_raw_bytes(10_000);

    println!("Collected {} raw bytes from pool\n", data.len());

    let result = chaos_analysis(&data);

    println!(
        "Hurst exponent:       H  = {:.4}  (0.5 = no memory)",
        result.hurst.hurst_exponent
    );
    println!(
        "Lyapunov exponent:    λ  = {:.4}  (near 0 = no divergence)",
        result.lyapunov.lyapunov_exponent
    );
    println!(
        "Correlation dimension D₂ = {:.4}  (>3 = high-dimensional)",
        result.correlation_dimension.dimension
    );
    println!(
        "BiEntropy:            BiEn = {:.4} (>0.95 = disordered)",
        result.bientropy.bien
    );
    println!(
        "Epiplexity:           ratio = {:.4} (>0.99 = incompressible)",
        result.epiplexity.compression_ratio
    );
}
