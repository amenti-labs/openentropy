//! Atomic CAS contention — entropy from multi-thread compare-and-swap arbitration.
//!
//! Multiple threads race on atomic CAS operations targeting shared cache lines.
//! The hardware coherence engine's arbitration order is physically nondeterministic.
//!

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{mach_time, xor_fold_u64};

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

const NUM_THREADS: usize = 4;
const NUM_TARGETS: usize = 64;
// 128-byte spacing to put each target on its own Apple Silicon cache line.
const TARGET_SPACING: usize = 16; // 16 * 8 bytes = 128 bytes

/// Configuration for CAS contention entropy collection.
#[derive(Debug, Clone)]
pub struct CASContentionConfig {
    /// Number of threads to spawn for contention.
    ///
    /// More threads increase contention and entropy quality at the cost of CPU.
    ///
    /// **Default:** `4`
    pub num_threads: usize,
}

impl Default for CASContentionConfig {
    fn default() -> Self {
        Self {
            num_threads: NUM_THREADS,
        }
    }
}

/// CAS contention entropy source.
///
/// Spawns multiple threads that perform atomic compare-and-swap on shared
/// targets spread across cache lines. The arbitration timing between threads
/// competing for the same cache line is physically nondeterministic because:
///
/// 1. The cache coherence protocol (MOESI on Apple Silicon) arbitrates
///    concurrent exclusive-access requests nondeterministically.
/// 2. The interconnect fabric latency varies with thermal state and traffic.
/// 3. Each thread's CAS targets are chosen pseudo-randomly, creating
///    unpredictable contention patterns.
/// 4. XOR-combining timings from all threads amplifies the arbitration entropy.
pub struct CASContentionSource {
    config: CASContentionConfig,
}

impl CASContentionSource {
    pub fn new(config: CASContentionConfig) -> Self {
        Self { config }
    }
}

impl Default for CASContentionSource {
    fn default() -> Self {
        Self::new(CASContentionConfig::default())
    }
}

static CAS_CONTENTION_INFO: SourceInfo = SourceInfo {
    name: "cas_contention",
    description: "Multi-thread atomic CAS arbitration contention jitter",
    physics: "Spawns 4 threads performing atomic compare-and-swap operations on \
              shared targets spread across 128-byte-aligned cache lines. The \
              hardware coherence engine (MOESI protocol on Apple Silicon) must \
              arbitrate concurrent exclusive-access requests. This arbitration is \
              physically nondeterministic due to interconnect fabric latency \
              variations, thermal state, and traffic from other cores/devices. \
              XOR-combining timing measurements from all threads amplifies the \
              arbitration entropy.",
    category: SourceCategory::Microarch,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

struct ThreadResult {
    timings: Vec<u64>,
}

impl EntropySource for CASContentionSource {
    fn info(&self) -> &SourceInfo {
        &CAS_CONTENTION_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let samples_per_thread = n_samples * 4 + 64;
        let nthreads = self.config.num_threads;

        // Allocate contention targets (each on its own cache line).
        let total_atomics = NUM_TARGETS * TARGET_SPACING;
        let targets: Arc<Vec<AtomicU64>> =
            Arc::new((0..total_atomics).map(|_| AtomicU64::new(0)).collect());

        let go = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::with_capacity(nthreads);

        for thread_id in 0..nthreads {
            let targets = targets.clone();
            let go = go.clone();
            let stop = stop.clone();
            let count = samples_per_thread;

            handles.push(thread::spawn(move || {
                let mut timings = Vec::with_capacity(count);
                let mut lcg: u64 = mach_time() ^ ((thread_id as u64) << 32) | 1;

                // Wait for go signal.
                while go.load(Ordering::Acquire) == 0 {
                    std::hint::spin_loop();
                }

                for _ in 0..count {
                    if stop.load(Ordering::Relaxed) != 0 {
                        break;
                    }

                    lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let idx = ((lcg >> 32) as usize % NUM_TARGETS) * TARGET_SPACING;

                    let t0 = mach_time();

                    let expected = targets[idx].load(Ordering::Relaxed);
                    let _ = targets[idx].compare_exchange_weak(
                        expected,
                        expected.wrapping_add(1),
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    );

                    let t1 = mach_time();
                    timings.push(t1.wrapping_sub(t0));
                }

                ThreadResult { timings }
            }));
        }

        // Start all threads.
        go.store(1, Ordering::Release);

        // Collect results.
        let results: Vec<ThreadResult> = handles
            .into_iter()
            .map(|h| h.join().unwrap_or(ThreadResult { timings: vec![] }))
            .collect();

        // Signal stop (in case any thread is still running).
        stop.store(1, Ordering::Release);

        // XOR-combine timings from all threads for maximum entropy.
        let min_len = results.iter().map(|r| r.timings.len()).min().unwrap_or(0);
        if min_len < 4 {
            return Vec::new();
        }

        let mut combined: Vec<u64> = Vec::with_capacity(min_len);
        for i in 0..min_len {
            let mut val = 0u64;
            for result in &results {
                val ^= result.timings[i];
            }
            combined.push(val);
        }

        // Extract entropy: deltas → XOR adjacent → xor-fold.
        let deltas: Vec<u64> = combined
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();
        let xored: Vec<u64> = deltas.windows(2).map(|w| w[0] ^ w[1]).collect();
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
        let src = CASContentionSource::default();
        assert_eq!(src.info().name, "cas_contention");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert!(!src.info().composite);
    }

    #[test]
    fn custom_config() {
        let config = CASContentionConfig { num_threads: 2 };
        let src = CASContentionSource::new(config);
        assert_eq!(src.config.num_threads, 2);
    }

    #[test]
    #[ignore] // Hardware-dependent: requires multi-core CPU
    fn collects_bytes() {
        let src = CASContentionSource::default();
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 1, "Expected variation in collected bytes");
    }
}
