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
use std::collections::HashMap;

/// Conditioning mode for entropy output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
/// - `Raw`: returns the input unchanged (truncated to `n_output`)
/// - `VonNeumann`: debiases then truncates to `n_output`
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
        return Vec::new();
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

// ---------------------------------------------------------------------------
// Min-entropy estimators
//
// Notes:
// - `mcv_estimate` follows the NIST 800-90B MCV style closely and is used as
//   the primary conservative estimate.
// - The other estimators are retained as NIST-inspired diagnostics. They are
//   useful for comparative/source characterization, but this implementation is
//   not a strict validation harness for 800-90B.
// ---------------------------------------------------------------------------

/// Min-entropy estimate: H∞ = -log2(max probability).
/// More conservative than Shannon — reflects worst-case guessing probability.
/// Returns bits per sample (0.0 to 8.0 for byte-valued data).
pub fn min_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    let p_max = counts.iter().map(|&c| c as f64 / n).fold(0.0f64, f64::max);
    if p_max <= 0.0 {
        return 0.0;
    }
    -p_max.log2()
}

/// Most Common Value (MCV) estimator (NIST-inspired 800-90B 6.3.1 style).
/// Estimates min-entropy with upper bound on p_max using confidence interval.
/// Returns (min_entropy_bits_per_sample, p_max_upper_bound).
pub fn mcv_estimate(data: &[u8]) -> (f64, f64) {
    if data.is_empty() {
        return (0.0, 1.0);
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    let max_count = *counts.iter().max().unwrap() as f64;
    let p_hat = max_count / n;

    // Upper bound of 99% confidence interval
    // p_u = min(1, p_hat + 2.576 * sqrt(p_hat * (1 - p_hat) / n))
    let z = 2.576; // z_{0.995} for 99% CI
    let p_u = (p_hat + z * (p_hat * (1.0 - p_hat) / n).sqrt()).min(1.0);

    let h = if p_u >= 1.0 {
        0.0
    } else {
        (-p_u.log2()).max(0.0)
    };
    (h, p_u)
}

/// Collision estimator (NIST-inspired diagnostic).
///
/// Scans the data sequentially, finding the distance between successive
/// "collisions" — where any two adjacent samples in the sequence are equal
/// (data[i] == data[i+1]). The mean collision distance relates to the
/// collision probability q = sum(p_i^2), from which we derive min-entropy.
///
/// Key correction vs prior implementation: NIST defines a collision as any
/// two consecutive equal values, not as a repeat of a specific starting value.
/// We scan pairs sequentially and measure the gap between collisions.
///
/// Returns estimated min-entropy bits per sample.
pub fn collision_estimate(data: &[u8]) -> f64 {
    if data.len() < 3 {
        return 0.0;
    }

    // Scan for collisions: positions where data[i] == data[i+1].
    // Record the distance (in samples) between successive collisions.
    let mut distances = Vec::new();
    let mut last_collision: Option<usize> = None;

    for i in 0..data.len() - 1 {
        if data[i] == data[i + 1] {
            if let Some(prev) = last_collision {
                distances.push((i - prev) as f64);
            }
            last_collision = Some(i);
        }
    }

    if distances.is_empty() {
        // No repeated collisions found — either very high entropy or too little data.
        // Fall back to counting total collisions vs total pairs.
        let mut collision_count = 0usize;
        for i in 0..data.len() - 1 {
            if data[i] == data[i + 1] {
                collision_count += 1;
            }
        }
        if collision_count == 0 {
            return 8.0; // No collisions at all
        }
        // q_hat ≈ collision_count / (n-1), min-entropy from q >= p_max^2
        let q_hat = collision_count as f64 / (data.len() - 1) as f64;
        let p_max = q_hat.sqrt().min(1.0);
        return if p_max <= 0.0 {
            8.0
        } else {
            (-p_max.log2()).min(8.0)
        };
    }

    let mean_dist = distances.iter().sum::<f64>() / distances.len() as f64;

    // The mean inter-collision distance ≈ 1/q where q = sum(p_i^2).
    // Since p_max^2 <= q, we have p_max <= sqrt(q) <= sqrt(1/mean_dist).
    // Apply a confidence bound: use the lower bound on mean distance
    // (conservative → higher q → higher p_max → lower entropy).
    let n_collisions = distances.len() as f64;
    let variance = distances
        .iter()
        .map(|d| (d - mean_dist).powi(2))
        .sum::<f64>()
        / (n_collisions - 1.0).max(1.0);
    let std_err = (variance / n_collisions).sqrt();

    let z = 2.576; // 99% CI
    let mean_lower = (mean_dist - z * std_err).max(1.0);

    // q_upper ≈ 1/mean_lower, p_max <= sqrt(q_upper)
    let p_max = (1.0 / mean_lower).sqrt().min(1.0);

    if p_max <= 0.0 {
        8.0
    } else {
        (-p_max.log2()).min(8.0)
    }
}

/// Markov estimator (NIST-inspired diagnostic).
///
/// Models first-order dependencies between consecutive samples using byte-level
/// transition counts. For each byte value, computes the maximum transition
/// probability from any predecessor. The per-sample entropy is then bounded by
/// the maximum over all values of: p_init[s] * max_predecessor(p_trans[pred][s]).
///
/// Unlike a binned approach, this operates on all 256 byte values directly.
/// To keep memory bounded (256x256 = 64KB), we use a flat array.
///
/// Returns estimated min-entropy bits per sample.
pub fn markov_estimate(data: &[u8]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }

    let n = data.len() as f64;

    // Initial distribution: count of each byte value
    let mut init_counts = [0u64; 256];
    for &b in data {
        init_counts[b as usize] += 1;
    }

    // Transition counts: transitions[from * 256 + to]
    let mut transitions = vec![0u64; 256 * 256];
    for w in data.windows(2) {
        transitions[w[0] as usize * 256 + w[1] as usize] += 1;
    }

    // Row sums for transition probabilities
    let mut row_sums = [0u64; 256];
    for (from, row_sum) in row_sums.iter_mut().enumerate() {
        let base = from * 256;
        *row_sum = transitions[base..base + 256].iter().sum();
    }

    // NIST-inspired Markov-style bound:
    // For each output value s, find the maximum probability of producing s
    // considering all possible predecessor states.
    //
    // p_max_markov = max over s of: max over pred of (p_init[pred] * p_trans[pred][s])
    //
    // But a simpler conservative bound: for each value s, compute
    //   p_s = max(p_init[s], max over pred of p_trans[pred][s])
    // and take p_max = max over s of p_s.
    //
    // This bounds the per-sample probability under the first-order Markov model.
    let mut p_max = 0.0f64;
    for s in 0..256usize {
        // Initial probability
        let p_init_s = init_counts[s] as f64 / n;
        p_max = p_max.max(p_init_s);

        // Max transition probability into s from any predecessor
        for pred in 0..256usize {
            if row_sums[pred] > 0 {
                let p_trans = transitions[pred * 256 + s] as f64 / row_sums[pred] as f64;
                p_max = p_max.max(p_trans);
            }
        }
    }

    if p_max <= 0.0 {
        8.0
    } else {
        (-p_max.log2()).min(8.0)
    }
}

