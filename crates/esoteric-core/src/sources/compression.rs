//! Compression and hash timing entropy sources.
//!
//! These sources exploit data-dependent branch prediction behaviour and
//! micro-architectural side-effects to extract timing entropy from
//! compression (zlib) and hashing (SHA-256) operations.

use std::io::Write;
use std::time::Instant;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Extract LSBs from u64 deltas, packing 8 bits per byte.
fn extract_lsbs_u64(deltas: &[u64]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::with_capacity(deltas.len());
    for d in deltas {
        bits.push((d & 1) as u8);
    }

    let mut bytes = Vec::with_capacity(bits.len() / 8 + 1);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
}

/// SHA-256 hash-extend: stretch a short entropy seed to `needed` bytes.
fn hash_extend(seed_entropy: &[u8], raw_timings: &[u64], needed: usize) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(seed_entropy);
    for t in raw_timings {
        hasher.update(t.to_le_bytes());
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    hasher.update(ts.as_nanos().to_le_bytes());

    let seed: [u8; 32] = hasher.finalize().into();
    let mut entropy = seed_entropy.to_vec();
    let mut state = seed;
    while entropy.len() < needed {
        let mut h = Sha256::new();
        h.update(state);
        h.update((entropy.len() as u64).to_le_bytes());
        state = h.finalize().into();
        entropy.extend_from_slice(&state);
    }
    entropy.truncate(needed);
    entropy
}

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
        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Use an LCG for varying the data content across iterations.
        let mut lcg: u64 = Instant::now().elapsed().as_nanos() as u64 | 1;

        for i in 0..raw_count {
            // Create a 192-byte buffer: mix of random, pattern, and random.
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

            // Last 64 bytes: more pseudo-random, seeded differently
            for byte in data[128..].iter_mut() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(i as u64);
                *byte = (lcg >> 32) as u8;
            }

            // Measure compression time at nanosecond precision.
            let t0 = Instant::now();

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            let _ = encoder.write_all(&data);
            let _ = encoder.finish();

            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // XOR consecutive deltas.
        let xor_deltas: Vec<u64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract LSBs.
        let mut entropy = extract_lsbs_u64(&xor_deltas);

        if entropy.len() < n_samples {
            entropy = hash_extend(&entropy, &timings, n_samples);
        }

        entropy.truncate(n_samples);
        entropy
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
pub struct HashTimingSource;

impl EntropySource for HashTimingSource {
    fn info(&self) -> &SourceInfo {
        &HASH_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Use an LCG for generating varying data.
        let mut lcg: u64 = Instant::now().elapsed().as_nanos() as u64 | 1;

        for i in 0..raw_count {
            // Create varying-size data (64 to 512 bytes).
            let size = 64 + (i % 449); // varies from 64 to 512
            let mut data = Vec::with_capacity(size);
            for _ in 0..size {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                data.push((lcg >> 32) as u8);
            }

            // Measure SHA-256 hash time.
            let t0 = Instant::now();

            let mut hasher = Sha256::new();
            hasher.update(&data);
            let digest = hasher.finalize();
            std::hint::black_box(&digest);

            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // Extract LSBs directly (deltas already carry jitter).
        let mut entropy = extract_lsbs_u64(&deltas);

        if entropy.len() < n_samples {
            entropy = hash_extend(&entropy, &timings, n_samples);
        }

        entropy.truncate(n_samples);
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_timing_info() {
        let src = CompressionTimingSource;
        assert_eq!(src.name(), "compression_timing");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 1800.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compression_timing_collects_bytes() {
        let src = CompressionTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert_eq!(data.len(), 64);
    }

    #[test]
    fn hash_timing_info() {
        let src = HashTimingSource;
        assert_eq!(src.name(), "hash_timing");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 2000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hash_timing_collects_bytes() {
        let src = HashTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert_eq!(data.len(), 64);
    }

    #[test]
    fn extract_lsbs_basic() {
        let deltas = vec![3u64, 6, 9, 12, 15, 18, 21, 24];
        let bytes = extract_lsbs_u64(&deltas);
        // Bits: 1,0,1,0,1,0,1,0 -> 0xAA
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0xAA);
    }
}
