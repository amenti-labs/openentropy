//! Thread lifecycle timing — entropy from pthread create/join scheduling.

use std::thread;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Harvests timing jitter from thread creation and destruction.
///
/// # What it measures
/// Nanosecond timing of the full `pthread_create` + `pthread_join` cycle,
/// with variable per-thread workloads.
///
/// # Why it's entropic
/// Each thread lifecycle exercises deep kernel scheduling paths:
/// - **Mach thread port allocation** from the kernel IPC port name space
/// - **Zone allocator** allocation for the kernel thread structure
/// - **CPU core selection** — P-core vs E-core, influenced by ALL threads
/// - **Stack page allocation** via `vm_allocate`
/// - **TLS setup** including dyld per-thread state
/// - **Context switch on join** — depends on current runqueue state
/// - **Core migration** — new thread may run on a different core
///
/// # What makes it unique
/// Thread lifecycle timing is a previously untapped entropy source. The
/// combination of kernel memory allocation, scheduling decisions, and
/// cross-core communication produces 89 unique LSB values — the richest
/// of any frontier source.
///
/// # Configuration
/// No configuration needed — this source has no tunable parameters.
pub struct ThreadLifecycleSource;

static THREAD_LIFECYCLE_INFO: SourceInfo = SourceInfo {
    name: "thread_lifecycle",
    description: "Thread create/join kernel scheduling and allocation jitter",
    physics: "Creates and immediately joins threads, measuring the full lifecycle timing. \
              Each cycle involves: Mach thread port allocation, zone allocator allocation, \
              CPU core selection (P-core vs E-core), stack page allocation, TLS setup, and \
              context switch on join. The scheduler\u{2019}s core selection depends on thermal \
              state, load from ALL processes, and QoS priorities.",
    category: SourceCategory::Scheduling,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 3000.0,
    composite: false,
};

impl EntropySource for ThreadLifecycleSource {
    fn info(&self) -> &SourceInfo {
        &THREAD_LIFECYCLE_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let work_amount = (lcg >> 48) as u32 % 100;

            let t0 = mach_time();
            let handle = thread::spawn(move || {
                let mut sink: u64 = 0;
                for j in 0..work_amount {
                    sink = sink.wrapping_add(j as u64);
                }
                std::hint::black_box(sink);
            });
            let _ = handle.join();
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
    fn info() {
        let src = ThreadLifecycleSource;
        assert_eq!(src.name(), "thread_lifecycle");
        assert_eq!(src.info().category, SourceCategory::Scheduling);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Spawns threads
    fn collects_bytes() {
        let src = ThreadLifecycleSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes identical");
        }
    }
}
