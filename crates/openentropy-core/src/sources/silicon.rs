//! Silicon-level entropy sources that exploit CPU and DRAM microarchitecture
//! timing: row buffer contention, cache hierarchy interference, page fault
//! resolution, and speculative execution pipeline state.

use rand::Rng;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

use super::helpers::{extract_timing_entropy, mach_time};

// ---------------------------------------------------------------------------
// 1. DRAMRowBufferSource
// ---------------------------------------------------------------------------

/// Measures DRAM row buffer hit/miss timing by accessing random locations in a
/// large (32 MB) buffer that exceeds L2/L3 cache capacity. The exact timing of
/// each access depends on physical address mapping, row buffer state from all
/// system activity, memory controller scheduling, and DRAM refresh
/// interference.
pub struct DRAMRowBufferSource;

static DRAM_ROW_BUFFER_INFO: SourceInfo = SourceInfo {
    name: "dram_row_buffer",
    description: "DRAM row buffer hit/miss timing from random memory accesses",
    physics: "Measures DRAM row buffer hit/miss timing by accessing different memory rows. \
              DRAM is organized into rows of capacitor cells. Accessing an open row (hit) \
              is fast; accessing a different row requires precharge + activate (miss), \
              which is slower. The exact timing depends on: physical address mapping, \
              row buffer state from ALL system activity, memory controller scheduling, \
              and DRAM refresh interference.",
    category: SourceCategory::Silicon,
    platform_requirements: &[],
    entropy_rate_estimate: 3000.0,
    composite: false,
};

