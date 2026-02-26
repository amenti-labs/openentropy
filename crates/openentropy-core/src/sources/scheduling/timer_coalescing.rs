//! OS timer coalescing wakeup jitter — entropy from the system-wide timer queue.
//!
//! Modern operating systems batch timer wakeups to reduce power consumption.
//! When a thread calls `nanosleep(1ns)`, it does not wake up after 1 nanosecond.
//! It wakes up when the *next coalesced timer fires*, which depends on the
//! pending timer queue across **all processes on the system**.
//!
//! ## Physics
//!
//! macOS uses "timer coalescing" (introduced in 10.9) to align timer wakeups
//! within configurable windows. The actual wakeup time after a 1 ns sleep is
//! determined by:
//!
//! 1. Which other processes have pending timers (every app, daemon, and kernel
//!    subsystem contributes to the shared timer wheel)
//! 2. The current coalescence window size (varies with power state and system
//!    activity level)
//! 3. The phase of the hardware timer interrupt relative to our wakeup request
//! 4. Scheduler decisions about which runqueue to place the thread on after wakeup
//!
//! Empirically this source produces a bimodal distribution (~3 µs and ~13 µs)
//! with CV > 70%. The *position within each cluster* (the intra-cluster jitter)
//! encodes the real physical entropy: the phase relationship between our wakeup
//! request and the hardware interrupt firing cycle.
//!
//! ## Platform notes
//!
//! Available on all Unix-like systems with `nanosleep`. On Linux, behavior
//! depends on `CONFIG_HZ` and the high-resolution timer subsystem. On macOS,
//! the bimodal distribution reflects the 2-level coalescing structure. The
//! actual distribution shape is irrelevant — we extract LSB entropy from the
//! raw tick counts, which captures the sub-cluster jitter regardless of
//! coalescing policy.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static TIMER_COALESCING_INFO: SourceInfo = SourceInfo {
    name: "timer_coalescing",
    description: "OS timer coalescing wakeup jitter from system-wide timer queue state",
    physics: "Calls nanosleep(1ns) and measures the actual wakeup latency. The OS \
              batches timer wakeups across all processes; actual wakeup time depends on \
              pending timers from every process, daemon, and kernel subsystem on the \
              machine. Produces bimodal distribution (~3\u{00b5}s / ~13\u{00b5}s clusters on macOS) \
              with CV >70%. Intra-cluster jitter encodes the phase of the hardware timer \
              interrupt relative to our wakeup request — a system-wide aggregate noise \
              source with no per-process equivalent.",
    category: SourceCategory::Scheduling,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
    is_fast: false,
};

/// Entropy source that harvests OS timer coalescing wakeup jitter.
///
/// Each sample calls `nanosleep(1ns)` and records the actual elapsed time
/// (hardware ticks). The jitter comes from the shared timer queue across all
/// running processes on the system.
pub struct TimerCoalescingSource;

impl EntropySource for TimerCoalescingSource {
    fn info(&self) -> &SourceInfo {
        &TIMER_COALESCING_INFO
    }

    fn is_available(&self) -> bool {
        // nanosleep is available on all Unix targets.
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(unix)]
        {
            collect_unix(n_samples)
        }
        #[cfg(not(unix))]
        {
            let _ = n_samples;
            Vec::new()
        }
    }
}

#[cfg(unix)]
fn collect_unix(n_samples: usize) -> Vec<u8> {
    use std::time::Instant;

    // 12× oversampling: each wakeup contributes ~2-3 bits but has structural bias.
    let raw_count = n_samples * 12 + 64;
    let mut timings = Vec::with_capacity(raw_count);

    // Warm-up: let the OS scheduler settle our thread into its normal wakeup pattern.
    let warmup_req = libc_timespec(0, 1);
    for _ in 0..32 {
        let mut rem = libc_timespec(0, 0);
        // SAFETY: pointers are to stack-allocated timespec structs.
        unsafe { libc::nanosleep(&warmup_req, &mut rem) };
    }

    for _ in 0..raw_count {
        let req = libc_timespec(0, 1); // 1 ns request
        let mut rem = libc_timespec(0, 0);

        let t0 = Instant::now();
        // SAFETY: req and rem are valid stack-allocated timespec structs.
        unsafe { libc::nanosleep(&req, &mut rem) };
        let elapsed_ns = t0.elapsed().as_nanos() as u64;

        // Sanity filter: reject absurd values (>500ms would indicate a suspend/resume).
        if elapsed_ns < 500_000_000 {
            timings.push(elapsed_ns);
        }
    }

    extract_timing_entropy(&timings, n_samples)
}

#[cfg(unix)]
#[inline]
fn libc_timespec(secs: i64, nsecs: i64) -> libc::timespec {
    libc::timespec {
        tv_sec: secs as libc::time_t,
        tv_nsec: nsecs as libc::c_long,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = TimerCoalescingSource;
        assert_eq!(src.info().name, "timer_coalescing");
        assert!(matches!(src.info().category, SourceCategory::Scheduling));
        assert_eq!(src.info().platform, Platform::Any);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(unix)]
    fn is_available_on_unix() {
        assert!(TimerCoalescingSource.is_available());
    }

    #[test]
    #[ignore] // Hardware timing — can be slow in constrained environments
    fn collects_bytes_with_variation() {
        let src = TimerCoalescingSource;
        if !src.is_available() {
            return;
        }
        let data = src.collect(32);
        assert!(!data.is_empty(), "expected non-empty output");
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(
            unique.len() > 2,
            "expected byte variation from coalescing jitter"
        );
    }
}
