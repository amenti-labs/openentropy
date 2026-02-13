//! Silicon-level entropy sources that exploit CPU and DRAM microarchitecture
//! timing: row buffer contention, cache hierarchy interference, page fault
//! resolution, and speculative execution pipeline state.

use std::time::Instant;

use rand::Rng;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

// ---------------------------------------------------------------------------
// Platform-specific high-resolution timing
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_absolute_time() -> u64;
}

/// High-resolution timestamp. On macOS this reads the ARM system counter
/// directly via `mach_absolute_time()`; on other platforms it falls back to
/// `std::time::Instant` converted to nanoseconds.
#[cfg(target_os = "macos")]
fn mach_time() -> u64 {
    unsafe { mach_absolute_time() }
}

#[cfg(not(target_os = "macos"))]
fn mach_time() -> u64 {
    // Use a thread-local epoch so successive calls produce meaningful deltas.
    use std::cell::RefCell;
    thread_local! {
        static EPOCH: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }
    EPOCH.with(|cell| {
        let mut slot = cell.borrow_mut();
        let epoch = *slot.get_or_insert_with(Instant::now);
        epoch.elapsed().as_nanos() as u64
    })
}

// ---------------------------------------------------------------------------
// Shared helper: extract entropy from timing deltas
// ---------------------------------------------------------------------------

/// Takes a slice of raw timestamps, computes consecutive deltas, XORs
/// adjacent deltas, and extracts the lowest byte of each XOR'd value.
///
/// **Raw output characteristics:** LSBs of XOR'd timing deltas.
/// Shannon entropy ~4-6 bits/byte depending on source. No conditioning applied.
fn extract_timing_entropy(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 2 {
        return Vec::new();
    }

    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // XOR consecutive deltas for mixing (not conditioning — just combines adjacent values)
    let xored: Vec<u64> = deltas.windows(2).map(|w| w[0] ^ w[1]).collect();

    // Extract LSBs (lowest byte) — raw, unconditioned
    let mut raw: Vec<u8> = xored.iter().map(|&x| (x & 0xFF) as u8).collect();
    raw.truncate(n_samples);
    raw
}

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
        // raw timings. Oversample to ensure we have enough.
        let num_accesses = n_samples * 2 + 64;

        // Allocate a large buffer and touch it to ensure pages are backed.
        let mut buffer: Vec<u8> = vec![0u8; BUF_SIZE];
        for i in (0..BUF_SIZE).step_by(4096) {
            buffer[i] = i as u8;
        }

        let mut rng = rand::rng();
        let mut timings = Vec::with_capacity(num_accesses);

        for _ in 0..num_accesses {
            let idx = rng.random_range(0..BUF_SIZE);

            let t0 = mach_time();
            // Volatile read to prevent compiler from eliding the access.
            let _val = unsafe { std::ptr::read_volatile(&buffer[idx]) };
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

        let num_rounds = n_samples * 2 + 64;
        let mut rng = rand::rng();
        let mut timings = Vec::with_capacity(num_rounds);

        for round in 0..num_rounds {
            let t0 = mach_time();

            if round % 2 == 0 {
                // Sequential access — cache-friendly.
                let start = rng.random_range(0..BUF_SIZE.saturating_sub(256));
                let mut sink: u8 = 0;
                for offset in 0..256 {
                    sink ^= unsafe { std::ptr::read_volatile(&buffer[start + offset]) };
                }
                std::hint::black_box(sink);
            } else {
                // Random access — cache-hostile.
                let mut sink: u8 = 0;
                for _ in 0..256 {
                    let idx = rng.random_range(0..BUF_SIZE);
                    sink ^= unsafe { std::ptr::read_volatile(&buffer[idx]) };
                }
                std::hint::black_box(sink);
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
};

impl EntropySource for PageFaultTimingSource {
    fn info(&self) -> &SourceInfo {
        &PAGE_FAULT_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let num_pages: usize = 4;
        let map_size = page_size * num_pages;

        // Number of mmap/touch/munmap cycles. Each cycle produces `num_pages`
        // timings, so we need enough cycles to yield n_samples after whitening.
        let num_cycles = (n_samples * 2 / num_pages) + 4;

        let mut timings = Vec::with_capacity(num_cycles * num_pages);

        for _ in 0..num_cycles {
            // Allocate anonymous pages via mmap.
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

            // Touch each page to trigger a minor fault and time it.
            for p in 0..num_pages {
                let page_ptr = unsafe { (addr as *mut u8).add(p * page_size) };

                let before = Instant::now();
                unsafe {
                    // Write to the page to force a fault.
                    std::ptr::write_volatile(page_ptr, 0xAA);
                }
                let elapsed_ns = before.elapsed().as_nanos() as u64;

                timings.push(elapsed_ns);
            }

            // Unmap so the next cycle gets fresh pages.
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

    #[test]
    fn extract_timing_entropy_basic() {
        // Hand-crafted timings to verify the extraction logic.
        let timings = vec![100, 110, 105, 120, 108, 130, 112, 125];
        let result = extract_timing_entropy(&timings, 4);
        assert!(!result.is_empty());
        assert!(result.len() <= 4);
    }

    #[test]
    fn extract_timing_entropy_too_few_samples() {
        assert!(extract_timing_entropy(&[], 10).is_empty());
        assert!(extract_timing_entropy(&[42], 10).is_empty());
    }
}
