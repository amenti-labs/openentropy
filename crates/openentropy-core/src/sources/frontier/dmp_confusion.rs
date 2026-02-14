//! Pointer-chase random walk timing — entropy from multi-hop random memory
//! access latency across Apple Silicon's cache hierarchy.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::mach_time;

use super::extract_timing_entropy_variance;

/// Configuration for DMP confusion entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::DMPConfusionConfig;
/// // Use defaults (recommended)
/// let config = DMPConfusionConfig::default();
///
/// // Or customize
/// let config = DMPConfusionConfig {
///     array_size_mb: 8,
///     hops_per_sample: 3,
///     von_neumann_debias: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct DMPConfusionConfig {
    /// Size of the pointer-filled confusion array in megabytes.
    ///
    /// Larger arrays span more cache levels (L1 → L2 → SLC → DRAM), creating
    /// more diverse DMP prediction contexts. Must be at least 1 MB.
    ///
    /// **Default:** `16` (16 MB, exceeds the SLC on most Apple Silicon)
    pub array_size_mb: usize,

    /// Number of pointer dereference hops per timing measurement.
    ///
    /// Each hop follows a "pointer" value in the array. More hops give the DMP
    /// more opportunities to mispredict. 3 hops with a direction reversal
    /// produced the best min-entropy in testing.
    ///
    /// **Range:** 1-8. **Default:** `3`
    pub hops_per_sample: usize,

    /// Apply Von Neumann debiasing to the extracted bytes.
    ///
    /// The DMP confusion source has moderate bias in raw XOR-folded timings.
    /// Von Neumann debiasing pairs consecutive samples and discards equal
    /// pairs, improving min-entropy at ~50% data cost.
    ///
    /// **Default:** `false` (the variance extraction already handles bias well)
    pub von_neumann_debias: bool,
}

impl Default for DMPConfusionConfig {
    fn default() -> Self {
        Self {
            array_size_mb: 16,
            hops_per_sample: 3,
            von_neumann_debias: false,
        }
    }
}

/// Harvests timing jitter from multi-hop random pointer chases across a large array.
///
/// # What it measures
/// Nanosecond timing of pointer-chase sequences through a 16MB array filled with
/// pseudo-random intra-array "pointer" values.
///
/// # Why it's entropic
/// Multi-hop pointer chases through a large random array produce cache misses
/// at unpredictable cache levels (L1/L2/SLC/DRAM). The timing depends on:
/// - Which cache level satisfies each load (determined by recent access history
///   of ALL processes, not just ours)
/// - DRAM bank conflicts and row buffer state
/// - Memory controller queue depth and reordering
/// - Whether the DMP (Data Memory-dependent Prefetcher) activates on our
///   pointer-like values — this adds some additional nondeterminism but
///   validation showed the primary entropy driver is cache/DRAM timing, not
///   DMP prediction failures specifically
///
/// # Independence from cache_contention
/// Validation showed Pearson r = 0.17 with cache_contention — weak but nonzero
/// correlation. The sources share some cache-level entropy but differ in access
/// pattern (pointer-chase vs random stride) and working set size (16MB vs 4MB).
///
/// # Measured entropy (validated at 100K samples)
/// - XOR-fold: H∞ ≈ 2.0 bits/byte
/// - Stability across 10 trials: H∞ = 1.5–1.9, mean 1.7
/// - ~7μs per sample (fast)
///
/// # Configuration
/// See [`DMPConfusionConfig`] for tunable parameters.
#[derive(Default)]
pub struct DMPConfusionSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
    pub config: DMPConfusionConfig,
}

static DMP_CONFUSION_INFO: SourceInfo = SourceInfo {
    name: "dmp_confusion",
    description: "Multi-hop random pointer-chase timing across cache hierarchy",
    physics: "Performs multi-hop pointer chases through a 16MB array filled with \
              pseudo-random intra-array pointer values. Each hop causes a cache miss \
              at an unpredictable level (L1/L2/SLC/DRAM). Timing depends on: cache \
              occupancy from all processes, DRAM bank conflicts and row buffer state, \
              memory controller queue depth, and DMP (Data Memory-dependent Prefetcher) \
              activation. Validated H∞ ≈ 1.7 at 10K samples. Pearson r = 0.17 with \
              cache_contention indicates weak but nonzero shared entropy domain.",
    category: SourceCategory::Frontier,
    platform_requirements: &["macos", "aarch64"],
    entropy_rate_estimate: 1700.0,
    composite: false,
};

