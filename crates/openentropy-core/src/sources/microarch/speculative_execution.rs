//! Speculative execution / branch predictor state timing entropy source.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Measures timing variations from the CPU's speculative execution engine. The
/// branch predictor maintains per-address history that depends on ALL
/// previously executed code. Mispredictions cause pipeline flushes (~15 cycle
/// penalty on M4). By running data-dependent branches and measuring timing, we
/// capture the predictor's internal state which is influenced by all prior
/// program activity on the core.
pub struct SpeculativeExecutionSource;

static SPECULATIVE_EXECUTION_INFO: SourceInfo = SourceInfo {
    name: "speculative_execution",
    description: "Branch predictor state timing via data-dependent branches",
    physics: "Measures timing variations from the CPU's speculative execution engine. \
              The branch predictor maintains per-address history that depends on ALL \
              previously executed code. Mispredictions cause pipeline flushes (~15 cycle \
              penalty on M4). By running data-dependent branches and measuring timing, \
              we capture the predictor's internal state.",
    category: SourceCategory::Microarch,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2.5,
    composite: false,
    is_fast: true,
};

impl EntropySource for SpeculativeExecutionSource {
    fn info(&self) -> &SourceInfo {
        &SPECULATIVE_EXECUTION_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let num_batches = n_samples + 64;
        let mut timings = Vec::with_capacity(num_batches);

        // LCG state — seeded from the high-resolution clock so every call
        // exercises a different branch sequence.
        let mut lcg_state: u64 = mach_time() ^ 0xDEAD_BEEF_CAFE_BABE;

        for _batch_idx in 0..num_batches {
            // Randomize batch size via LCG (10-40 iterations) so the
            // branch predictor can't learn the pattern.
            lcg_state = lcg_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let batch_size = 10 + ((lcg_state >> 48) as usize % 31);

            let t0 = mach_time();

            // Execute a batch of data-dependent branches that defeat the
            // branch predictor because outcomes depend on runtime LCG values.
            let mut accumulator: u64 = 0;
            for _ in 0..batch_size {
                // Advance LCG: x' = x * 6364136223846793005 + 1442695040888963407
                lcg_state = lcg_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);

                // Data-dependent branch — outcome unknowable to predictor.
                if lcg_state & 0x8000_0000 != 0 {
                    accumulator = accumulator.wrapping_add(lcg_state);
                } else {
                    accumulator = accumulator.wrapping_mul(lcg_state | 1);
                }

                // Second branch: advance LCG independently for decorrelation.
                lcg_state = lcg_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if lcg_state & 0x8000_0000 != 0 {
                    accumulator ^= lcg_state.rotate_left(7);
                } else {
                    accumulator ^= lcg_state.rotate_right(11);
                }

                // Third branch: advance LCG again for independent outcome.
                lcg_state = lcg_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if lcg_state & 0x1 != 0 {
                    accumulator = accumulator.wrapping_add(lcg_state >> 32);
                }
            }

            // Prevent the compiler from optimizing away the computation.
            std::hint::black_box(accumulator);

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn speculative_execution_collects_bytes() {
        let src = SpeculativeExecutionSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
        assert!(data.len() <= 128);
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes were identical");
        }
    }

    #[test]
    fn source_info_category() {
        assert_eq!(
            SpeculativeExecutionSource.info().category,
            SourceCategory::Microarch
        );
    }

    #[test]
    fn source_info_name() {
        assert_eq!(SpeculativeExecutionSource.name(), "speculative_execution");
    }
}
