//! DiskIOSource — NVMe/SSD read latency jitter.
//!
//! Creates a temporary 64KB file, performs random seeks and 4KB reads,
//! and extracts LSBs of nanosecond timing deltas as entropy.
//!
//! **Raw output characteristics:** LSBs of inter-read timing deltas.
//! Shannon entropy ~4-6 bits/byte. Biased toward low values due to
//! clustered read latencies. Use SHA-256 conditioning for uniform output.

use std::io::{Read, Seek, SeekFrom, Write};
use std::time::Instant;

use tempfile::NamedTempFile;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Size of the temporary file used for random reads.
const TEMP_FILE_SIZE: usize = 64 * 1024; // 64 KB

/// Size of each random read operation.
const READ_BLOCK_SIZE: usize = 4 * 1024; // 4 KB

static DISK_IO_INFO: SourceInfo = SourceInfo {
    name: "disk_io",
    description: "NVMe/SSD read latency jitter from random 4KB reads",
    physics: "Measures NVMe/SSD read latency for small random reads. Jitter sources: \
              flash translation layer (FTL) remapping, wear leveling, garbage collection, \
              read disturb mitigation, NAND page read latency variation (depends on charge \
              level in floating-gate transistors), and NVMe controller queue arbitration.",
    category: SourceCategory::Hardware,
    platform_requirements: &[],
    entropy_rate_estimate: 800.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from NVMe/SSD random reads.
pub struct DiskIOSource;

impl EntropySource for DiskIOSource {
    fn info(&self) -> &SourceInfo {
        &DISK_IO_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Create a temporary 64KB file filled with varied data.
        let mut tmpfile = match NamedTempFile::new() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let mut fill_data = vec![0u8; TEMP_FILE_SIZE];
        let mut lcg: u64 = 0xCAFE_BABE_DEAD_BEEF;
        for chunk in fill_data.chunks_mut(8) {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bytes = lcg.to_le_bytes();
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = bytes[i % 8];
            }
        }
        if tmpfile.write_all(&fill_data).is_err() {
            return Vec::new();
        }
        if tmpfile.flush().is_err() {
            return Vec::new();
        }

        let mut raw = Vec::with_capacity(n_samples);
        let mut read_buf = vec![0u8; READ_BLOCK_SIZE];
        let max_offset = TEMP_FILE_SIZE.saturating_sub(READ_BLOCK_SIZE);

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let mut lcg_state = seed | 1;

        let mut prev_ns: u64 = 0;

        // Oversample to ensure enough raw data.
        let num_reads = n_samples * 2 + 64;

        for i in 0..num_reads {
            lcg_state = lcg_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let offset = (lcg_state as usize) % (max_offset + 1);

            let t0 = Instant::now();
            let _ = tmpfile.seek(SeekFrom::Start(offset as u64));
            let _ = tmpfile.read(&mut read_buf);
            let elapsed_ns = t0.elapsed().as_nanos() as u64;

            if i > 0 {
                let delta = elapsed_ns.wrapping_sub(prev_ns);
                // Extract lowest byte of delta — raw, unconditioned
                raw.push(delta as u8);
            }

            prev_ns = elapsed_ns;

            if raw.len() >= n_samples {
                break;
            }
        }

        raw.truncate(n_samples);
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn disk_io_collects_bytes() {
        let src = DiskIOSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
    }

    #[test]
    fn disk_io_info() {
        let src = DiskIOSource;
        assert_eq!(src.name(), "disk_io");
        assert_eq!(src.info().category, SourceCategory::Hardware);
    }
}
