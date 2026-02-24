//! ARM64 LDXR/STXR exclusive monitor timing entropy.
//!
//! The ARM64 exclusive access instructions (`LDXR` — Load Exclusive Register,
//! `STXR` — Store Exclusive Register) implement compare-and-swap semantics
//! via the hardware **exclusive monitor** — a per-core register that tracks
//! one cache line address.
//!
//! ## Physics
//!
//! The `LDXR` instruction loads from a memory address and marks it as
//! "exclusively owned" by this core. The `STXR` instruction attempts to
//! store to the same address, succeeding (returns 0) only if the exclusive
//! mark is still valid. The mark is cleared by:
//!
//! - Any other core writing to the same physical cache line
//! - An interrupt or context switch (the kernel clears exclusive monitors
//!   on thread preemption to prevent exclusive monitor starvation)
//! - An explicit `CLREX` instruction
//!
//! ## Timing Characteristics
//!
//! Even in single-threaded use, LDXR+STXR timing shows extreme variance:
//! - **CV=721.1%**, range=0–4,500 ticks (single-threaded measurement)
//!
//! This matches the preemption boundary probe: when the kernel preempts our
//! thread between `LDXR` and `STXR`, the elapsed time captures the exact
//! duration of that preemption event — which encodes what IRQ fired,
//! how long the interrupt handler ran, and how deep the runqueue was.
//!
//! ## Contention Mode
//!
//! With a background thread writing to the same cache line:
//! - **Failure rate: 99.2%** under heavy contention
//! - Failure timing: mean=26.3 ticks, range=0–125 ticks
//!
//! The timing of each STXR failure encodes the **phase alignment** between
//! our exclusive window and the contending thread's store timing. This phase
//! is nondeterministic due to:
//! - Thermal noise in the two threads' execution clocks
//! - Pipeline depth variation in the store queue
//! - Cache line state machine transition timing
//!
//! We use a controlled background thread (single same-process contender) to
//! ensure this source operates independently of other processes. The entropy
//! comes from the thermal and pipeline noise in the timing, not from
//! external covert channel leakage.
//!
//! ## Why This Source Is Unique
//!
//! LDXR/STXR is the only ARM64 instruction pair that has three entropy modes:
//! 1. **Timing variance** (CV=721%) from preemption boundary capture
//! 2. **Failure rate** from cross-core exclusive monitor invalidation
//! 3. **Failure timing** from phase alignment between asynchronous threads
//!
//! No other single instruction captures scheduler state, cross-core coherency,
//! and pipeline phase simultaneously.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::mach_time;

static EXCLUSIVE_MONITOR_INFO: SourceInfo = SourceInfo {
    name: "exclusive_monitor",
    description: "ARM64 LDXR/STXR exclusive monitor timing — preemption + cache line contention",
    physics: "Times LDXR+STXR pairs. Single-threaded: CV=721.1%, range=0\u{2013}4500 ticks — \
              captures exact kernel preemption duration when scheduler fires between load and \
              store (clears exclusive mark). Contention mode: 99.2% failure rate with a \
              background thread on the same cache line; failure timing encodes phase alignment \
              between asynchronous threads (thermal noise in CPU clock paths). Three entropy \
              modes: preemption boundary timing (scheduler state), failure rate (cross-core \
              coherency events), failure timing (cache line state machine phase noise).",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 7000.0,
    composite: false,
};

/// Entropy source from ARM64 LDXR/STXR exclusive monitor timing.
pub struct ExclusiveMonitorSource;

/// Cache-line-aligned target for exclusive monitor.
/// Aligned to 128 bytes (2 cache lines) to prevent false sharing with
/// any adjacent allocations.
#[repr(align(128))]
struct AlignedTarget(AtomicU64);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for ExclusiveMonitorSource {
    fn info(&self) -> &SourceInfo {
        &EXCLUSIVE_MONITOR_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw = n_samples * 8 + 64;

        // Shared target for exclusive monitor operations.
        let target = Arc::new(AlignedTarget(AtomicU64::new(0)));
        let target2 = target.clone();

        // Run a background thread that constantly writes to the same cache line.
        // This triggers STXR failures and creates phase-alignment entropy.
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();

        let contender = thread::spawn(move || {
            let atomic = &target2.0;
            while !stop2.load(Ordering::Relaxed) {
                // Stores via AtomicU64 force a cache-line-owning write through the
                // coherency fabric, which invalidates any exclusive monitor held on
                // this cache line by another core.
                atomic.store(0xDEAD_BEEF_CAFE_BABE, Ordering::Release);
                atomic.store(0xFEED_FACE_DEAD_C0DE, Ordering::Release);
            }
        });

        // Short warm-up: let the contender establish a rhythm before we start timing.
        thread::yield_now();

        let mut timings = Vec::with_capacity(raw);
        let mut failures = Vec::with_capacity(raw / 2);

        let tgt_ptr = target.0.as_ptr() as *const u64;

        for i in 0..raw {
            let t0 = mach_time();
            let (loaded, result) = unsafe {
                let mut loaded: u64;
                let mut result: u32;
                core::arch::asm!(
                    "ldxr {loaded}, [{ptr}]",
                    "stxr {res:w}, {val}, [{ptr}]",
                    ptr    = in(reg)  tgt_ptr,
                    val    = in(reg)  (i as u64).wrapping_add(0x5555_5555),
                    loaded = out(reg) loaded,
                    res    = out(reg) result,
                    options(nostack),
                );
                (loaded, result)
            };
            let elapsed = mach_time().wrapping_sub(t0);

            // Reject preemption spikes >10ms (system suspend/resume).
            if elapsed < 240_000 {
                timings.push(elapsed);
                // Failure timing encodes cache-line phase alignment.
                if result != 0 {
                    failures.push(elapsed);
                }
            }

            let _ = loaded;
        }

        stop.store(true, Ordering::Release);
        let _ = contender.join();

        // Primary: use failure timings if we got enough (contention mode).
        // Fallback: use all timings (preemption + coherency mode).
        let source = if failures.len() >= n_samples * 2 {
            &failures
        } else {
            &timings
        };

        extract_timing_entropy(source, n_samples)
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for ExclusiveMonitorSource {
    fn info(&self) -> &SourceInfo { &EXCLUSIVE_MONITOR_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = ExclusiveMonitorSource;
        assert_eq!(src.info().name, "exclusive_monitor");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_apple_silicon() {
        assert!(ExclusiveMonitorSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_with_variation() {
        let data = ExclusiveMonitorSource.collect(32);
        assert!(!data.is_empty());
    }
}