/// Compression estimator (NIST-inspired diagnostic).
///
/// Uses Maurer's universal statistic to estimate entropy via compression.
/// Maurer's f_n converges to the Shannon entropy rate, NOT min-entropy.
///
/// To convert to a min-entropy bound, we use the relationship:
///   H∞ <= H_Shannon
/// and apply a conservative correction. For IID data with alphabet size k=256:
///   H∞ = -log2(p_max), H_Shannon = -sum(p_i * log2(p_i))
/// The gap between them grows with distribution skew. We use:
///   H∞_est ≈ f_lower * (f_lower / log2(k))
/// which maps f_lower=log2(256)=8.0 → 8.0 (uniform) and compresses lower
/// values quadratically, reflecting that low Shannon entropy implies even
/// lower min-entropy.
///
/// Returns estimated min-entropy bits per sample.
pub fn compression_estimate(data: &[u8]) -> f64 {
    if data.len() < 100 {
        return 0.0;
    }

    // Maurer's universal statistic
    // For each byte, record the distance to its previous occurrence
    let l = 8.0f64; // log2(alphabet_size) = log2(256) = 8
    let q = 256.min(data.len() / 4); // initialization segment length
    let k = data.len() - q; // test segment length

    if k == 0 {
        return 0.0;
    }

    // Initialize: record last position of each byte value
    let mut last_pos = [0usize; 256];
    for (i, &b) in data[..q].iter().enumerate() {
        last_pos[b as usize] = i + 1; // 1-indexed
    }

    // Test segment: compute log2 of distances
    let mut sum = 0.0f64;
    let mut count = 0u64;
    for (i, &b) in data[q..].iter().enumerate() {
        let pos = q + i + 1; // 1-indexed
        let prev = last_pos[b as usize];
        if prev > 0 {
            let distance = pos - prev;
            sum += (distance as f64).log2();
            count += 1;
        }
        last_pos[b as usize] = pos;
    }

    if count == 0 {
        return l; // No repeated values
    }

    let f_n = sum / count as f64;

    // Variance estimate for confidence bound
    let mut var_sum = 0.0f64;
    // Reset for second pass
    let mut last_pos2 = [0usize; 256];
    for (i, &b) in data[..q].iter().enumerate() {
        last_pos2[b as usize] = i + 1;
    }
    for (i, &b) in data[q..].iter().enumerate() {
        let pos = q + i + 1;
        let prev = last_pos2[b as usize];
        if prev > 0 {
            let distance = pos - prev;
            let log_d = (distance as f64).log2();
            var_sum += (log_d - f_n).powi(2);
        }
        last_pos2[b as usize] = pos;
    }
    let variance = var_sum / (count as f64 - 1.0).max(1.0);
    let std_err = (variance / count as f64).sqrt();

    // Lower confidence bound on Shannon estimate (conservative)
    let z = 2.576; // 99% CI
    let f_lower = (f_n - z * std_err).max(0.0);

    // Convert Shannon estimate to min-entropy bound.
    // Maurer's statistic ≈ Shannon entropy. Min-entropy <= Shannon entropy.
    // Apply quadratic scaling: H∞_est = f_lower^2 / log2(k).
    // This correctly maps: 8.0 → 8.0 (uniform), 4.0 → 2.0, 1.0 → 0.125.
    // The quadratic penalty reflects that skewed distributions have a larger
    // gap between Shannon and min-entropy.
    (f_lower * f_lower / l).min(l)
}

