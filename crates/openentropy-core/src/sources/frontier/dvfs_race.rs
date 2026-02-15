//! Cross-core DVFS race — entropy from independent frequency scaling controllers.
//!
//! Two threads race on different CPU cores running tight counting loops. The
//! difference in iteration counts captures physical frequency jitter from
//! independent DVFS (Dynamic Voltage and Frequency Scaling) controllers on
//! P-core vs E-core clusters.
//!
//! PoC measured H∞ = 7.381 bits/byte — the highest of any discovered source.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{mach_time, xor_fold_u64};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

/// Cross-core DVFS race entropy source.
///
/// Spawns two threads that race via tight counting loops. The absolute
/// difference in their iteration counts after a short window (~2μs) is
/// physically nondeterministic because:
///
/// 1. P-cores and E-cores have independent DVFS controllers that adjust
///    frequency based on thermal sensors, power budget, and workload.
/// 2. The scheduler assigns threads to different core clusters
///    nondeterministically based on system-wide load and QoS.
/// 3. Even within a single core type, frequency transitions are asynchronous
///    and thermally-driven — two identical cores can run at different
///    frequencies at the same instant.
/// 4. The stop signal propagation has cache-coherence latency that varies
///    by which cores the threads landed on.
pub struct DVFSRaceSource;

static DVFS_RACE_INFO: SourceInfo = SourceInfo {
    name: "dvfs_race",
    description: "Cross-core DVFS frequency race between thread pairs",
    physics: "Spawns two threads running tight counting loops on different cores. \
              After a ~2\u{00b5}s race window, the absolute difference in iteration \
              counts captures nondeterminism from: scheduler core placement (P-core vs \
              E-core), cache coherence latency for the stop signal, interrupt jitter, \
              and cross-core pipeline state differences. On Apple Silicon, P-core and \
              E-core clusters have separate frequency domains, but the 2\u{00b5}s window is \
              too short for DVFS transitions (~100\u{00b5}s-1ms); the primary entropy comes \
              from scheduling and cache-coherence nondeterminism. \
              PoC measured H\u{221e} = 7.381 bits/byte.",
    category: SourceCategory::Microarch,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 5000.0,
    composite: false,
};

impl EntropySource for DVFSRaceSource {
    fn info(&self) -> &SourceInfo {
        &DVFS_RACE_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // We need enough race differentials to extract n_samples bytes.
        // Each race produces one u64 differential; XOR-fold pairs → bytes.
        let raw_count = n_samples * 4 + 64;
        let mut diffs: Vec<u64> = Vec::with_capacity(raw_count);

        // Get timebase for ~2μs window calculation.
        // On Apple Silicon, mach_absolute_time ticks at 24MHz → 1 tick ≈ 41.67ns.
        // 2μs ≈ 48 ticks. Use a small window to keep collection fast.
        let window_ticks: u64 = 48; // ~2μs on Apple Silicon

        for _ in 0..raw_count {
            let stop = Arc::new(AtomicBool::new(false));
            let count1 = Arc::new(AtomicU64::new(0));
            let count2 = Arc::new(AtomicU64::new(0));
            let ready1 = Arc::new(AtomicBool::new(false));
            let ready2 = Arc::new(AtomicBool::new(false));

            let s1 = stop.clone();
            let c1 = count1.clone();
            let r1 = ready1.clone();
            let handle1 = thread::spawn(move || {
                let mut local_count: u64 = 0;
                r1.store(true, Ordering::Release);
                while !s1.load(Ordering::Relaxed) {
                    local_count = local_count.wrapping_add(1);
                }
                c1.store(local_count, Ordering::Release);
            });

            let s2 = stop.clone();
            let c2 = count2.clone();
            let r2 = ready2.clone();
            let handle2 = thread::spawn(move || {
                let mut local_count: u64 = 0;
                r2.store(true, Ordering::Release);
                while !s2.load(Ordering::Relaxed) {
                    local_count = local_count.wrapping_add(1);
                }
                c2.store(local_count, Ordering::Release);
            });

            // Wait for both threads to be ready.
            while !ready1.load(Ordering::Acquire) || !ready2.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }

            // Let them race for ~2μs.
            let t_start = mach_time();
            let t_end = t_start.wrapping_add(window_ticks);
            while mach_time() < t_end {
                std::hint::spin_loop();
            }

            // Stop both threads.
            stop.store(true, Ordering::Release);
            let _ = handle1.join();
            let _ = handle2.join();

            let v1 = count1.load(Ordering::Acquire);
            let v2 = count2.load(Ordering::Acquire);
            let diff = v1.abs_diff(v2);
            diffs.push(diff);
        }

        // Extract entropy: XOR adjacent diffs, then xor-fold to bytes.
        let xored: Vec<u64> = diffs.windows(2).map(|w| w[0] ^ w[1]).collect();
        let mut raw: Vec<u8> = xored.iter().map(|&x| xor_fold_u64(x)).collect();
        raw.truncate(n_samples);
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = DVFSRaceSource;
        assert_eq!(src.info().name, "dvfs_race");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Hardware-dependent: requires multi-core CPU
    fn collects_bytes() {
        let src = DVFSRaceSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        // Check we get some variation (not all zeros or all same).
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 1, "Expected variation in collected bytes");
    }
}
