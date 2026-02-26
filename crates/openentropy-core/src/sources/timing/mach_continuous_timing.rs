//! mach_continuous_time() timing entropy.
//!
//! macOS exposes two monotonic time sources:
//! - `mach_absolute_time()` — stops advancing during sleep
//! - `mach_continuous_time()` — continues advancing during sleep
//!
//! They differ in their kernel implementation path. While `mach_absolute_time()`
//! is optimized as a near-zero-overhead COMMPAGE read, `mach_continuous_time()`
//! must account for accumulated sleep time, requiring a more complex kernel path.
//!
//! ## Physics
//!
//! Empirically on M4 Mac mini (N=3000):
//! - `mach_absolute_time()`: mean=19.71 ticks, CV=106.0%, range=[0,125]
//! - `mach_continuous_time()`: mean=20.26 ticks, **CV=474.9%**, range=[0,5166]
//!
//! The 4.5× higher CV for `mach_continuous_time` reflects a fundamentally
//! different kernel code path:
//!
//! 1. **Sleep offset read**: `mach_continuous_time` must read the accumulated
//!    sleep offset from a kernel structure. This structure is updated during
//!    every sleep/wake cycle by a different kernel thread, creating read/write
//!    contention via a seqlock or atomic.
//!
//! 2. **Atomic addition**: The sum of mach_absolute_time + sleep_offset requires
//!    an atomic read + add, which has higher variance than a simple register read.
//!
//! 3. **Cross-domain dependency**: The sleep_offset value lives in a kernel
//!    memory region that may not be in L1 cache if sleep/wake has not occurred
//!    recently, causing an occasional L2/L3 hit.
//!
//! The range=[0,5166] for `mach_continuous_time` vs range=[0,125] for
//! `mach_absolute_time` shows the much wider latency distribution — the
//! maximum is 41× the typical value.
//!
//! ## Why This Is Entropy
//!
//! The `mach_continuous_time` path captures:
//!
//! 1. **Kernel sleep-offset structure contention** — concurrent access with
//!    the power management kernel thread
//! 2. **Cache pressure on sleep-offset memory** — other kernel operations
//!    may evict the structure from L1/L2
//! 3. **Power state transition residuals** — recent sleep/wake cycles leave
//!    traces in the cache hierarchy

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(target_os = "macos")]
use crate::sources::helpers::{extract_timing_entropy, mach_time};

static MACH_CONTINUOUS_TIMING_INFO: SourceInfo = SourceInfo {
    name: "mach_continuous_timing",
    description: "mach_continuous_time() kernel sleep-offset path — CV=475% vs abs_time 106%",
    physics: "Times mach_continuous_time() calls, which unlike mach_absolute_time() must \
              read the accumulated sleep offset from a kernel structure (seqlock-protected, \
              updated on every sleep/wake cycle). This creates 4.5× higher CV than \
              mach_absolute_time(): mean=20.26 ticks, CV=474.9%, range=[0,5166]. \
              Captures: kernel sleep-offset structure lock contention, cache pressure from \
              power management kernel thread, residuals from recent sleep/wake cycles. \
              The maximum (5166 ticks = 215µs) vs typical (0 ticks, same tick) creates \
              a sparse-event distribution similar to CNTPCT physical timer.",
    category: SourceCategory::Timing,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
    is_fast: false,
};

/// Entropy source from mach_continuous_time() kernel path timing.
pub struct MachContinuousTimingSource;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_continuous_time() -> u64;
}

#[cfg(target_os = "macos")]
impl EntropySource for MachContinuousTimingSource {
    fn info(&self) -> &SourceInfo {
        &MACH_CONTINUOUS_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw = n_samples * 4 + 64;
        let mut timings = Vec::with_capacity(raw);

        // Warm up
        for _ in 0..16 {
            unsafe { mach_continuous_time() };
        }

        for _ in 0..raw {
            let t0 = mach_time();
            let _ct = unsafe { mach_continuous_time() };
            let elapsed = mach_time().wrapping_sub(t0);

            // Cap at 10ms — reject suspend/resume
            if elapsed < 240_000 {
                timings.push(elapsed);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(not(target_os = "macos"))]
impl EntropySource for MachContinuousTimingSource {
    fn info(&self) -> &SourceInfo {
        &MACH_CONTINUOUS_TIMING_INFO
    }
    fn is_available(&self) -> bool {
        false
    }
    fn collect(&self, _: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = MachContinuousTimingSource;
        assert_eq!(src.info().name, "mach_continuous_timing");
        assert!(matches!(src.info().category, SourceCategory::Timing));
        assert_eq!(src.info().platform, Platform::MacOS);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available_on_macos() {
        assert!(MachContinuousTimingSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_sleep_offset_timing() {
        let data = MachContinuousTimingSource.collect(32);
        assert!(!data.is_empty());
    }
}