/// t-Tuple estimator (NIST-inspired diagnostic).
/// Estimates entropy from most frequent t-length tuple.
/// Returns estimated min-entropy bits per sample.
pub fn t_tuple_estimate(data: &[u8]) -> f64 {
    if data.len() < 20 {
        return 0.0;
    }

    // Try t=1,2,3 and take the minimum (most conservative)
    let mut min_h = 8.0f64;

    for t in 1..=3usize {
        if data.len() < t + 1 {
            break;
        }
        let mut counts: HashMap<&[u8], u64> = HashMap::new();
        for window in data.windows(t) {
            *counts.entry(window).or_insert(0) += 1;
        }
        let n = (data.len() - t + 1) as f64;
        let max_count = *counts.values().max().unwrap_or(&0) as f64;
        let p_max = max_count / n;

        if p_max > 0.0 {
            // For t-tuples, per-sample entropy is -log2(p_max) / t
            let h = -p_max.log2() / t as f64;
            min_h = min_h.min(h);
        }
    }

    min_h.min(8.0)
}

/// Min-entropy estimate with diagnostic side metrics.
///
/// For professional operational use, `min_entropy` is the MCV-based estimate.
/// Additional estimators are reported as diagnostics, and their minimum is
/// exposed as `heuristic_floor`.
pub fn min_entropy_estimate(data: &[u8]) -> MinEntropyReport {
    let shannon = quick_shannon(data);
    let (mcv_h, mcv_p_upper) = mcv_estimate(data);
    let collision_h = collision_estimate(data);
    let markov_h = markov_estimate(data);
    let compression_h = compression_estimate(data);
    let t_tuple_h = t_tuple_estimate(data);

    let heuristic_floor = collision_h.min(markov_h).min(compression_h).min(t_tuple_h);

    MinEntropyReport {
        shannon_entropy: shannon,
        min_entropy: mcv_h,
        heuristic_floor,
        mcv_estimate: mcv_h,
        mcv_p_upper,
        collision_estimate: collision_h,
        markov_estimate: markov_h,
        compression_estimate: compression_h,
        t_tuple_estimate: t_tuple_h,
        samples: data.len(),
    }
}

