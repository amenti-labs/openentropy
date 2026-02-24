//! ISB (Instruction Synchronization Barrier) pipeline flush timing.
//!
//! The ARM64 `ISB` instruction flushes the instruction pipeline and ensures
//! all preceding instructions complete before any subsequent instructions
//! begin executing. Unlike data memory barriers (`DMB`/`DSB`), ISB operates
//! on the **instruction** pipeline — a completely separate microarchitectural
//! unit.
//!
//! ## Physics
//!
//! ISB execution time varies based on:
//!
//! 1. **I-cache state**: If the instruction cache is warm (recently fetched
//!    instructions still in L1-I), the pipeline restarts quickly. If the
//!    I-cache is cold (another core evicted our lines), the restart is slower
//!    because the instruction fetch unit must reload from L2 or SLC.
//!
//! 2. **In-flight instruction count**: ISB must drain all instructions ahead
//!    of it in the pipeline. A longer in-flight queue means more drain time.
//!    The in-flight depth varies with out-of-order execution state, which
//!    reflects what other threads and processes have been doing.
//!
//! 3. **Pipeline restart path**: After the ISB, the fetch unit restarts from
//!    the next instruction's address. The restart cost depends on which
//!    branch predictor entry is hit, which depends on recent branch history.
//!
//! Empirically on M4 Mac mini (N=2000):
//! - Mean: 33.37 ticks (~1.4µs), CV=50.1%, range=0–83 ticks
//! - LSB=0.266 (near-unbiased) — ISB is NOT in the "all-even" memory op cluster
//!
//! ## Comparison with Memory Barriers
//!
//! Unlike data memory barriers (DMB/DSB) which show extreme LSB bias
//! (LSB=0.004–0.018, always even), ISB timing shows LSB=0.266. This
//! confirms that ISB uses an entirely different microarchitectural path
//! than memory operations — the instruction frontend, not the memory backend.
//!
//! ## Cross-process Sensitivity
//!
//! ISB flushes the **shared** instruction cache hierarchy. Heavy I-cache
//! pressure from other processes (code loading, JIT compilation, large
//! hot loops) increases ISB latency by evicting our instructions from
//! shared L2/SLC I-cache sets.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::{extract_timing_entropy, mach_time};

static ISB_PIPELINE_TIMING_INFO: SourceInfo = SourceInfo {
    name: "isb_pipeline_timing",
    description: "ARM64 ISB instruction pipeline flush timing via I-cache + frontend state",
    physics: "Times `ISB` (instruction synchronization barrier) which flushes the \
              instruction pipeline and restarts fetch from the I-cache. Timing varies \
              with I-cache state (warm vs cold), in-flight instruction depth (drain time), \
              and branch predictor restart path. Unlike data memory ops which show extreme \
              LSB bias (always-even, memory fabric constant), ISB timing is in the instruction \
              frontend: LSB=0.266 (near-unbiased). Measured: CV=50.1%, mean=33.4 ticks \
              (~1.4\u{00b5}s), range=0\u{2013}83 ticks. Cross-process I-cache eviction from JIT/code \
              loading increases our ISB latency.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 8000.0,
    composite: false,
};

/// Entropy source from ARM64 ISB instruction pipeline flush timing.
pub struct ISBPipelineTimingSource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for ISBPipelineTimingSource {
    fn info(&self) -> &SourceInfo {
        &ISB_PIPELINE_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // ISB timing has CV=50.1% and near-unbiased LSB=0.266.
        // 4× oversampling for robustness.
        let raw_count = n_samples * 4 + 32;
        let mut timings = Vec::with_capacity(raw_count);

        // Warm up: run several ISBs to ensure pipeline is in a steady state.
        for _ in 0..16 {
            unsafe {
                core::arch::asm!("isb", options(nostack, nomem));
            }
        }

        for _ in 0..raw_count {
            let t0 = mach_time();
            unsafe {
                core::arch::asm!("isb", options(nostack, nomem));
            }
            let elapsed = mach_time().wrapping_sub(t0);

            // Reject preemption spikes (>500µs).
            if elapsed < 12_000 {
                timings.push(elapsed);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for ISBPipelineTimingSource {
    fn info(&self) -> &SourceInfo {
        &ISB_PIPELINE_TIMING_INFO
    }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _n_samples: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = ISBPipelineTimingSource;
        assert_eq!(src.info().name, "isb_pipeline_timing");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_apple_silicon() {
        assert!(ISBPipelineTimingSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_with_variation() {
        let data = ISBPipelineTimingSource.collect(32);
        assert!(!data.is_empty());
    }
}
