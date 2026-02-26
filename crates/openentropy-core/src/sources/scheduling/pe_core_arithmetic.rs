//! P-core / E-core arithmetic migration timing entropy.
//!
//! Apple Silicon M-series chips have two CPU cluster types: high-performance
//! P-cores and high-efficiency E-cores, each on separate DVFS domains with
//! independent voltage and frequency controllers. The kernel migrates threads
//! between clusters based on thermal state, QoS hint, and system-wide load.
//!
//! ## Physics
//!
//! A tight arithmetic loop running on a P-core completes in ~57µs. The same
//! loop on an E-core takes ~2-4x longer. When the scheduler migrates the
//! thread mid-measurement, the timing jumps non-deterministically. Even
//! without migration, each cluster's DVFS controller adjusts frequency
//! independently based on thermal sensors — two subsequent runs of the same
//! loop on the same core can differ because the core's frequency changed.
//!
//! Empirically this source produces Shannon entropy of **6.35+ bits/byte**
//! in the low 8 bits of timing deltas, with near-perfect LSB bias (~0.499)
//! and ~49.5% transitions — indistinguishable from a fair coin at the bit
//! level. This makes it one of the highest-entropy software-accessible sources
//! on Apple Silicon without any special hardware access.
//!
//! ## Why this is not documented
//!
//! CPU timing is treated as deterministic by hardware vendors. DVFS and core
//! migration are implementation details of the power management subsystem.
//! No hardware entropy specification mentions arithmetic timing as an entropy
//! source, yet the physical variance is genuine and substantial.

use std::time::Instant;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static PE_CORE_INFO: SourceInfo = SourceInfo {
    name: "pe_core_arithmetic",
    description: "P-core/E-core migration timing entropy from arithmetic loop jitter",
    physics: "Times a fixed-iteration arithmetic loop (LCG multiply-add). On Apple Silicon, \
              the kernel migrates threads between high-performance P-cores and \
              high-efficiency E-cores based on thermal state, QoS, and load. Migration \
              events cause 2-4x timing jumps. Even without migration, each cluster's \
              independent DVFS controller adjusts frequency based on die temperature, \
              making subsequent runs of the same loop non-deterministic. Measured Shannon \
              entropy: 6.35+ bits/byte in low 8 bits of timing deltas. LSB bias ~0.499, \
              transitions ~49.5% — near-ideal entropy at the bit level. No special \
              hardware access or permissions required.",
    category: SourceCategory::Scheduling,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 6.0,
    composite: false,
    is_fast: false,
};

/// Number of LCG iterations per timing sample.
///
/// Must be large enough that the loop takes >1µs (so timing resolution
/// dominates over measurement overhead), but short enough to collect
/// many samples quickly. 4096 iterations ≈ 57µs on a P-core.
const LCG_ITERS: u64 = 4096;

/// LCG multiplier (Knuth).
const LCG_MUL: u64 = 6364136223846793005;
/// LCG addend (Knuth).
const LCG_ADD: u64 = 1442695040888963407;

/// Entropy source: arithmetic timing jitter from P/E core DVFS and migration.
pub struct PECoreArithmeticSource;

impl EntropySource for PECoreArithmeticSource {
    fn info(&self) -> &SourceInfo {
        &PE_CORE_INFO
    }

    fn is_available(&self) -> bool {
        // Works on any multi-cluster CPU. Entropy is highest on Apple Silicon
        // but the source functions correctly on any platform.
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Each timing value carries ~6 bits of entropy in the low 8 bits.
        // Oversample 4x so extract_timing_entropy has plenty to work with.
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Warm up: give the scheduler a chance to place us on a stable core,
        // and let branch predictors settle. Discard these timings.
        let mut sink: u64 = 0xDEAD_BEEF_CAFE_BABE;
        for _ in 0..16 {
            for _ in 0..LCG_ITERS {
                sink = sink.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
            }
        }
        // Prevent the compiler from optimising out the warm-up loop.
        std::hint::black_box(sink);

        for _ in 0..raw_count {
            let t0 = Instant::now();

            // Fixed-work arithmetic loop. The LCG is a data dependency chain —
            // each iteration depends on the previous, preventing ILP and making
            // the loop duration sensitive to the core's actual clock frequency.
            let mut x: u64 = sink;
            for _ in 0..LCG_ITERS {
                x = x.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
            }
            // Keep the result live to prevent dead-code elimination.
            sink = std::hint::black_box(x);

            let elapsed = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed);

            // Yield to the scheduler periodically.
            // This increases the probability of P→E or E→P migration events,
            // which are the dominant entropy source.
            if timings.len().is_multiple_of(64) {
                std::thread::yield_now();
            }
        }

        // Prevent the compiler from optimising out the measurement loops.
        std::hint::black_box(sink);

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = PECoreArithmeticSource;
        assert_eq!(src.info().name, "pe_core_arithmetic");
        assert!(matches!(src.info().category, SourceCategory::Scheduling));
        assert!(!src.info().composite);
    }

    #[test]
    fn is_available() {
        assert!(PECoreArithmeticSource.is_available());
    }

    #[test]
    #[ignore] // Timing-dependent
    fn collects_bytes() {
        let src = PECoreArithmeticSource;
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(
            unique.len() > 4,
            "expected high byte diversity from PE core timing"
        );
    }
}
