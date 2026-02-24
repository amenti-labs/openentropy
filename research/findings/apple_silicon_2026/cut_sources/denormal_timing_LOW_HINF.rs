//! Floating-point denormal timing — data-dependent microcode timing jitter.
//!
//! Denormalized floating-point numbers (between 0 and `f64::MIN_POSITIVE`)
//! can cause data-dependent timing variation due to microcode assist or
//! hardware handling differences. This source times blocks of denormal
//! multiply-accumulate operations and extracts timing jitter.
//!

//! On Apple Silicon, denormal handling is fast (no microcode penalty),
//! but residual pipeline state and cache effects still create jitter.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Number of floating-point operations per timing measurement.
const OPS_PER_SAMPLE: usize = 100;

static DENORMAL_TIMING_INFO: SourceInfo = SourceInfo {
    name: "denormal_timing",
    description: "Floating-point denormal multiply-accumulate timing jitter",
    physics: "Times blocks of floating-point operations on denormalized values \
              (magnitudes between 0 and f64::MIN_POSITIVE). Denormals may trigger \
              microcode assists on some architectures, creating data-dependent timing. \
              Even on Apple Silicon where denormal handling is fast in hardware, \
              residual timing jitter comes from FPU pipeline state, cache line \
              alignment, and memory controller arbitration.",
    category: SourceCategory::Thermal,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 300.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from denormalized float operations.
pub struct DenormalTimingSource;

impl EntropySource for DenormalTimingSource {
    fn info(&self) -> &SourceInfo {
        &DENORMAL_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Pre-generate denormal values with varying mantissa patterns.
        let mut lcg: u64 = mach_time() | 1;
        let mut denormals = [0.0f64; OPS_PER_SAMPLE];
        for d in denormals.iter_mut() {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            // Construct denormal: exponent bits = 0, random mantissa
            let bits = lcg & 0x000F_FFFF_FFFF_FFFF_u64;
            *d = f64::from_bits(bits);
        }

        for _ in 0..raw_count {
            // Rotate denormal array slightly for per-iteration variation.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start_idx = (lcg >> 32) as usize % OPS_PER_SAMPLE;

            let mut acc = denormals[start_idx];

            let t0 = mach_time();
            for i in 0..OPS_PER_SAMPLE {
                let idx = (start_idx + i) % OPS_PER_SAMPLE;
                acc *= denormals[idx];
                acc += denormals[(idx + 1) % OPS_PER_SAMPLE];
            }
            let t1 = mach_time();

            // Prevent dead code elimination.
            std::hint::black_box(acc);
            timings.push(t1.wrapping_sub(t0));
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = DenormalTimingSource;
        assert_eq!(src.name(), "denormal_timing");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Timing-dependent
    fn collects_bytes() {
        let src = DenormalTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }
}
