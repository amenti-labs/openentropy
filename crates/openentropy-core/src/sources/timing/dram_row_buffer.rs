//! DRAM row buffer hit/miss timing entropy source.

use rand::Rng;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

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
    category: SourceCategory::Timing,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 3.0,
    composite: false,
    is_fast: false,
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
    fn source_info_category() {
        assert_eq!(DRAMRowBufferSource.info().category, SourceCategory::Timing);
    }

    #[test]
    fn source_info_name() {
        assert_eq!(DRAMRowBufferSource.name(), "dram_row_buffer");
    }
}
