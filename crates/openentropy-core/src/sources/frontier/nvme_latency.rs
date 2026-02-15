//! NVMe I/O stack timing — storage controller scheduling entropy.
//!
//! Each read with F_NOCACHE (bypassing OS buffer cache) traverses:
//! - Filesystem metadata lookup and block mapping
//! - NVMe command submission and completion queue arbitration
//! - SSD controller DRAM cache lookup and scheduling
//! - SSD internal firmware scheduling (garbage collection, wear leveling)
//!
//! Note: the file is freshly written, so data typically resides in the SSD's
//! internal DRAM cache rather than NAND cells. The entropy comes from I/O
//! stack scheduling nondeterminism, not NAND cell physics.
//!
//! PoC measured H∞ ≈ 2.3 bits/byte for multi-offset reads.

use std::io::{Read, Seek, SeekFrom, Write};
use std::time::Instant;

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

/// Number of distinct offsets to cycle through (hitting different NAND pages).
const N_OFFSETS: usize = 8;

/// Block size for each read.
const BLOCK_SIZE: usize = 4096;

static NVME_LATENCY_INFO: SourceInfo = SourceInfo {
    name: "nvme_latency",
    description: "NVMe I/O stack timing jitter from storage controller scheduling",
    physics: "Reads a file at multiple offsets with OS buffer cache bypassed (F_NOCACHE). \
              Each read traverses: filesystem metadata lookup \u{2192} NVMe command queue \
              submission \u{2192} SSD controller DRAM cache \u{2192} completion interrupt. \
              Timing jitter arises from NVMe command queue arbitration, SSD controller \
              firmware scheduling (garbage collection, wear leveling background tasks), \
              and interrupt delivery latency. Note: freshly-written data typically resides \
              in SSD DRAM cache, not NAND cells. \
              PoC measured H\u{221e} \u{2248} 2.3 bits/byte.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 1000.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from NVMe flash cell reads.
pub struct NVMeLatencySource;

impl EntropySource for NVMeLatencySource {
    fn info(&self) -> &SourceInfo {
        &NVME_LATENCY_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Create a temp file with varied data across multiple offsets.
        let mut tmpfile = match tempfile::NamedTempFile::new() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let total_size = BLOCK_SIZE * N_OFFSETS;
        let mut fill = vec![0u8; total_size];
        let mut lcg: u64 = 0xDEAD_BEEF_CAFE_1234;
        for chunk in fill.chunks_mut(8) {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bytes = lcg.to_le_bytes();
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = bytes[i % 8];
            }
        }
        if tmpfile.write_all(&fill).is_err() {
            return Vec::new();
        }
        if tmpfile.flush().is_err() {
            return Vec::new();
        }

        // Disable buffer caching on macOS.
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: F_NOCACHE is a valid fcntl command on macOS that disables
            // the unified buffer cache for this file descriptor.
            unsafe {
                libc::fcntl(tmpfile.as_raw_fd(), libc::F_NOCACHE, 1);
            }
        }

        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut read_buf = vec![0u8; BLOCK_SIZE];

        for i in 0..raw_count {
            let offset = (i % N_OFFSETS) as u64 * BLOCK_SIZE as u64;
            if tmpfile.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }
            let t0 = Instant::now();
            let _ = tmpfile.read(&mut read_buf);
            let elapsed = t0.elapsed();
            timings.push(elapsed.as_nanos() as u64);
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = NVMeLatencySource;
        assert_eq!(src.name(), "nvme_latency");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // I/O dependent
    fn collects_bytes() {
        let src = NVMeLatencySource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }
}