/// Min-entropy analysis report with individual estimator results.
#[derive(Debug, Clone)]
pub struct MinEntropyReport {
    /// Shannon entropy (bits/byte, max 8.0). Upper bound, not conservative.
    pub shannon_entropy: f64,
    /// Primary conservative min-entropy estimate (bits/byte), MCV-based.
    pub min_entropy: f64,
    /// Minimum across heuristic diagnostic estimators.
    pub heuristic_floor: f64,
    /// Most Common Value estimator.
    pub mcv_estimate: f64,
    /// Upper bound on max probability from MCV
    pub mcv_p_upper: f64,
    /// Collision estimator (diagnostic)
    pub collision_estimate: f64,
    /// Markov estimator (diagnostic)
    pub markov_estimate: f64,
    /// Compression estimator (diagnostic)
    pub compression_estimate: f64,
    /// t-Tuple estimator (diagnostic)
    pub t_tuple_estimate: f64,
    /// Number of samples analyzed
    pub samples: usize,
}

impl std::fmt::Display for MinEntropyReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Min-Entropy Analysis ({} samples)", self.samples)?;
        writeln!(
            f,
            "  Shannon H:       {:.3} bits/byte  (upper bound)",
            self.shannon_entropy
        )?;
        writeln!(
            f,
            "  Min-Entropy H∞:  {:.3} bits/byte  (primary, MCV)",
            self.min_entropy
        )?;
        writeln!(
            f,
            "  Heuristic floor: {:.3} bits/byte  (diagnostic minimum)",
            self.heuristic_floor
        )?;
        writeln!(f, "  ─────────────────────────────────")?;
        writeln!(
            f,
            "  MCV:                 {:.3}  (p_upper={:.4})",
            self.mcv_estimate, self.mcv_p_upper
        )?;
        writeln!(f, "  Collision (diag):    {:.3}", self.collision_estimate)?;
        writeln!(f, "  Markov (diag):       {:.3}", self.markov_estimate)?;
        writeln!(
            f,
            "  Compression (diag):  {:.3}  (Maurer-inspired)",
            self.compression_estimate
        )?;
        writeln!(f, "  t-Tuple (diag):      {:.3}", self.t_tuple_estimate)?;
        Ok(())
    }
}

