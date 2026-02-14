//! TLB shootdown timing — entropy from mprotect-induced IPI broadcasts.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

use super::extract_timing_entropy_variance;

/// Configuration for TLB shootdown entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::TLBShootdownConfig;
/// let config = TLBShootdownConfig {
///     page_count_range: (16, 64),   // fewer pages = fewer IPIs
///     region_pages: 128,            // smaller region
///     measure_variance: true,       // delta-of-deltas (recommended)
/// };
/// ```
#[derive(Debug, Clone)]
pub struct TLBShootdownConfig {
    /// Range of pages to invalidate per measurement `(min, max)`.
    ///
    /// Varying the page count changes the number of Inter-Processor Interrupts
    /// (IPIs) sent per `mprotect()` call. More pages = more IPIs = longer and
    /// more variable latency.
    ///
    /// Both values are clamped to `[1, region_pages]`.
    ///
    /// **Range:** min 1, max = `region_pages`. **Default:** `(8, 128)`
    pub page_count_range: (usize, usize),

    /// Total memory region size in pages.
    ///
    /// Larger regions use different physical pages each measurement, preventing
    /// TLB prefetch patterns. The region is allocated via `mmap` and touched
    /// on every page to establish TLB entries before measurement begins.
    ///
    /// **Range:** 8+. **Default:** `256` (1 MB with 4KB pages)
    pub region_pages: usize,

    /// Use delta-of-deltas (variance) extraction (`true`) or standard
    /// absolute timing extraction (`false`).
    ///
    /// Variance mode computes second-order deltas between consecutive
    /// shootdowns, removing systematic bias and amplifying the nondeterministic
    /// component. Produces higher min-entropy at the cost of ~2x raw samples.
    ///
    /// **Default:** `true`
    pub measure_variance: bool,
}

impl Default for TLBShootdownConfig {
    fn default() -> Self {
        Self {
            page_count_range: (8, 128),
            region_pages: 256,
            measure_variance: true,
        }
    }
}

/// Harvests timing jitter from TLB invalidation broadcasts via `mprotect()`.
///
/// # What it measures
/// Nanosecond timing of `mprotect()` permission toggles (read-write → read-only
/// → read-write) on varying numbers of pages within a pre-allocated memory region.
///
/// # Why it's entropic
/// When `mprotect()` changes page protection on a multi-core system, the kernel
/// must invalidate stale TLB entries on ALL cores:
/// - **Inter-Processor Interrupt (IPI)** — the kernel sends an IPI to every
///   core that might have cached TLB entries for the affected pages
/// - **TLB flush latency** — each receiving core must drain its pipeline,
///   flush matching TLB entries, and acknowledge
/// - **Cross-cluster latency** — Apple Silicon has separate P-core and E-core
///   clusters with different interconnect latencies
/// - **Concurrent IPI traffic** — other processes' `mprotect()`/`munmap()` calls
///   create IPI storms that interfere with our measurements
///
/// # What makes it unique
/// TLB shootdowns are a microarchitectural side-channel that has been studied
/// for attacks but never harvested as an entropy source. The IPI broadcast
/// mechanism creates system-wide nondeterminism that depends on the state of
/// EVERY core simultaneously.
///
/// # Configuration
/// See [`TLBShootdownConfig`] for tunable parameters. Key options:
/// - `measure_variance`: delta-of-deltas extraction (recommended: `true`)
/// - `page_count_range`: controls IPI storm intensity
/// - `region_pages`: controls physical page diversity
#[derive(Default)]
pub struct TLBShootdownSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
    pub config: TLBShootdownConfig,
}

static TLB_SHOOTDOWN_INFO: SourceInfo = SourceInfo {
    name: "tlb_shootdown",
    description: "TLB invalidation broadcast timing via variable-count mprotect IPI storms",
    physics: "Toggles page protection via mprotect() on varying page counts to trigger TLB \
              shootdown broadcasts. Each mprotect() sends IPIs to ALL cores to flush stale \
              TLB entries. Varying page counts creates different IPI patterns. Different \
              memory regions each time prevent TLB prefetch. Variance between consecutive \
              shootdowns captures relative timing with higher min-entropy. IPI latency depends \
              on: what each core is executing, P-core vs E-core cluster latency, core power \
              states, and concurrent IPI traffic.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

impl EntropySource for TLBShootdownSource {
    fn info(&self) -> &SourceInfo {
        &TLB_SHOOTDOWN_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // SAFETY: sysconf(_SC_PAGESIZE) is always safe and returns the page size.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let region_pages = self.config.region_pages.max(8);
        let region_size = page_size * region_pages;
        let (min_pages, max_pages) = self.config.page_count_range;
        let min_pages = min_pages.max(1).min(region_pages);
        let max_pages = max_pages.max(min_pages).min(region_pages);

        // SAFETY: mmap with MAP_ANONYMOUS|MAP_PRIVATE creates a private anonymous
        // mapping. We check for MAP_FAILED before using the returned address.
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                region_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };

        if addr == libc::MAP_FAILED {
            return Vec::new();
        }

        // Touch every page to establish TLB entries on this core.
        for p in 0..region_pages {
            // SAFETY: addr is valid mmap'd region, p * page_size < region_size.
            unsafe {
                std::ptr::write_volatile((addr as *mut u8).add(p * page_size), 0xAA);
            }
        }

        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            // Vary number of pages to invalidate.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let num_pages = if min_pages == max_pages {
                min_pages
            } else {
                min_pages + ((lcg >> 32) as usize % (max_pages - min_pages + 1))
            };
            let prot_size = num_pages * page_size;

            // Vary the region offset to use different memory each time.
            let max_offset_pages = region_pages.saturating_sub(num_pages);
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let offset_pages = if max_offset_pages > 0 {
                (lcg >> 48) as usize % max_offset_pages
            } else {
                0
            };
            let offset = offset_pages * page_size;

            let t0 = mach_time();

            // SAFETY: addr+offset is within the mmap'd region, prot_size fits.
            unsafe {
                let target = (addr as *mut u8).add(offset) as *mut libc::c_void;
                libc::mprotect(target, prot_size, libc::PROT_READ);
                libc::mprotect(target, prot_size, libc::PROT_READ | libc::PROT_WRITE);
            }

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        // SAFETY: addr was returned by mmap (checked != MAP_FAILED) with size region_size.
        unsafe {
            libc::munmap(addr, region_size);
        }

        if self.config.measure_variance {
            extract_timing_entropy_variance(&timings, n_samples)
        } else {
            extract_timing_entropy(&timings, n_samples)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = TLBShootdownSource::default();
        assert_eq!(src.name(), "tlb_shootdown");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = TLBShootdownConfig::default();
        assert_eq!(config.page_count_range, (8, 128));
        assert_eq!(config.region_pages, 256);
        assert!(config.measure_variance);
    }

    #[test]
    fn custom_config() {
        let src = TLBShootdownSource {
            config: TLBShootdownConfig {
                page_count_range: (4, 64),
                region_pages: 128,
                measure_variance: false,
            },
        };
        assert_eq!(src.config.page_count_range, (4, 64));
    }

    #[test]
    #[ignore] // Uses mmap/mprotect
    fn collects_bytes() {
        let src = TLBShootdownSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    #[ignore] // Uses mmap/mprotect
    fn absolute_mode() {
        let src = TLBShootdownSource {
            config: TLBShootdownConfig {
                measure_variance: false,
                ..TLBShootdownConfig::default()
            },
        };
        if src.is_available() {
            assert!(!src.collect(64).is_empty());
        }
    }
}
