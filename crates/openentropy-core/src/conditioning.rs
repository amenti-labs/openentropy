//! Centralized entropy conditioning module.
//!
//! **ALL** post-processing of raw entropy lives here — no conditioning code
//! should exist in individual source implementations. Sources produce raw bytes;
//! this module is the single, auditable gateway for any transformation.
//!
//! # Architecture
//!
//! ```text
//! Source → Raw Bytes → Conditioning Layer (this module) → Output
//! ```
//!
//! # Conditioning Modes
//!
//! - **Raw**: No processing. XOR-combined bytes pass through unchanged.
//!   Preserves the actual hardware noise signal for research.
//! - **VonNeumann**: Debias only. Removes first-order bias without destroying
//!   the noise structure. Output is shorter than input (~25% yield).
//! - **Sha256**: Full SHA-256 conditioning with counter and timestamp mixing.
//!   Produces cryptographically strong output but destroys the raw signal.
//!
//! Most QRNG APIs (ANU, Outshift/Cisco) apply DRBG post-processing that makes
//! output indistinguishable from PRNG. The `Raw` mode here is what makes
//! openentropy useful for researchers studying actual hardware noise.

use sha2::{Digest, Sha256};

/// Conditioning mode for entropy output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum ConditioningMode {
    /// No conditioning. Raw bytes pass through unchanged.
    Raw,
    /// Von Neumann debiasing only.
    VonNeumann,
    /// SHA-256 hash conditioning (default). Cryptographically strong output.
    #[default]
    Sha256,
}


impl std::fmt::Display for ConditioningMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::VonNeumann => write!(f, "von_neumann"),
            Self::Sha256 => write!(f, "sha256"),
        }
    }
}

// ---------------------------------------------------------------------------
// Central conditioning gateway
// ---------------------------------------------------------------------------

/// Apply the specified conditioning mode to raw entropy bytes.
///
/// This is the **single gateway** for all entropy conditioning. No other code
/// in the crate should perform SHA-256, Von Neumann debiasing, or any other
/// form of whitening/post-processing on entropy data.
///
/// - `Raw`: returns the input unchanged (truncated or zero-padded to `n_output`)
/// - `VonNeumann`: debiases then truncates/pads to `n_output`
/// - `Sha256`: chained SHA-256 hashing to produce exactly `n_output` bytes
pub fn condition(raw: &[u8], n_output: usize, mode: ConditioningMode) -> Vec<u8> {
    match mode {
        ConditioningMode::Raw => {
            let mut out = raw.to_vec();
            out.truncate(n_output);
            out
        }
        ConditioningMode::VonNeumann => {
            let debiased = von_neumann_debias(raw);
            let mut out = debiased;
            out.truncate(n_output);
            out
        }
        ConditioningMode::Sha256 => sha256_condition_bytes(raw, n_output),
    }
}

// ---------------------------------------------------------------------------
// SHA-256 conditioning
// ---------------------------------------------------------------------------

/// SHA-256 chained conditioning: stretches or compresses raw bytes to exactly
/// `n_output` bytes using counter-mode hashing.
///
/// Each 32-byte output block is: SHA-256(state || chunk || counter).
/// State is chained from the previous block's digest.
pub fn sha256_condition_bytes(raw: &[u8], n_output: usize) -> Vec<u8> {
    if raw.is_empty() {
        return vec![0u8; n_output];
    }
    let mut output = Vec::with_capacity(n_output);
    let mut state = [0u8; 32];
    let mut offset = 0;
    let mut counter: u64 = 0;
    while output.len() < n_output {
        let end = (offset + 64).min(raw.len());
        let chunk = &raw[offset..end];
        let mut h = Sha256::new();
        h.update(state);
        h.update(chunk);
        h.update(counter.to_le_bytes());
        state = h.finalize().into();
        output.extend_from_slice(&state);
        offset += 64;
        counter += 1;
        if offset >= raw.len() {
            offset = 0;
        }
    }
    output.truncate(n_output);
    output
}

/// SHA-256 condition with explicit state, sample, counter, and extra data.
/// Returns (new_state, 32-byte digest).
pub fn sha256_condition(
    state: &[u8; 32],
    sample: &[u8],
    counter: u64,
    extra: &[u8],
) -> ([u8; 32], [u8; 32]) {
    let mut h = Sha256::new();
    h.update(state);
    h.update(sample);
    h.update(counter.to_le_bytes());

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    h.update(ts.as_nanos().to_le_bytes());

    h.update(extra);

    let digest: [u8; 32] = h.finalize().into();
    (digest, digest)
}

