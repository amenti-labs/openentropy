//! Ordinal pattern analysis for consciousness-RNG experiments.
//!
//! Permutation entropy and forbidden pattern detection provide strict randomness
//! criteria. For a truly random process of sufficient length, all L! ordinal
//! patterns of length L appear with equal probability. Forbidden patterns
//! (zero count) are impossible under true randomness and represent definitive
//! non-randomness evidence — stronger than any p-value.
//!
//! Based on Bandt & Pompe (2002) "Permutation entropy: a natural complexity
//! measure for time series."

use serde::{Deserialize, Serialize};

/// Result of ordinal pattern comparison between baseline and intention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdinalComparison {
    /// Order used (pattern length).
    pub order: usize,
    /// Total possible patterns (order!).
    pub n_patterns: usize,
    /// Baseline permutation entropy (normalized 0..1).
    pub baseline_pe: f64,
    /// Intention permutation entropy (normalized 0..1).
    pub intention_pe: f64,
    /// Baseline weighted permutation entropy.
    pub baseline_wpe: f64,
    /// Intention weighted permutation entropy.
    pub intention_wpe: f64,
    /// Forbidden patterns in baseline.
    pub baseline_forbidden: Vec<Vec<usize>>,
    /// Forbidden patterns in intention.
    pub intention_forbidden: Vec<Vec<usize>>,
    /// Chi-squared statistic comparing pattern distributions.
    pub chi_squared: f64,
    /// Degrees of freedom for chi-squared test.
    pub df: usize,
    /// Approximate p-value for chi-squared test.
    pub chi_squared_p: f64,
    /// Interpretation.
    pub interpretation: String,
}

/// Compute the ordinal rank pattern of a window of values.
///
/// Returns a permutation vector where pattern[i] is the rank of window[i].
/// E.g., [30, 10, 20] -> [2, 0, 1] (10 is rank 0, 20 is rank 1, 30 is rank 2).
pub fn ordinal_pattern(window: &[u8]) -> Vec<usize> {
    let n = window.len();
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| window[a].cmp(&window[b]).then(a.cmp(&b)));

    let mut pattern = vec![0usize; n];
    for (rank, &idx) in indices.iter().enumerate() {
        pattern[idx] = rank;
    }
    pattern
}

/// Map an ordinal pattern to a unique index in 0..L!
///
/// Uses the factorial number system (Lehmer code).
pub fn pattern_to_index(pattern: &[usize]) -> usize {
    let n = pattern.len();
    let mut index = 0;
    let mut used = vec![false; n];

    for i in 0..n {
        // Count how many unused values are less than pattern[i]
        let mut count = 0;
        for j in 0..pattern[i] {
            if !used[j] {
                count += 1;
            }
        }
        index = index * (n - i) + count;
        used[pattern[i]] = true;
    }
    index
}

/// Compute factorial (for small values).
fn factorial(n: usize) -> usize {
    (1..=n).product()
}

/// Count all ordinal patterns of given order in the data.
///
/// Returns a histogram with `order!` bins, one per possible pattern.
pub fn ordinal_distribution(data: &[u8], order: usize) -> Vec<usize> {
    let n_patterns = factorial(order);
    let mut counts = vec![0usize; n_patterns];

    if data.len() < order {
        return counts;
    }

    for i in 0..=(data.len() - order) {
        let window = &data[i..i + order];
        let pattern = ordinal_pattern(window);
        let idx = pattern_to_index(&pattern);
        if idx < n_patterns {
            counts[idx] += 1;
        }
    }

    counts
}

/// Compute normalized permutation entropy (Bandt & Pompe).
///
/// PE = -sum(p_i * log2(p_i)) / log2(L!)
///
/// Returns value in [0, 1]: 0 = fully deterministic, 1 = maximally random.
pub fn permutation_entropy(data: &[u8], order: usize) -> f64 {
    let counts = ordinal_distribution(data, order);
    let total: usize = counts.iter().sum();

    if total == 0 {
        return 0.0;
    }

    let total_f = total as f64;
    let mut entropy = 0.0;

    for &c in &counts {
        if c > 0 {
            let p = c as f64 / total_f;
            entropy -= p * p.log2();
        }
    }

    let max_entropy = (factorial(order) as f64).log2();
    if max_entropy > 0.0 {
        entropy / max_entropy
    } else {
        0.0
    }
}

/// Find forbidden patterns (patterns with zero count).
///
/// For truly random data of sufficient length, there should be no forbidden
/// patterns. Their presence is definitive evidence of non-randomness.
pub fn forbidden_patterns(data: &[u8], order: usize) -> Vec<Vec<usize>> {
    let counts = ordinal_distribution(data, order);
    let n_patterns = factorial(order);
    let mut forbidden = Vec::new();

    // Only report forbidden patterns if we have enough data
    // Need at least 5 * n_patterns windows for statistical significance
    let total: usize = counts.iter().sum();
    if total < 5 * n_patterns {
        return forbidden; // Not enough data to conclude
    }

    for idx in 0..n_patterns {
        if counts[idx] == 0 {
            // Reconstruct pattern from index
            forbidden.push(index_to_pattern(idx, order));
        }
    }

    forbidden
}