impl EntropySource for DMPConfusionSource {
    fn info(&self) -> &SourceInfo {
        &DMP_CONFUSION_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(all(target_os = "macos", target_arch = "aarch64"))
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let array_size = self.config.array_size_mb.max(1) * 1024 * 1024;
        let hops = self.config.hops_per_sample.clamp(1, 8);

        let n_elements = array_size / std::mem::size_of::<u64>();

        // Allocate a large array via mmap.
        // SAFETY: mmap with MAP_ANONYMOUS|MAP_PRIVATE creates a private anonymous mapping.
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                array_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };

        if addr == libc::MAP_FAILED {
            return Vec::new();
        }

        let array = addr as *mut u64;
        let base = array as u64;

        // Fill with pseudo-random values that look like valid pointers into the array.
        // This is what activates the DMP — it sees pointer-like values and tries to
        // prefetch their targets.
        let mut lcg: u64 = mach_time() | 1;
        for i in 0..n_elements {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let offset = (lcg >> 16) as usize % n_elements;
            // SAFETY: i < n_elements, all within the mmap'd region.
            unsafe {
                *array.add(i) = base + (offset as u64) * 8;
            }
        }

        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        let mut sink: u64 = 0;

        for _ in 0..raw_count {
            // Pick a random starting position.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start_idx = (lcg >> 16) as usize % (n_elements.saturating_sub(256).max(1));

            // Memory barrier before timing.
            // SAFETY: inline assembly for DMB instruction, always safe.
            unsafe {
                std::arch::asm!("dmb sy", options(nostack, preserves_flags));
            }

            let t0 = mach_time();

            // Multi-hop pointer chase.
            // SAFETY: all indices are bounds-checked against n_elements.
            unsafe {
                let mut idx = start_idx;
                for _ in 0..hops {
                    let val = std::ptr::read_volatile(array.add(idx));
                    let next = ((val - base) / 8) as usize;
                    if next < n_elements {
                        idx = next;
                    }
                }
                sink = sink.wrapping_add(std::ptr::read_volatile(array.add(idx)));

                // Direction reversal — the DMP predicted forward, we go backward.
                // This is the key entropy mechanism: the DMP's prediction was wrong.
                if idx >= 64 {
                    sink = sink.wrapping_add(std::ptr::read_volatile(array.add(idx - 64)));
                }
            }

            // Memory barrier after.
            unsafe {
                std::arch::asm!("dmb sy", options(nostack, preserves_flags));
            }

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        // Prevent sink from being optimized away.
        std::hint::black_box(sink);

        // SAFETY: addr was returned by mmap (checked != MAP_FAILED) with size array_size.
        unsafe {
            libc::munmap(addr, array_size);
        }

        if self.config.von_neumann_debias {
            super::extract_timing_entropy_debiased(&timings, n_samples)
        } else {
            extract_timing_entropy_variance(&timings, n_samples)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = DMPConfusionSource::default();
        assert_eq!(src.name(), "dmp_confusion");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = DMPConfusionConfig::default();
        assert_eq!(config.array_size_mb, 16);
        assert_eq!(config.hops_per_sample, 3);
        assert!(!config.von_neumann_debias);
    }

    #[test]
    fn custom_config() {
        let src = DMPConfusionSource {
            config: DMPConfusionConfig {
                array_size_mb: 8,
                hops_per_sample: 2,
                von_neumann_debias: true,
            },
        };
        assert_eq!(src.config.array_size_mb, 8);
        assert_eq!(src.config.hops_per_sample, 2);
    }

    #[test]
    #[ignore] // Requires macOS aarch64 + 16MB allocation
    fn collects_bytes() {
        let src = DMPConfusionSource::default();
        if src.is_available() {
            let data = src.collect(128);
            assert!(!data.is_empty());
            assert!(data.len() <= 128);
        }
    }

    #[test]
    #[ignore] // Requires macOS aarch64
    fn debiased_mode() {
        let src = DMPConfusionSource {
            config: DMPConfusionConfig {
                von_neumann_debias: true,
                ..DMPConfusionConfig::default()
            },
        };
        if src.is_available() {
            let data = src.collect(64);
            // Debiased may produce fewer bytes
            assert!(data.len() <= 64);
        }
    }
}
