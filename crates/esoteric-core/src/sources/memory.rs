//! MemoryTimingSource — DRAM allocation and access timing.
//!
//! Repeatedly allocates page-sized anonymous mmap regions, touches them,
//! measures the timing, and extracts LSBs as entropy.

use std::ptr;
use std::time::Instant;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Page size for mmap allocations (4 KB on most platforms).
const PAGE_SIZE: usize = 4096;

static MEMORY_TIMING_INFO: SourceInfo = SourceInfo {
    name: "memory_timing",
    description: "DRAM allocation and access timing jitter via mmap",
    physics: "Times memory allocation (malloc/mmap) and access patterns. Allocation jitter \
              comes from heap fragmentation, page fault handling, and kernel memory pressure. \
              Access timing varies with: DRAM refresh interference (~64ms cycle), cache \
              hierarchy state (L1/L2/L3 hits vs misses), and memory controller scheduling.",
    category: SourceCategory::Hardware,
    platform_requirements: &[],
    entropy_rate_estimate: 1500.0,
};

/// Entropy source that harvests timing jitter from memory allocation and access.
pub struct MemoryTimingSource;

impl EntropySource for MemoryTimingSource {
    fn info(&self) -> &SourceInfo {
        &MEMORY_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        // mmap with MAP_ANONYMOUS is available on all Unix-like systems.
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(n_samples);
        let mut prev_ns: u64 = 0;

        // Each iteration: mmap a page, touch it, munmap it, measure timing.
        // We need n_samples + 1 iterations to get n_samples deltas.
        let iterations = n_samples + 1;

        for i in 0..iterations {
            let t0 = Instant::now();

            // Allocate a page-sized anonymous private mapping.
            let addr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    PAGE_SIZE,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                    -1, // no file descriptor
                    0,  // offset
                )
            };

            if addr == libc::MAP_FAILED {
                continue;
            }

            // Touch the page — trigger the page fault and force DRAM access.
            // Write a pattern and read it back to ensure the compiler doesn't
            // optimize away the access.
            unsafe {
                let page = addr as *mut u8;
                // Write to first and last byte of the page to touch both ends.
                ptr::write_volatile(page, 0xAA);
                ptr::write_volatile(page.add(PAGE_SIZE - 1), 0x55);

                // Read back to force a full round-trip.
                let _v1 = ptr::read_volatile(page);
                let _v2 = ptr::read_volatile(page.add(PAGE_SIZE - 1));

                // Unmap immediately to keep memory usage bounded.
                libc::munmap(addr, PAGE_SIZE);
            }

            let elapsed_ns = t0.elapsed().as_nanos() as u64;

            if i > 0 {
                // Delta between consecutive allocation timings.
                let delta = elapsed_ns.wrapping_sub(prev_ns);
                // XOR the low two bytes together for extra mixing, then take LSB.
                let mixed = (delta as u8) ^ ((delta >> 8) as u8);
                output.push(mixed);

                if output.len() >= n_samples {
                    break;
                }
            }

            prev_ns = elapsed_ns;
        }

        output.truncate(n_samples);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn memory_timing_collects_bytes() {
        let src = MemoryTimingSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert_eq!(data.len(), 128);
    }

    #[test]
    fn memory_timing_info() {
        let src = MemoryTimingSource;
        assert_eq!(src.name(), "memory_timing");
        assert_eq!(src.info().category, SourceCategory::Hardware);
    }
}
