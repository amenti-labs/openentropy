//! Entropy conditioning: SHA-256, Von Neumann debiasing, XOR folding.

use sha2::{Digest, Sha256};

/// SHA-256 condition raw entropy into high-quality output.
///
/// Feeds: state || sample || counter || timestamp.
/// Returns new state and 32 bytes of conditioned output.
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

    // Timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    h.update(ts.as_nanos().to_le_bytes());

    h.update(extra);

    let digest: [u8; 32] = h.finalize().into();
    (digest, digest)
}

/// Von Neumann debiasing: extract unbiased bits from a biased stream.
/// Takes pairs of bits: (0,1) → 0, (1,0) → 1, same → discard.
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

/// XOR-fold: reduce data by XORing pairs of bytes.
pub fn xor_fold(data: &[u8]) -> Vec<u8> {
    if data.len() < 2 {
        return data.to_vec();
    }
    let half = data.len() / 2;
    (0..half).map(|i| data[i] ^ data[half + i]).collect()
}

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
    let score = eff * 60.0 + comp_ratio.min(1.0) * 20.0 + (unique as f64 / 256.0).min(1.0) * 20.0;
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
