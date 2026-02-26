//! Mach absolute time entropy source (macOS only).
//!
//! Reads the ARM system counter at sub-nanosecond resolution with variable
//! micro-workloads between samples.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Reads the ARM system counter (`mach_absolute_time`) at sub-nanosecond
/// resolution with variable micro-workloads between samples. Returns raw
/// LSBs of timing deltas — no conditioning applied.
pub struct MachTimingSource;

static MACH_TIMING_INFO: SourceInfo = SourceInfo {
    name: "mach_timing",
    description: "mach_absolute_time() with micro-workload jitter (raw LSBs)",
    physics: "Reads the ARM system counter (mach_absolute_time) at sub-nanosecond \
              resolution with variable micro-workloads between samples. The timing \
              jitter comes from CPU pipeline state: instruction reordering, branch \
              prediction, cache state, interrupt coalescing, and power-state \
              transitions.",
    category: SourceCategory::Timing,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 0.3,
    composite: false,
    is_fast: true,
};

impl EntropySource for MachTimingSource {
    fn info(&self) -> &SourceInfo {
        &MACH_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples + 64;
        let mut timings = Vec::with_capacity(raw_count);

        // LCG for randomizing workload size (seeded from clock).
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            let t0 = mach_time();

            // Variable micro-workload: randomized iteration count (1-8)
            // via LCG so the branch predictor can't settle.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let iterations = ((lcg >> 32) & 7) + 1;
            let mut sink: u64 = t0;
            for _ in 0..iterations {
                sink = sink.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(sink);

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Run with: cargo test -- --ignored
    fn mach_timing_collects_bytes() {
        let src = MachTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn source_info_name() {
        assert_eq!(MachTimingSource.name(), "mach_timing");
    }

    #[test]
    fn source_info_category() {
        assert_eq!(MachTimingSource.info().category, SourceCategory::Timing);
        assert!(!MachTimingSource.info().composite);
    }
}