impl EntropySource for DRAMRowBufferSource {
    fn info(&self) -> &SourceInfo {
        &DRAM_ROW_BUFFER_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        const BUF_SIZE: usize = 32 * 1024 * 1024; // 32 MB — exceeds L2/L3 cache

        // We need ~(n_samples + 2) XOR'd deltas, which requires ~(n_samples + 4)
        // raw timings. Oversample 4x to give XOR-folding more to work with.
        let num_accesses = n_samples * 4 + 64;

        // Allocate a large buffer and touch it to ensure pages are backed.
        let mut buffer: Vec<u8> = vec![0u8; BUF_SIZE];
        for i in (0..BUF_SIZE).step_by(4096) {
            buffer[i] = i as u8;
        }

        let mut rng = rand::rng();
        let mut timings = Vec::with_capacity(num_accesses);

        for _ in 0..num_accesses {
            // Access two distant random locations per measurement to amplify
            // row buffer miss timing variation.
            let idx1 = rng.random_range(0..BUF_SIZE);
            let idx2 = rng.random_range(0..BUF_SIZE);

            let t0 = mach_time();
            // SAFETY: idx1 and idx2 are bounded by BUF_SIZE via random_range.
            // read_volatile prevents the compiler from eliding the accesses.
            let _v1 = unsafe { std::ptr::read_volatile(&buffer[idx1]) };
            let _v2 = unsafe { std::ptr::read_volatile(&buffer[idx2]) };
            let t1 = mach_time();

            timings.push(t1.wrapping_sub(t0));
        }

        // Prevent the buffer from being optimized away.
        std::hint::black_box(&buffer);

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// 2. CacheContentionSource
// ---------------------------------------------------------------------------

/// Measures L1/L2 cache miss patterns by alternating between sequential
/// (cache-friendly) and random (cache-hostile) access patterns on an 8 MB
/// buffer that spans the L2 boundary. Cache timing depends on what every other
/// process and hardware unit is doing — the cache is a shared resource whose
/// state is fundamentally unpredictable.
pub struct CacheContentionSource;

static CACHE_CONTENTION_INFO: SourceInfo = SourceInfo {
    name: "cache_contention",
    description: "L1/L2 cache contention timing from alternating access patterns",
    physics: "Measures L1/L2 cache miss patterns by alternating access patterns. Cache \
              timing depends on what every other process and hardware unit is doing \
              \u{2014} the cache is a shared resource whose state is fundamentally \
              unpredictable. A cache miss requires main memory access (100+ ns vs \
              1 ns for L1 hit).",
    category: SourceCategory::Silicon,
    platform_requirements: &[],
    entropy_rate_estimate: 2500.0,
    composite: false,
};

impl EntropySource for CacheContentionSource {
    fn info(&self) -> &SourceInfo {
        &CACHE_CONTENTION_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        const BUF_SIZE: usize = 8 * 1024 * 1024; // 8 MB — spans L2 boundary

        let mut buffer: Vec<u8> = vec![0u8; BUF_SIZE];
        // Touch pages to ensure they are resident.
        for i in (0..BUF_SIZE).step_by(4096) {
            buffer[i] = i as u8;
        }

        // 4x oversampling for better XOR-fold quality.
        let num_rounds = n_samples * 4 + 64;
        let mut rng = rand::rng();
        let mut timings = Vec::with_capacity(num_rounds);

        for round in 0..num_rounds {
            let t0 = mach_time();

            // Cycle through 3 access patterns for more contention diversity:
            // sequential, random, and strided (cache-line bouncing).
            match round % 3 {
                0 => {
                    // Sequential access — cache-friendly.
                    let start = rng.random_range(0..BUF_SIZE.saturating_sub(512));
                    let mut sink: u8 = 0;
                    for offset in 0..512 {
                        // SAFETY: start + offset < BUF_SIZE due to saturating_sub(512) bound.
                        sink ^= unsafe { std::ptr::read_volatile(&buffer[start + offset]) };
                    }
                    std::hint::black_box(sink);
                }
                1 => {
                    // Random access — cache-hostile.
                    let mut sink: u8 = 0;
                    for _ in 0..512 {
                        let idx = rng.random_range(0..BUF_SIZE);
                        // SAFETY: idx is bounded by BUF_SIZE via random_range.
                        sink ^= unsafe { std::ptr::read_volatile(&buffer[idx]) };
                    }
                    std::hint::black_box(sink);
                }
                _ => {
                    // Strided access — cache-line bouncing (64-byte stride).
                    let start = rng.random_range(0..BUF_SIZE.saturating_sub(512 * 64));
                    let mut sink: u8 = 0;
                    for i in 0..512 {
                        // SAFETY: start + i*64 < BUF_SIZE due to saturating_sub(512*64) bound.
                        sink ^= unsafe { std::ptr::read_volatile(&buffer[start + i * 64]) };
                    }
                    std::hint::black_box(sink);
                }
            }

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        std::hint::black_box(&buffer);

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// 3. PageFaultTimingSource
// ---------------------------------------------------------------------------

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
    category: SourceCategory::Silicon,
    platform_requirements: &[],
    entropy_rate_estimate: 1500.0,
    composite: false,
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
// 4. SpeculativeExecutionSource
// ---------------------------------------------------------------------------

/// Measures timing variations from the CPU's speculative execution engine. The
/// branch predictor maintains per-address history that depends on ALL
/// previously executed code. Mispredictions cause pipeline flushes (~15 cycle
/// penalty on M4). By running data-dependent branches and measuring timing, we
/// capture the predictor's internal state which is influenced by all prior
/// program activity on the core.
pub struct SpeculativeExecutionSource;

static SPECULATIVE_EXECUTION_INFO: SourceInfo = SourceInfo {
    name: "speculative_execution",
    description: "Branch predictor state timing via data-dependent branches",
    physics: "Measures timing variations from the CPU's speculative execution engine. \
              The branch predictor maintains per-address history that depends on ALL \
              previously executed code. Mispredictions cause pipeline flushes (~15 cycle \
              penalty on M4). By running data-dependent branches and measuring timing, \
              we capture the predictor's internal state.",
    category: SourceCategory::Silicon,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

impl EntropySource for SpeculativeExecutionSource {
    fn info(&self) -> &SourceInfo {
        &SPECULATIVE_EXECUTION_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Variable batch sizes to create different workloads per measurement.
        let num_batches = n_samples * 4 + 64;
        let mut timings = Vec::with_capacity(num_batches);

        // LCG state — seeded from the high-resolution clock so every call
        // exercises a different branch sequence.
        let mut lcg_state: u64 = mach_time() ^ 0xDEAD_BEEF_CAFE_BABE;

        for batch_idx in 0..num_batches {
            // Variable workload: batch size varies with index for different
            // branch predictor pressure levels.
            let batch_size = 10 + (batch_idx % 31);

            let t0 = mach_time();

            // Execute a batch of data-dependent branches that defeat the
            // branch predictor because outcomes depend on runtime LCG values.
            let mut accumulator: u64 = 0;
            for _ in 0..batch_size {
                // Advance LCG: x' = x * 6364136223846793005 + 1442695040888963407
                lcg_state = lcg_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);

                // Data-dependent branch — outcome unknowable to predictor.
                if lcg_state & 0x8000_0000 != 0 {
                    accumulator = accumulator.wrapping_add(lcg_state);
                } else {
                    accumulator = accumulator.wrapping_mul(lcg_state | 1);
                }

                // Additional branch for more predictor pressure.
                if (lcg_state >> 16) & 0xFF > 128 {
                    accumulator ^= lcg_state.rotate_left(7);
                } else {
                    accumulator ^= lcg_state.rotate_right(11);
                }

                // Third branch varying with batch index for extra state diversity.
                if (lcg_state >> 32) & 0x1 != 0 {
                    accumulator = accumulator.wrapping_add(batch_idx as u64);
                }
            }

            // Prevent the compiler from optimizing away the computation.
            std::hint::black_box(accumulator);

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
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
    fn dram_row_buffer_collects_bytes() {
        let src = DRAMRowBufferSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
        assert!(data.len() <= 128);
        // Sanity: not all bytes should be identical.
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes were identical");
        }
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn cache_contention_collects_bytes() {
        let src = CacheContentionSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
        assert!(data.len() <= 128);
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes were identical");
        }
    }

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
    #[ignore] // Run with: cargo test -- --ignored
    fn speculative_execution_collects_bytes() {
        let src = SpeculativeExecutionSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
        assert!(data.len() <= 128);
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes were identical");
        }
    }

    #[test]
    fn source_info_categories() {
        assert_eq!(DRAMRowBufferSource.info().category, SourceCategory::Silicon);
        assert_eq!(
            CacheContentionSource.info().category,
            SourceCategory::Silicon
        );
        assert_eq!(
            PageFaultTimingSource.info().category,
            SourceCategory::Silicon
        );
        assert_eq!(
            SpeculativeExecutionSource.info().category,
            SourceCategory::Silicon
        );
    }

    #[test]
    fn source_info_names() {
        assert_eq!(DRAMRowBufferSource.name(), "dram_row_buffer");
        assert_eq!(CacheContentionSource.name(), "cache_contention");
        assert_eq!(PageFaultTimingSource.name(), "page_fault_timing");
        assert_eq!(SpeculativeExecutionSource.name(), "speculative_execution");
    }
}