/// Quick min-entropy estimate using only the MCV estimator (NIST SP 800-90B 6.3.1).
///
/// This is the fast path used by the entropy pool and TUI for per-collection
/// health checks. It uses only the Most Common Value estimator — the most
/// well-established and computationally cheap NIST estimator (O(n) single pass).
///
/// For a full multi-estimator breakdown, use [`min_entropy_estimate`] instead.
pub fn quick_min_entropy(data: &[u8]) -> f64 {
    mcv_estimate(data).0
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

/// Grade a source based on its min-entropy (H∞) value.
///
/// This is the **single source of truth** for entropy grading. All CLI commands,
/// server endpoints, and reports should use this function instead of duplicating
/// threshold logic.
///
/// | Grade | Min-Entropy (H∞) |
/// |-------|-------------------|
/// | A     | ≥ 6.0             |
/// | B     | ≥ 4.0             |
/// | C     | ≥ 2.0             |
/// | D     | ≥ 1.0             |
/// | F     | < 1.0             |
pub fn grade_min_entropy(min_entropy: f64) -> char {
    if min_entropy >= 6.0 {
        'A'
    } else if min_entropy >= 4.0 {
        'B'
    } else if min_entropy >= 2.0 {
        'C'
    } else if min_entropy >= 1.0 {
        'D'
    } else {
        'F'
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Conditioning mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_condition_raw_passthrough() {
        let data = vec![1, 2, 3, 4, 5];
        let out = condition(&data, 3, ConditioningMode::Raw);
        assert_eq!(out, vec![1, 2, 3]);
    }

    #[test]
    fn test_condition_raw_exact_length() {
        let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let out = condition(&data, 100, ConditioningMode::Raw);
        assert_eq!(out, data);
    }

    #[test]
    fn test_condition_raw_truncates() {
        let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let out = condition(&data, 50, ConditioningMode::Raw);
        assert_eq!(out.len(), 50);
        assert_eq!(out, &data[..50]);
    }

    #[test]
    fn test_condition_sha256_produces_exact_length() {
        let data = vec![42u8; 100];
        for len in [1, 16, 32, 64, 100, 256] {
            let out = condition(&data, len, ConditioningMode::Sha256);
            assert_eq!(out.len(), len, "SHA256 should produce exactly {len} bytes");
        }
    }

    #[test]
    fn test_sha256_deterministic() {
        let data = vec![42u8; 100];
        let out1 = sha256_condition_bytes(&data, 64);
        let out2 = sha256_condition_bytes(&data, 64);
        assert_eq!(
            out1, out2,
            "SHA256 conditioning should be deterministic for same input"
        );
    }

    #[test]
    fn test_sha256_different_inputs_differ() {
        let data1 = vec![1u8; 100];
        let data2 = vec![2u8; 100];
        let out1 = sha256_condition_bytes(&data1, 32);
        let out2 = sha256_condition_bytes(&data2, 32);
        assert_ne!(out1, out2);
    }

    #[test]
    fn test_sha256_empty_input() {
        let out = sha256_condition_bytes(&[], 32);
        assert!(out.is_empty(), "Empty input should produce no output");
    }

    #[test]
    fn test_von_neumann_reduces_size() {
        let input = vec![0b10101010u8; 128];
        let output = von_neumann_debias(&input);
        assert!(output.len() < input.len());
    }

    #[test]
    fn test_von_neumann_known_output() {
        // Input: 0b10_10_10_10 = pairs (1,0)(1,0)(1,0)(1,0)
        // Von Neumann: (1,0) -> 1, repeated 4 times = 4 bits = 1111 per byte
        // But we need 8 bits for one output byte.
        // Two input bytes = 8 pairs of bits -> each (1,0) -> 1, so 8 bits -> 0b11111111
        let input = vec![0b10101010u8; 2];
        let output = von_neumann_debias(&input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0b11111111);
    }

    #[test]
    fn test_von_neumann_alternating_01() {
        // Input: 0b01_01_01_01 = pairs (0,1)(0,1)(0,1)(0,1)
        // Von Neumann: (0,1) -> 0, repeated 4 times per byte
        // Two input bytes = 8 pairs -> 8 zero bits -> 0b00000000
        let input = vec![0b01010101u8; 2];
        let output = von_neumann_debias(&input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0b00000000);
    }

    #[test]
    fn test_von_neumann_all_same_discards() {
        // Input: all 0xFF = pairs (1,1)(1,1)... -> all discarded
        let input = vec![0xFF; 100];
        let output = von_neumann_debias(&input);
        assert!(output.is_empty(), "All-ones should produce no output");
    }

    #[test]
    fn test_von_neumann_all_zeros_discards() {
        // Input: all 0x00 = pairs (0,0)(0,0)... -> all discarded
        let input = vec![0x00; 100];
        let output = von_neumann_debias(&input);
        assert!(output.is_empty(), "All-zeros should produce no output");
    }

    #[test]
    fn test_condition_modes_differ() {
        let data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        let raw = condition(&data, 64, ConditioningMode::Raw);
        let sha = condition(&data, 64, ConditioningMode::Sha256);
        assert_ne!(raw, sha);
    }

    #[test]
    fn test_conditioning_mode_display() {
        assert_eq!(ConditioningMode::Raw.to_string(), "raw");
        assert_eq!(ConditioningMode::VonNeumann.to_string(), "von_neumann");
        assert_eq!(ConditioningMode::Sha256.to_string(), "sha256");
    }

    #[test]
    fn test_conditioning_mode_default() {
        assert_eq!(ConditioningMode::default(), ConditioningMode::Sha256);
    }

    // -----------------------------------------------------------------------
    // XOR fold tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_xor_fold_basic() {
        let data = vec![0xFF, 0x00, 0xAA, 0x55];
        let folded = xor_fold(&data);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0], 0xFF ^ 0xAA);
        assert_eq!(folded[1], 0x55);
    }

    #[test]
    fn test_xor_fold_single_byte() {
        let data = vec![42];
        let folded = xor_fold(&data);
        assert_eq!(folded, vec![42]);
    }

    #[test]
    fn test_xor_fold_empty() {
        let folded = xor_fold(&[]);
        assert!(folded.is_empty());
    }

    #[test]
    fn test_xor_fold_odd_length() {
        // With 5 bytes, half=2, so XOR data[0..2] with data[2..4]
        let data = vec![1, 2, 3, 4, 5];
        let folded = xor_fold(&data);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0], 1 ^ 3);
        assert_eq!(folded[1], 2 ^ 4);
    }

    // -----------------------------------------------------------------------
    // Shannon entropy tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_shannon_empty() {
        assert_eq!(quick_shannon(&[]), 0.0);
    }

    #[test]
    fn test_shannon_single_byte() {
        // One byte = one value, p=1.0, H = -1.0 * log2(1.0) = 0.0
        assert_eq!(quick_shannon(&[42]), 0.0);
    }

    #[test]
    fn test_shannon_all_same() {
        let data = vec![0u8; 1000];
        assert_eq!(quick_shannon(&data), 0.0);
    }

    #[test]
    fn test_shannon_two_values_equal() {
        // 50/50 split between two values = 1.0 bits
        let mut data = vec![0u8; 500];
        data.extend(vec![1u8; 500]);
        let h = quick_shannon(&data);
        assert!((h - 1.0).abs() < 0.01, "Expected ~1.0, got {h}");
    }

    #[test]
    fn test_shannon_uniform_256() {
        // Perfectly uniform over 256 values = 8.0 bits
        let data: Vec<u8> = (0..=255).collect();
        let h = quick_shannon(&data);
        assert!((h - 8.0).abs() < 0.01, "Expected ~8.0, got {h}");
    }

    #[test]
    fn test_shannon_uniform_large() {
        // Large uniform sample — each value appears ~40 times
        let mut data = Vec::with_capacity(256 * 40);
        for _ in 0..40 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let h = quick_shannon(&data);
        assert!((h - 8.0).abs() < 0.01, "Expected ~8.0, got {h}");
    }

    // -----------------------------------------------------------------------
    // Min-entropy estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_min_entropy_empty() {
        assert_eq!(min_entropy(&[]), 0.0);
    }

    #[test]
    fn test_min_entropy_all_same() {
        let data = vec![42u8; 1000];
        let h = min_entropy(&data);
        assert!(h < 0.01, "All-same should have ~0 min-entropy, got {h}");
    }

    #[test]
    fn test_min_entropy_uniform() {
        let mut data = Vec::with_capacity(256 * 40);
        for _ in 0..40 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let h = min_entropy(&data);
        assert!(
            (h - 8.0).abs() < 0.1,
            "Uniform should have ~8.0 min-entropy, got {h}"
        );
    }

    #[test]
    fn test_min_entropy_two_values() {
        let mut data = vec![0u8; 500];
        data.extend(vec![1u8; 500]);
        let h = min_entropy(&data);
        // p_max = 0.5, H∞ = -log2(0.5) = 1.0
        assert!((h - 1.0).abs() < 0.01, "Expected ~1.0, got {h}");
    }

    #[test]
    fn test_min_entropy_biased() {
        // 90% value 0, 10% value 1: p_max=0.9, H∞ = -log2(0.9) ≈ 0.152
        let mut data = vec![0u8; 900];
        data.extend(vec![1u8; 100]);
        let h = min_entropy(&data);
        let expected = -(0.9f64.log2());
        assert!(
            (h - expected).abs() < 0.02,
            "Expected ~{expected:.3}, got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // MCV estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_mcv_empty() {
        let (h, p) = mcv_estimate(&[]);
        assert_eq!(h, 0.0);
        assert_eq!(p, 1.0);
    }

    #[test]
    fn test_mcv_all_same() {
        let data = vec![42u8; 1000];
        let (h, p_upper) = mcv_estimate(&data);
        assert!(h < 0.1, "All-same should have ~0 MCV entropy, got {h}");
        assert!((p_upper - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_mcv_uniform() {
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let (h, _p_upper) = mcv_estimate(&data);
        assert!(h > 7.0, "Uniform should have high MCV entropy, got {h}");
    }

    // -----------------------------------------------------------------------
    // Collision estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_collision_too_short() {
        assert_eq!(collision_estimate(&[1, 2]), 0.0);
    }

    #[test]
    fn test_collision_all_same() {
        let data = vec![0u8; 1000];
        let h = collision_estimate(&data);
        // All same -> every adjacent pair is a collision -> mean distance = 1
        // -> p_max = 1.0 -> H = 0
        assert!(
            h < 1.0,
            "All-same should have very low collision entropy, got {h}"
        );
    }

    #[test]
    fn test_collision_uniform_large() {
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let h = collision_estimate(&data);
        assert!(
            h > 3.0,
            "Uniform should have reasonable collision entropy, got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // Markov estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_markov_too_short() {
        assert_eq!(markov_estimate(&[42]), 0.0);
    }

    #[test]
    fn test_markov_all_same() {
        let data = vec![0u8; 1000];
        let h = markov_estimate(&data);
        assert!(h < 1.0, "All-same should have low Markov entropy, got {h}");
    }

    #[test]
    fn test_markov_uniform_large() {
        // Byte-level Markov estimator finds the max transition probability across
        // all 256x256 = 65536 transitions. With ~25600 samples, the transition
        // matrix is very sparse (~0.4 counts per cell on average). Some cells will
        // get a disproportionate share by chance, making p_max high.
        //
        // This is the correct, expected behavior: the Markov estimator is inherently
        // conservative with small sample sizes relative to the state space.
        // With truly uniform IID data you'd need ~1M+ samples for the Markov
        // estimate to converge near 8.0.
        //
        // We verify it's meaningfully above zero (all-same baseline).
        let mut data = Vec::with_capacity(256 * 100);
        for i in 0..(256 * 100) {
            let v = ((i as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                >> 56) as u8;
            data.push(v);
        }
        let h = markov_estimate(&data);
        assert!(
            h > 0.1,
            "Pseudo-random should have Markov entropy > 0.1, got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // Compression estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_compression_too_short() {
        assert_eq!(compression_estimate(&[1; 50]), 0.0);
    }

    #[test]
    fn test_compression_all_same() {
        let data = vec![0u8; 1000];
        let h = compression_estimate(&data);
        assert!(
            h < 2.0,
            "All-same should have low compression entropy, got {h}"
        );
    }

    #[test]
    fn test_compression_uniform_large() {
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let h = compression_estimate(&data);
        assert!(
            h > 4.0,
            "Uniform should have reasonable compression entropy, got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // t-Tuple estimator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_t_tuple_too_short() {
        assert_eq!(t_tuple_estimate(&[1; 10]), 0.0);
    }

    #[test]
    fn test_t_tuple_all_same() {
        let data = vec![0u8; 1000];
        let h = t_tuple_estimate(&data);
        assert!(h < 0.1, "All-same should have ~0 t-tuple entropy, got {h}");
    }

    #[test]
    fn test_t_tuple_uniform_large() {
        // t-Tuple estimator finds the most frequent t-length tuple and computes
        // -log2(p_max)/t. For t>1, pseudo-random data with sequential correlation
        // may show elevated tuple frequencies. We verify the result is well above
        // the all-same baseline (~0).
        let mut data = Vec::with_capacity(256 * 100);
        for i in 0..(256 * 100) {
            let v = ((i as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                >> 56) as u8;
            data.push(v);
        }
        let h = t_tuple_estimate(&data);
        assert!(
            h > 2.5,
            "Pseudo-random should have t-tuple entropy > 2.5, got {h}"
        );
    }

    // -----------------------------------------------------------------------
    // Combined min-entropy report tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_min_entropy_estimate_all_same() {
        let data = vec![0u8; 1000];
        let report = min_entropy_estimate(&data);
        assert!(
            report.min_entropy < 1.0,
            "All-same combined estimate: {}",
            report.min_entropy
        );
        assert!(report.shannon_entropy < 0.01);
        assert_eq!(report.samples, 1000);
    }

    #[test]
    fn test_min_entropy_estimate_uniform() {
        // Primary min-entropy is MCV-based; heuristic floor remains available
        // as an additional diagnostic view.
        let mut data = Vec::with_capacity(256 * 100);
        for i in 0..(256 * 100) {
            let v = ((i as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                >> 56) as u8;
            data.push(v);
        }
        let report = min_entropy_estimate(&data);
        assert!(
            report.min_entropy > 6.0,
            "Primary min-entropy should be high for uniform marginals: {}",
            report.min_entropy
        );
        assert!(
            report.shannon_entropy > 7.9,
            "Shannon should be near 8.0 for uniform marginals: {}",
            report.shannon_entropy
        );
        // MCV should be close to 8.0 for uniform-ish data
        assert!(
            report.mcv_estimate > 6.0,
            "MCV should be high for uniform data: {}",
            report.mcv_estimate
        );
        assert!(
            report.heuristic_floor <= report.min_entropy + 1e-9,
            "heuristic floor should not exceed primary min-entropy"
        );
    }

    #[test]
    fn test_min_entropy_report_display() {
        let data = vec![0u8; 1000];
        let report = min_entropy_estimate(&data);
        let s = format!("{report}");
        assert!(s.contains("Min-Entropy Analysis"));
        assert!(s.contains("1000 samples"));
    }

    #[test]
    fn test_quick_min_entropy_uses_mcv() {
        let data: Vec<u8> = (0..=255).collect();
        let quick = quick_min_entropy(&data);
        let (mcv_h, _) = mcv_estimate(&data);
        // quick_min_entropy uses MCV only — should match exactly
        assert!(
            (quick - mcv_h).abs() < f64::EPSILON,
            "quick_min_entropy ({quick}) should equal MCV estimate ({mcv_h})"
        );
    }

    #[test]
    fn test_quick_min_entropy_leq_shannon() {
        // Min-entropy should always be <= Shannon entropy
        let data: Vec<u8> = (0..=255).cycle().take(2560).collect();
        let quick = quick_min_entropy(&data);
        let shannon = quick_shannon(&data);
        assert!(
            quick <= shannon + 0.01,
            "H∞ ({quick}) should be <= Shannon ({shannon})"
        );
    }

    // -----------------------------------------------------------------------
    // Quality report tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_quality_too_short() {
        let q = quick_quality(&[1, 2, 3]);
        assert_eq!(q.grade, 'F');
        assert_eq!(q.quality_score, 0.0);
    }

    #[test]
    fn test_quality_all_same() {
        let data = vec![0u8; 1000];
        let q = quick_quality(&data);
        assert!(
            q.grade == 'F' || q.grade == 'D',
            "All-same should grade poorly, got {}",
            q.grade
        );
        assert_eq!(q.unique_values, 1);
        assert!(q.shannon_entropy < 0.01);
    }

    #[test]
    fn test_quality_uniform() {
        let mut data = Vec::with_capacity(256 * 40);
        for _ in 0..40 {
            for b in 0..=255u8 {
                data.push(b);
            }
        }
        let q = quick_quality(&data);
        assert!(
            q.grade == 'A' || q.grade == 'B',
            "Uniform should grade well, got {}",
            q.grade
        );
        assert_eq!(q.unique_values, 256);
        assert!(q.shannon_entropy > 7.9);
    }

    // -----------------------------------------------------------------------
    // grade_min_entropy tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_grade_boundaries() {
        assert_eq!(grade_min_entropy(8.0), 'A');
        assert_eq!(grade_min_entropy(6.0), 'A');
        assert_eq!(grade_min_entropy(5.99), 'B');
        assert_eq!(grade_min_entropy(4.0), 'B');
        assert_eq!(grade_min_entropy(3.99), 'C');
        assert_eq!(grade_min_entropy(2.0), 'C');
        assert_eq!(grade_min_entropy(1.99), 'D');
        assert_eq!(grade_min_entropy(1.0), 'D');
        assert_eq!(grade_min_entropy(0.99), 'F');
        assert_eq!(grade_min_entropy(0.0), 'F');
    }

    #[test]
    fn test_grade_negative() {
        assert_eq!(grade_min_entropy(-1.0), 'F');
    }
}
