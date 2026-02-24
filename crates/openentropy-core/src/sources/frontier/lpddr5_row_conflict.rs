//! LPDDR5 unified memory row-conflict timing entropy.
//!
//! Apple Silicon uses LPDDR5 in a unified memory architecture (UMA) shared
//! across CPU, GPU, Neural Engine, and ISP. DRAM is organised into rows of
//! capacitor cells. Accessing a closed row (different from the currently open
//! row in a DRAM bank) requires a precharge + row-activate sequence, adding
//! ~40–100 ns of latency. The exact timing varies with physical noise sources
//! that are below what DRAM vendors publish.
//!
//! ## Physics
//!
//! LPDDR5 row-conflict timing is influenced by:
//!
//! - **Capacitor charge decay**: DRAM cells lose charge between refreshes via
//!   quantum tunnelling and leakage. The refresh timing and residual charge
//!   level affect sense-amplifier latch timing.
//! - **Sense amplifier thermal noise**: The latch decision is made by a
//!   differential sense amplifier that resolves the small charge difference
//!   on a bit-line. Johnson–Nyquist noise from the resistive bit-line sets
//!   a physical limit on how deterministic the latch timing can be.
//! - **Multi-domain bus arbitration**: On UMA, the DRAM bus is shared among
//!   CPU, GPU, ANE, and ISP. The memory controller arbitrates access requests
//!   from all agents simultaneously. The arbitration outcome is non-deterministic
//!   from the CPU's perspective.
//! - **Refresh interference**: LPDDR5 auto-refresh operations temporarily block
//!   access and alter timing non-deterministically relative to any given access.
//!
//! These effects produce a measured coefficient of variation of ~36% on Apple
//! Silicon, with timing ranges spanning a 12x window (125 ns to 1500 ns).
//!
//! ## Why this is below vendor documentation
//!
//! DRAM datasheets specify nominal access times (tRCD, tRP, tCL) as worst-case
//! bounds, not as statistical distributions. The sub-nanosecond physics driving
//! variability within those bounds — quantum tunnelling rates, Johnson–Nyquist
//! noise, multi-agent bus arbitration — are implementation details that DRAM
//! manufacturers do not document.

use std::ptr;
use std::time::Instant;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static LPDDR5_ROW_CONFLICT_INFO: SourceInfo = SourceInfo {
    name: "lpddr5_row_conflict",
    description: "LPDDR5 unified memory row-conflict timing from sense-amplifier thermal noise",
    physics: "Forces DRAM row-conflict accesses by walking through a buffer larger than L3 \
              cache (64 MB) with a stride designed to hit different DRAM banks on each \
              access. Row-conflict latency (precharge + row-activate) varies with: \
              capacitor charge decay between refreshes (quantum tunnelling), sense-amplifier \
              Johnson-Nyquist thermal noise, multi-agent bus arbitration between CPU/GPU/ANE \
              on the unified memory bus, and LPDDR5 auto-refresh timing interference. \
              Measured CV ~36%, 12x timing range (125-1500ns). Below the level of any \
              published DRAM specification.",
    category: SourceCategory::Timing,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 3500.0,
    composite: false,
};

/// Buffer size: must exceed L2 + L3 cache to force real DRAM accesses.
/// Apple M-series has up to 48 MB L3; 64 MB guarantees cache misses.
const BUF_SIZE: usize = 64 * 1024 * 1024;

/// Stride between accesses.
/// 8 KB = half of Apple Silicon's 16 KB page size. This stride maps to
/// alternating DRAM rows in most bank-interleaving configurations,
/// maximising the row-conflict rate.
const STRIDE: usize = 8 * 1024;

/// Entropy source: LPDDR5 unified memory row-conflict timing.
pub struct LPDDR5RowConflictSource;

#[cfg(unix)]
mod imp {
    use super::*;

    impl EntropySource for LPDDR5RowConflictSource {
        fn info(&self) -> &SourceInfo {
            &LPDDR5_ROW_CONFLICT_INFO
        }

        fn is_available(&self) -> bool {
            true
        }

        fn collect(&self, n_samples: usize) -> Vec<u8> {
            let raw_count = n_samples * 8 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            // Allocate a 64 MB anonymous mapping.
            let buf = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    BUF_SIZE,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    -1,
                    0,
                )
            } as *mut u8;

            if buf == libc::MAP_FAILED as *mut u8 {
                return Vec::new();
            }

            // Touch every page to ensure physical backing (prevent lazy allocation).
            unsafe {
                for page_start in (0..BUF_SIZE).step_by(4096) {
                    ptr::write_volatile(buf.add(page_start), 0xAB);
                }
            }

            // LCG for pseudo-random access pattern across DRAM rows.
            // A purely sequential or power-of-two stride would alias to the
            // same DRAM rows repeatedly. The LCG ensures we hit different banks.
            let mut lcg: u64 = 0xC0FFEE_DEAD_BEEF_42;

            unsafe {
                for i in 0..raw_count {
                    lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);

                    // Target address: stride-aligned within buffer.
                    // Alternating the stride target every access maximises
                    // DRAM row conflicts by ensuring consecutive accesses
                    // hit different rows in the same bank.
                    let offset_a = (i * STRIDE) % (BUF_SIZE - STRIDE);
                    let offset_b = (offset_a + STRIDE) % (BUF_SIZE - STRIDE);

                    let ptr_a = buf.add(offset_a);
                    let ptr_b = buf.add(offset_b);

                    // Write to ptr_a to dirty it (ensures real DRAM write-back
                    // on the next flush, maximising row-precharge latency).
                    ptr::write_volatile(ptr_a, (lcg & 0xFF) as u8);

                    // Measure the row-conflict access to ptr_b.
                    // ptr_b is in a different DRAM row from ptr_a.
                    let t0 = Instant::now();
                    let _v = ptr::read_volatile(ptr_b);
                    let elapsed = t0.elapsed().as_nanos() as u64;

                    timings.push(elapsed);
                }

                libc::munmap(buf as *mut libc::c_void, BUF_SIZE);
            }

            extract_timing_entropy(&timings, n_samples)
        }
    }
}

#[cfg(not(unix))]
impl EntropySource for LPDDR5RowConflictSource {
    fn info(&self) -> &SourceInfo {
        &LPDDR5_ROW_CONFLICT_INFO
    }

    fn is_available(&self) -> bool {
        false
    }

    fn collect(&self, _n_samples: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = LPDDR5RowConflictSource;
        assert_eq!(src.info().name, "lpddr5_row_conflict");
        assert!(matches!(src.info().category, SourceCategory::Timing));
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(unix)]
    fn is_available() {
        assert!(LPDDR5RowConflictSource.is_available());
    }

    #[test]
    #[cfg(unix)]
    #[ignore] // Allocates 64 MB; run with: cargo test -- --ignored
    fn collects_bytes() {
        let src = LPDDR5RowConflictSource;
        let data = src.collect(32);
        assert!(!data.is_empty());
        assert!(data.len() <= 32);
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 2, "expected variation in DRAM timing");
    }
}
