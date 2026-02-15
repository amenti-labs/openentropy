//! Compression and hash timing entropy sources.
//!
//! These sources exploit data-dependent branch prediction behaviour and
//! micro-architectural side-effects to extract timing entropy from
//! compression (zlib) and hashing (SHA-256) operations.
//!
//! **Raw output characteristics:** XOR-folded timing deltas between successive
//! operations. Shannon entropy ~5-7 bits/byte. The timing jitter is driven
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

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

use super::helpers::{extract_timing_entropy, mach_time};

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
    category: SourceCategory::Signal,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 1800.0,
    composite: false,
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
        // 4x oversampling for better XOR-fold quality.
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Seed from high-resolution timer for per-call variation.
        let mut lcg: u64 = mach_time() | 1;

        for i in 0..raw_count {
            // Vary data size (128-512 bytes) to create more timing diversity.
            let data_len = 128 + (lcg as usize % 385);
            let mut data = vec![0u8; data_len];

            // First third: pseudo-random
            let third = data_len / 3;
            for byte in data[..third].iter_mut() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                *byte = (lcg >> 32) as u8;
            }

            // Middle third: repeating pattern (highly compressible)
            for (j, byte) in data[third..third * 2].iter_mut().enumerate() {
                *byte = (j % 4) as u8;
            }

            // Last third: more pseudo-random
            for byte in data[third * 2..].iter_mut() {
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

        extract_timing_entropy(&timings, n_samples)
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
    category: SourceCategory::Signal,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
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
        // 4x oversampling for better XOR-fold quality after delta computation.
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Seed from high-resolution timer for per-call variation.
        let mut lcg: u64 = mach_time() | 1;

        for i in 0..raw_count {
            // Wider range of sizes (32-2048 bytes) to create more timing diversity.
            let size = 32 + (lcg as usize % 2017);
            let mut data = Vec::with_capacity(size);
            for _ in 0..size {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                data.push((lcg >> 32) as u8);
            }

            // SHA-256 is the WORKLOAD being timed — not conditioning.
            // Hash multiple rounds for smaller inputs to amplify timing variation.
            let rounds = if size < 256 { 3 } else { 1 };
            let t0 = Instant::now();
            for _ in 0..rounds {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let digest = hasher.finalize();
                std::hint::black_box(&digest);
                // Feed digest back as additional data to prevent loop elision
                if let Some(b) = data.last_mut() {
                    *b ^= digest[i % 32];
                }
            }
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_timing_info() {
        let src = CompressionTimingSource;
        assert_eq!(src.name(), "compression_timing");
        assert_eq!(src.info().category, SourceCategory::Signal);
        assert!(!src.info().composite);
    }

    #[test]
    fn hash_timing_info() {
        let src = HashTimingSource;
        assert_eq!(src.name(), "hash_timing");
        assert_eq!(src.info().category, SourceCategory::Signal);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn compression_timing_collects_bytes() {
        let src = CompressionTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn hash_timing_collects_bytes() {
        let src = HashTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
    }
}