/// Reconstruct ordinal pattern from its index (inverse of pattern_to_index).
fn index_to_pattern(mut index: usize, order: usize) -> Vec<usize> {
    let mut available: Vec<usize> = (0..order).collect();
    let mut pattern = Vec::with_capacity(order);

    for i in 0..order {
        let remaining = order - i;
        let fact = if remaining > 1 {
            factorial(remaining - 1)
        } else {
            1
        };
        let pos = index / fact;
        index %= fact;
        if pos < available.len() {
            pattern.push(available.remove(pos));
        } else {
            pattern.push(available.pop().unwrap_or(0));
        }
    }

    pattern
}

/// Compute weighted permutation entropy (amplitude-weighted variant).
///
/// Weights each pattern by the variance of the window values, giving more
/// weight to windows with larger amplitude fluctuations.
pub fn weighted_permutation_entropy(data: &[u8], order: usize) -> f64 {
    if data.len() < order {
        return 0.0;
    }

    let n_patterns = factorial(order);
    let mut weighted_counts = vec![0.0f64; n_patterns];
    let mut total_weight = 0.0;

    for i in 0..=(data.len() - order) {
        let window = &data[i..i + order];
        let pattern = ordinal_pattern(window);
        let idx = pattern_to_index(&pattern);

        // Weight = variance of window values
        let mean = window.iter().map(|&b| b as f64).sum::<f64>() / order as f64;
        let var = window
            .iter()
            .map(|&b| (b as f64 - mean).powi(2))
            .sum::<f64>()
            / order as f64;
        let weight = var.max(1e-10); // Avoid zero weights

        if idx < n_patterns {
            weighted_counts[idx] += weight;
            total_weight += weight;
        }
    }

    if total_weight < 1e-15 {
        return 0.0;
    }

    let mut entropy = 0.0;
    for &w in &weighted_counts {
        if w > 0.0 {
            let p = w / total_weight;
            entropy -= p * p.log2();
        }
    }

    let max_entropy = (n_patterns as f64).log2();
    if max_entropy > 0.0 {
        entropy / max_entropy
    } else {
        0.0
    }
}

/// Compare ordinal pattern distributions between baseline and intention data.
///
/// Uses chi-squared test to compare the two pattern distributions.
pub fn compare_ordinal(baseline: &[u8], intention: &[u8], order: usize) -> OrdinalComparison {
    let baseline_counts = ordinal_distribution(baseline, order);
    let intention_counts = ordinal_distribution(intention, order);
    let n_patterns = factorial(order);

    let baseline_pe = permutation_entropy(baseline, order);
    let intention_pe = permutation_entropy(intention, order);
    let baseline_wpe = weighted_permutation_entropy(baseline, order);
    let intention_wpe = weighted_permutation_entropy(intention, order);
    let baseline_forbidden_pats = forbidden_patterns(baseline, order);
    let intention_forbidden_pats = forbidden_patterns(intention, order);

    // Chi-squared test comparing the two distributions
    let total_bl: f64 = baseline_counts.iter().sum::<usize>() as f64;
    let total_int: f64 = intention_counts.iter().sum::<usize>() as f64;

    let mut chi_sq = 0.0;
    let mut df = 0usize;

    if total_bl > 0.0 && total_int > 0.0 {
        for i in 0..n_patterns {
            let o_bl = baseline_counts[i] as f64;
            let o_int = intention_counts[i] as f64;
            let total = o_bl + o_int;
            if total > 0.0 {
                let e_bl = total * total_bl / (total_bl + total_int);
                let e_int = total * total_int / (total_bl + total_int);
                if e_bl > 0.0 {
                    chi_sq += (o_bl - e_bl).powi(2) / e_bl;
                }
                if e_int > 0.0 {
                    chi_sq += (o_int - e_int).powi(2) / e_int;
                }
                df += 1;
            }
        }
        df = df.saturating_sub(1); // chi-squared df = categories - 1
    }

    // Approximate chi-squared p-value using Wilson-Hilferty normal approximation
    let chi_squared_p = chi_squared_approx_p(chi_sq, df);

    let interpretation = if chi_squared_p < 0.01 {
        "Strong distributional difference in ordinal patterns between conditions".to_string()
    } else if chi_squared_p < 0.05 {
        "Significant distributional difference in ordinal patterns".to_string()
    } else if !intention_forbidden_pats.is_empty() && baseline_forbidden_pats.is_empty() {
        format!(
            "Intention data has {} forbidden pattern(s) not present in baseline — structural anomaly",
            intention_forbidden_pats.len()
        )
    } else {
        "No significant difference in ordinal pattern distributions".to_string()
    };

    OrdinalComparison {
        order,
        n_patterns,
        baseline_pe,
        intention_pe,
        baseline_wpe,
        intention_wpe,
        baseline_forbidden: baseline_forbidden_pats,
        intention_forbidden: intention_forbidden_pats,
        chi_squared: chi_sq,
        df,
        chi_squared_p,
        interpretation,
    }
}

