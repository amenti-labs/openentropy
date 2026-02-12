//! Compression and hash timing entropy sources.
//!
//! These sources exploit data-dependent branch prediction behaviour and
//! micro-architectural side-effects to extract timing entropy from
//! compression (zlib) and hashing (SHA-256) operations.
//!
//! **Raw output characteristics:** LSBs of timing deltas between successive
//! operations. Shannon entropy ~3-5 bits/byte. The timing jitter is driven
//! by branch predictor state, cache contention, and pipeline hazards.
//!
//! Note: HashTimingSource uses SHA-256 as its *workload* (the thing being
//! timed) — this is NOT conditioning. The entropy comes from the timing
//! variation, not from the hash output.

use std::io::Write;
use std::time::Instant;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha2::{Digest, Sha256};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

// ---------------------------------------------------------------------------
// CompressionTimingSource
// ---------------------------------------------------------------------------

static COMPRESSION_TIMING_INFO: SourceInfo = SourceInfo {
    name: "compression_timing",
    description: "Zlib compression timing jitter from data-dependent branch prediction",
    physics: "Compresses varying data with zlib and measures per-operation timing. \
              Compression algorithms have heavily data-dependent branches (Huffman tree \
              traversal, LZ77 match finding). The CPU\u{2019}s branch predictor state from \
              ALL running code affects prediction accuracy for these branches. Pipeline \
              stalls from mispredictions create timing variation.",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 1800.0,
};

/// Entropy source that harvests timing jitter from zlib compression.
pub struct CompressionTimingSource;

impl EntropySource for CompressionTimingSource {
    fn info(&self) -> &SourceInfo {
        &COMPRESSION_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Oversample: each timing delta produces ~1 raw byte
        let raw_count = n_samples * 2 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        let mut lcg: u64 = Instant::now().elapsed().as_nanos() as u64 | 1;

        for i in 0..raw_count {
            let mut data = [0u8; 192];

            // First 64 bytes: pseudo-random
            for byte in data[..64].iter_mut() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                *byte = (lcg >> 32) as u8;
            }

            // Middle 64 bytes: repeating pattern (highly compressible)
            for (j, byte) in data[64..128].iter_mut().enumerate() {
                *byte = (j % 4) as u8;
            }

            // Last 64 bytes: more pseudo-random
            for byte in data[128..].iter_mut() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(i as u64);
                *byte = (lcg >> 32) as u8;
            }

            let t0 = Instant::now();
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            let _ = encoder.write_all(&data);
            let _ = encoder.finish();
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Extract raw LSBs of timing deltas
        let mut raw = Vec::with_capacity(n_samples);
        for pair in timings.windows(2) {
            let delta = pair[1].wrapping_sub(pair[0]);
            raw.push(delta as u8);
            if raw.len() >= n_samples {
                break;
            }
        }

        raw.truncate(n_samples);
        raw
    }
}

// ---------------------------------------------------------------------------
// HashTimingSource
// ---------------------------------------------------------------------------

static HASH_TIMING_INFO: SourceInfo = SourceInfo {
    name: "hash_timing",
    description: "SHA-256 hashing timing jitter from micro-architectural side effects",
    physics: "SHA-256 hashes data of varying sizes and measures timing. While SHA-256 is \
              algorithmically constant-time, the actual execution time varies due to: \
              memory access patterns for the message schedule, cache line alignment, TLB \
              state, and CPU frequency scaling. The timing also captures micro-architectural \
              side effects from other processes.",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
};

/// Entropy source that harvests timing jitter from SHA-256 hashing.
/// Note: SHA-256 is used as the *workload* being timed, not for conditioning.
pub struct HashTimingSource;

impl EntropySource for HashTimingSource {
    fn info(&self) -> &SourceInfo {
        &HASH_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 2 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        let mut lcg: u64 = Instant::now().elapsed().as_nanos() as u64 | 1;

        for i in 0..raw_count {
            let size = 64 + (i % 449);
            let mut data = Vec::with_capacity(size);
            for _ in 0..size {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                data.push((lcg >> 32) as u8);
            }

            // SHA-256 is the WORKLOAD being timed — not conditioning
            let t0 = Instant::now();
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let digest = hasher.finalize();
            std::hint::black_box(&digest);
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Extract raw LSBs of timing deltas
        let mut raw = Vec::with_capacity(n_samples);
        for pair in timings.windows(2) {
            let delta = pair[1].wrapping_sub(pair[0]);
            raw.push(delta as u8);
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
    fn compression_timing_collects_bytes() {
        let src = CompressionTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
    }

    #[test]
    fn hash_timing_collects_bytes() {
        let src = HashTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
    }
}
