//! Page fault timing entropy source via mmap/munmap cycles.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Triggers and times minor page faults via `mmap`/`munmap`. Page fault
/// resolution requires TLB lookup, hardware page table walk (up to 4 levels on
/// ARM64), physical page allocation from the kernel free list, and zero-fill
/// for security. The timing depends on physical memory fragmentation.
pub struct PageFaultTimingSource;

static PAGE_FAULT_TIMING_INFO: SourceInfo = SourceInfo {
    name: "page_fault_timing",
    description: "Minor page fault timing via mmap/munmap cycles",
    physics: "Triggers and times minor page faults via mmap/munmap. Page fault resolution \
              requires: TLB lookup, hardware page table walk (up to 4 levels on ARM64), \
              physical page allocation from the kernel free list, and zero-fill for \
              security. The timing depends on physical memory fragmentation.",
    category: SourceCategory::Timing,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
    is_fast: true,
};

impl EntropySource for PageFaultTimingSource {
    fn info(&self) -> &SourceInfo {
        &PAGE_FAULT_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // SAFETY: sysconf(_SC_PAGESIZE) is always safe and returns the page size.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let num_pages: usize = 8;
        let map_size = page_size * num_pages;

        // 4x oversampling; each cycle produces `num_pages` timings.
        let num_cycles = (n_samples * 4 / num_pages) + 4;

        let mut timings = Vec::with_capacity(num_cycles * num_pages);

        for _ in 0..num_cycles {
            // SAFETY: mmap with MAP_ANONYMOUS|MAP_PRIVATE creates a private anonymous
            // mapping. We check for MAP_FAILED before using the returned address.
            let addr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    map_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                    -1,
                    0,
                )
            };

            if addr == libc::MAP_FAILED {
                continue;
            }

            // Touch each page to trigger a minor fault and time it with
            // high-resolution mach_time instead of Instant.
            for p in 0..num_pages {
                // SAFETY: addr points to a valid mmap region of map_size bytes.
                // p * page_size < map_size since p < num_pages and map_size = num_pages * page_size.
                let page_ptr = unsafe { (addr as *mut u8).add(p * page_size) };

                let t0 = mach_time();
                // SAFETY: page_ptr points within a valid mmap'd region. We write then
                // read to trigger a page fault and install a TLB entry.
                unsafe {
                    std::ptr::write_volatile(page_ptr, 0xAA);
                    let _v = std::ptr::read_volatile(page_ptr);
                }
                let t1 = mach_time();

                timings.push(t1.wrapping_sub(t0));
            }

            // SAFETY: addr was returned by mmap (checked != MAP_FAILED) with size map_size.
            unsafe {
                libc::munmap(addr, map_size);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn page_fault_timing_collects_bytes() {
        let src = PageFaultTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn source_info_category() {
        assert_eq!(
            PageFaultTimingSource.info().category,
            SourceCategory::Timing
        );
    }

    #[test]
    fn source_info_name() {
        assert_eq!(PageFaultTimingSource.name(), "page_fault_timing");
    }
}