// ---------------------------------------------------------------------------
// Von Neumann debiasing
// ---------------------------------------------------------------------------

/// Von Neumann debiasing: extract unbiased bits from a biased stream.
///
/// Takes pairs of bits: (0,1) → 0, (1,0) → 1, same → discard.
/// Expected yield: ~25% of input bits (for unbiased input).
pub fn von_neumann_debias(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::new();
    for byte in data {
        for i in (0..8).step_by(2) {
            let b1 = (byte >> (7 - i)) & 1;
            let b2 = (byte >> (6 - i)) & 1;
            if b1 != b2 {
                bits.push(b1);
            }
        }
    }

    // Pack bits back into bytes
    let mut result = Vec::with_capacity(bits.len() / 8);
    for chunk in bits.chunks_exact(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        result.push(byte);
    }
    result
}

// ---------------------------------------------------------------------------
// XOR folding
// ---------------------------------------------------------------------------

/// XOR-fold: reduce data by XORing the first half with the second half.
pub fn xor_fold(data: &[u8]) -> Vec<u8> {
    if data.len() < 2 {
        return data.to_vec();
    }
    let half = data.len() / 2;
    (0..half).map(|i| data[i] ^ data[half + i]).collect()
}

// ---------------------------------------------------------------------------
// Quick analysis utilities
// ---------------------------------------------------------------------------

/// Quick Shannon entropy in bits/byte for a byte slice.
pub fn quick_shannon(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    let mut h = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / n;
            h -= p * p.log2();
        }
    }
    h
}

/// Quick quality assessment.
pub fn quick_quality(data: &[u8]) -> QualityReport {
    if data.len() < 16 {
        return QualityReport {
            samples: data.len(),
            unique_values: 0,
            shannon_entropy: 0.0,
            compression_ratio: 0.0,
            quality_score: 0.0,
            grade: 'F',
        };
    }

    let shannon = quick_shannon(data);

    // Compression ratio
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap_or_default();
    let compressed = encoder.finish().unwrap_or_default();
    let comp_ratio = compressed.len() as f64 / data.len() as f64;

    // Unique values
    let mut seen = [false; 256];
    for &b in data {
        seen[b as usize] = true;
    }
    let unique = seen.iter().filter(|&&s| s).count();

    let eff = shannon / 8.0;
    let score =
        eff * 60.0 + comp_ratio.min(1.0) * 20.0 + (unique as f64 / 256.0).min(1.0) * 20.0;
    let grade = if score >= 80.0 {
        'A'
    } else if score >= 60.0 {
        'B'
    } else if score >= 40.0 {
        'C'
    } else if score >= 20.0 {
        'D'
    } else {
        'F'
    };

    QualityReport {
        samples: data.len(),
        unique_values: unique,
        shannon_entropy: shannon,
        compression_ratio: comp_ratio,
        quality_score: score,
        grade,
    }
}

#[derive(Debug, Clone)]
pub struct QualityReport {
    pub samples: usize,
    pub unique_values: usize,
    pub shannon_entropy: f64,
    pub compression_ratio: f64,
    pub quality_score: f64,
    pub grade: char,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_raw_passthrough() {
        let data = vec![1, 2, 3, 4, 5];
        let out = condition(&data, 3, ConditioningMode::Raw);
        assert_eq!(out, vec![1, 2, 3]);
    }

    #[test]
    fn test_condition_sha256_produces_exact_length() {
        let data = vec![42u8; 100];
        let out = condition(&data, 64, ConditioningMode::Sha256);
        assert_eq!(out.len(), 64);
    }

    #[test]
    fn test_von_neumann_reduces_size() {
        let input = vec![0b10101010u8; 128];
        let output = von_neumann_debias(&input);
        assert!(output.len() < input.len());
    }

    #[test]
    fn test_condition_modes_differ() {
        let data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        let raw = condition(&data, 64, ConditioningMode::Raw);
        let sha = condition(&data, 64, ConditioningMode::Sha256);
        assert_ne!(raw, sha);
    }
}