/// Approximate chi-squared p-value using Wilson-Hilferty transformation.
fn chi_squared_approx_p(chi_sq: f64, df: usize) -> f64 {
    if df == 0 {
        return 1.0;
    }
    let k = df as f64;
    // Wilson-Hilferty: ((chi2/k)^(1/3) - (1 - 2/(9k))) / sqrt(2/(9k)) ~ N(0,1)
    let term = (chi_sq / k).powf(1.0 / 3.0);
    let mu = 1.0 - 2.0 / (9.0 * k);
    let sigma = (2.0 / (9.0 * k)).sqrt();
    if sigma < 1e-15 {
        return 1.0;
    }
    let z = (term - mu) / sigma;
    crate::consciousness::z_to_p_one_tailed(z)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_pattern_basic() {
        // [30, 10, 20] -> ranks: 10=0, 20=1, 30=2 -> pattern [2, 0, 1]
        let p = ordinal_pattern(&[30, 10, 20]);
        assert_eq!(p, vec![2, 0, 1]);
    }

    #[test]
    fn ordinal_pattern_ascending() {
        let p = ordinal_pattern(&[1, 2, 3]);
        assert_eq!(p, vec![0, 1, 2]);
    }

    #[test]
    fn ordinal_pattern_descending() {
        let p = ordinal_pattern(&[3, 2, 1]);
        assert_eq!(p, vec![2, 1, 0]);
    }

    #[test]
    fn pattern_to_index_identity() {
        // Pattern [0, 1, 2] = first pattern = index 0
        assert_eq!(pattern_to_index(&[0, 1, 2]), 0);
        // Pattern [2, 1, 0] = last pattern = index 5 (3! - 1)
        assert_eq!(pattern_to_index(&[2, 1, 0]), 5);
    }

    #[test]
    fn index_to_pattern_roundtrip() {
        for order in 2..=4 {
            let n = factorial(order);
            for idx in 0..n {
                let pat = index_to_pattern(idx, order);
                let recovered = pattern_to_index(&pat);
                assert_eq!(recovered, idx, "order={order}, idx={idx}, pat={pat:?}");
            }
        }
    }

    #[test]
    fn ordinal_distribution_counts() {
        // For order 2: patterns [0,1] (ascending) and [1,0] (descending)
        // Data [1, 2, 3, 4] -> windows: [1,2], [2,3], [3,4] all ascending
        let counts = ordinal_distribution(&[1, 2, 3, 4], 2);
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0], 3); // [0,1] ascending
        assert_eq!(counts[1], 0); // [1,0] descending
    }

    #[test]
    fn permutation_entropy_deterministic() {
        // Monotone sequence: only one pattern type -> PE = 0
        let data: Vec<u8> = (0..100).collect();
        let pe = permutation_entropy(&data, 3);
        assert!(pe < 0.1, "PE = {pe}");
    }

    #[test]
    fn permutation_entropy_random_like() {
        // Pseudo-random data should have moderate-to-high PE
        let data: Vec<u8> = (0..1000).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let pe = permutation_entropy(&data, 3);
        assert!(pe > 0.3, "PE = {pe}");
    }

    #[test]
    fn forbidden_patterns_monotone() {
        // Strictly increasing: only pattern [0,1,2,...] present, all others forbidden
        let data: Vec<u8> = (0..200).collect();
        let forbidden = forbidden_patterns(&data, 3);
        // Should have 5 forbidden patterns (3! - 1 = 5)
        assert_eq!(forbidden.len(), 5);
    }

    #[test]
    fn forbidden_patterns_insufficient_data() {
        // Not enough data -> no forbidden patterns reported
        let data = vec![1u8, 2, 3];
        let forbidden = forbidden_patterns(&data, 3);
        assert!(forbidden.is_empty());
    }

    #[test]
    fn weighted_pe_basic() {
        let data: Vec<u8> = (0..500).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let wpe = weighted_permutation_entropy(&data, 3);
        assert!(wpe > 0.0 && wpe <= 1.0, "WPE = {wpe}");
    }

    #[test]
    fn compare_ordinal_same_data() {
        let data: Vec<u8> = (0..500).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let result = compare_ordinal(&data, &data, 3);
        assert_eq!(result.order, 3);
        assert_eq!(result.n_patterns, 6);
        // Same data should have high p-value (no difference)
        assert!(result.chi_squared_p > 0.01 || result.chi_squared < 0.001);
    }

    #[test]
    fn compare_ordinal_different_data() {
        let baseline: Vec<u8> = (0..500).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = (0..500).map(|i: u32| (i % 256) as u8).collect(); // monotone-ish
        let result = compare_ordinal(&baseline, &intention, 3);
        assert!(result.baseline_pe > result.intention_pe);
    }
}
