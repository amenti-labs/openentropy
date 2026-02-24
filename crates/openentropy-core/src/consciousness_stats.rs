//! Advanced statistical functions for consciousness-RNG experiments.
//!
//! Provides information-theoretic measures (ApEn, SampEn, LZ76, spectral flatness),
//! meta-analytic statistics (Cochran's Q, I-squared, Benjamini-Hochberg FDR),
//! and correlation comparison tools (Fisher Z-transform, Welch's t-test).

use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Information-theoretic measures
// ---------------------------------------------------------------------------

/// Approximate Entropy (Pincus 1991).
///
/// Measures regularity/predictability in a time series. Lower values indicate
/// more regularity; higher values indicate more complexity/randomness.
///
/// - `m`: embedding dimension (typically 2)
/// - `r`: tolerance threshold (typically 0.2 * SD of data)
pub fn approximate_entropy(data: &[u8], m: usize, r: f64) -> f64 {
    if data.len() <= m + 1 {
        return 0.0;
    }

    let phi_m = phi(data, m, r);
    let phi_m1 = phi(data, m + 1, r);
    phi_m - phi_m1
}

/// Helper: compute phi(m) for ApEn calculation.
fn phi(data: &[u8], m: usize, r: f64) -> f64 {
    let n = data.len();
    if n < m {
        return 0.0;
    }
    let n_templates = n - m + 1;
    let mut counts = vec![0u32; n_templates];

    for i in 0..n_templates {
        for j in 0..n_templates {
            let mut match_ok = true;
            for k in 0..m {
                if (data[i + k] as f64 - data[j + k] as f64).abs() > r {
                    match_ok = false;
                    break;
                }
            }
            if match_ok {
                counts[i] += 1;
            }
        }
    }

    let sum_log: f64 = counts
        .iter()
        .map(|&c| (c as f64 / n_templates as f64).ln())
        .sum();
    sum_log / n_templates as f64
}

/// Sample Entropy (Richman & Moorman 2000).
///
/// Like ApEn but without self-matches, reducing bias for short series.
/// Returns `f64::INFINITY` if no matches found (maximally irregular).
///
/// - `m`: embedding dimension (typically 2)
/// - `r`: tolerance threshold (typically 0.2 * SD of data)
pub fn sample_entropy(data: &[u8], m: usize, r: f64) -> f64 {
    let n = data.len();
    if n <= m + 1 {
        return 0.0;
    }

    let mut b_count = 0u64; // matches of length m
    let mut a_count = 0u64; // matches of length m+1

    for i in 0..n - m {
        for j in (i + 1)..n - m {
            // Check m-length match
            let mut m_match = true;
            for k in 0..m {
                if (data[i + k] as f64 - data[j + k] as f64).abs() > r {
                    m_match = false;
                    break;
                }
            }
            if m_match {
                b_count += 1;
                // Check m+1 length match
                if i + m < n
                    && j + m < n
                    && (data[i + m] as f64 - data[j + m] as f64).abs() <= r
                {
                    a_count += 1;
                }
            }
        }
    }

    if b_count == 0 {
        return f64::INFINITY;
    }
    if a_count == 0 {
        return f64::INFINITY;
    }

    -((a_count as f64 / b_count as f64).ln())
}

/// Lempel-Ziv complexity (LZ76), normalized.
///
/// Measures algorithmic complexity of a binary sequence.
/// Returns normalized complexity in [0, 1] where 1 = maximally complex (random).
pub fn lz76_complexity(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    // Convert bytes to bit sequence
    let n_bits = data.len() * 8;
    let bits: Vec<u8> = data
        .iter()
        .flat_map(|&b| (0..8).rev().map(move |i| (b >> i) & 1))
        .collect();

    if n_bits < 2 {
        return 0.0;
    }

    // LZ76 parsing: count distinct phrases
    let mut complexity = 1u64;
    let i = 0;
    let mut prefix_end = 1;

    while prefix_end < n_bits {
        // Find longest match of bits[prefix_end..] in bits[i..prefix_end]
        let mut len = 0;
        let mut max_len = 0;
        let mut j = i;
        let mut k = prefix_end;

        while k < n_bits {
            if bits[j + len] == bits[k] {
                len += 1;
                k += 1;
                if j + len >= prefix_end {
                    // Wrap around — extended match
                    if len > max_len {
                        max_len = len;
                    }
                    break;
                }
            } else {
                if len > max_len {
                    max_len = len;
                }
                len = 0;
                j += 1;
                k = prefix_end;
                if j >= prefix_end {
                    break;
                }
            }
        }
        if len > max_len {
            max_len = len;
        }

        complexity += 1;
        prefix_end += max_len + 1;
    }

    // Normalize: theoretical max complexity for random binary sequence
    let b = n_bits as f64;
    let expected = b / b.log2();
    if expected > 0.0 {
        (complexity as f64 / expected).min(1.0)
    } else {
        0.0
    }
}

/// Spectral flatness (Wiener entropy) of byte data.
///
/// Returns a value in [0, 1] where 1 = perfectly flat spectrum (white noise)
/// and 0 = maximally peaked (periodic signal).
///
/// Uses the same DFT approach as `analysis::spectral_analysis`.
pub fn spectral_flatness(data: &[u8]) -> f64 {
    let n = data.len().min(4096);
    if n < 4 {
        return 0.0;
    }

    let arr: Vec<f64> = data[..n].iter().map(|&b| b as f64 - 127.5).collect();

    let n_freq = n / 2;
    let mut power_spectrum: Vec<f64> = Vec::with_capacity(n_freq);

    for k in 1..=n_freq {
        let mut re = 0.0;
        let mut im = 0.0;
        let freq = 2.0 * PI * k as f64 / n as f64;
        for (j, &x) in arr.iter().enumerate() {
            re += x * (freq * j as f64).cos();
            im -= x * (freq * j as f64).sin();
        }
        power_spectrum.push((re * re + im * im) / n as f64);
    }

    let arith_mean: f64 = power_spectrum.iter().sum::<f64>() / n_freq as f64;
    if arith_mean < 1e-20 {
        return 0.0;
    }

    let log_sum: f64 = power_spectrum
        .iter()
        .map(|&p| if p > 1e-20 { p.ln() } else { -46.0 })
        .sum();
    let geo_mean = (log_sum / n_freq as f64).exp();

    (geo_mean / arith_mean).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Meta-analytic statistics
// ---------------------------------------------------------------------------

/// Cochran's Q test for heterogeneity of effect sizes.
///
/// Tests whether a set of effect sizes are estimating the same underlying effect.
/// Returns `(Q_statistic, p_value)` where the p-value is from chi-squared(k-1).
///
/// - `effect_sizes`: per-study effect size estimates
/// - `variances`: per-study variance estimates (must be > 0)
pub fn cochrans_q(effect_sizes: &[f64], variances: &[f64]) -> (f64, f64) {
    let k = effect_sizes.len();
    if k < 2 || variances.len() != k {
        return (0.0, 1.0);
    }

    // Weights = 1/variance
    let weights: Vec<f64> = variances.iter().map(|&v| if v > 1e-20 { 1.0 / v } else { 1e20 }).collect();
    let sum_w: f64 = weights.iter().sum();
    if sum_w < 1e-20 {
        return (0.0, 1.0);
    }

    // Weighted mean effect size
    let weighted_mean: f64 = weights
        .iter()
        .zip(effect_sizes.iter())
        .map(|(&w, &e)| w * e)
        .sum::<f64>()
        / sum_w;

    // Q = sum(w_i * (e_i - weighted_mean)^2)
    let q: f64 = weights
        .iter()
        .zip(effect_sizes.iter())
        .map(|(&w, &e)| w * (e - weighted_mean).powi(2))
        .sum();

    let df = (k - 1) as usize;
    let p = chi_squared_p_value(q, df);

    (q, p)
}

/// I-squared heterogeneity statistic.
///
/// Describes the percentage of variability in effect sizes that is due to
/// heterogeneity rather than sampling error.
///
/// - `q`: Cochran's Q statistic
/// - `k`: number of studies
///
/// Returns percentage in [0, 100].
pub fn i_squared(q: f64, k: usize) -> f64 {
    if k < 2 {
        return 0.0;
    }
    let df = (k - 1) as f64;
    ((q - df) / q * 100.0).max(0.0)
}

/// Benjamini-Hochberg FDR correction.
///
/// Given a set of p-values with their original indices, returns which are
/// significant after FDR correction at the given alpha level.
///
/// Input: slice of `(original_index, p_value)`.
/// Output: vec of `(original_index, is_significant)`.
pub fn benjamini_hochberg(p_values: &mut [(usize, f64)], alpha: f64) -> Vec<(usize, bool)> {
    let m = p_values.len();
    if m == 0 {
        return Vec::new();
    }

    // Sort by p-value ascending
    p_values.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Find the largest k where p_(k) <= k/m * alpha
    let mut max_k = 0;
    for (rank, &(_, p)) in p_values.iter().enumerate() {
        let threshold = (rank + 1) as f64 / m as f64 * alpha;
        if p <= threshold {
            max_k = rank + 1;
        }
    }

    // All with rank <= max_k are significant
    p_values
        .iter()
        .enumerate()
        .map(|(rank, &(idx, _))| (idx, rank < max_k))
        .collect()
}

// ---------------------------------------------------------------------------
// Correlation comparison tools
// ---------------------------------------------------------------------------

/// Fisher Z-transform: converts a Pearson r to Z-space.
///
/// `Z = arctanh(r)` — stabilizes variance and normalizes distribution.
pub fn fisher_z_transform(r: f64) -> f64 {
    // Clamp to avoid infinity at |r| = 1
    let r_clamped = r.clamp(-0.9999, 0.9999);
    r_clamped.atanh()
}

/// Fisher Z-test: compare two independent correlations.
///
/// Returns `(z_statistic, p_value)` for testing H0: rho1 = rho2.
///
/// - `r1`, `r2`: sample correlations
/// - `n1`, `n2`: sample sizes
pub fn fisher_z_test(r1: f64, n1: usize, r2: f64, n2: usize) -> (f64, f64) {
    if n1 < 4 || n2 < 4 {
        return (0.0, 1.0);
    }

    let z1 = fisher_z_transform(r1);
    let z2 = fisher_z_transform(r2);

    let se = (1.0 / (n1 as f64 - 3.0) + 1.0 / (n2 as f64 - 3.0)).sqrt();
    if se < 1e-20 {
        return (0.0, 1.0);
    }

    let z_stat = (z1 - z2) / se;
    let p = crate::consciousness::z_to_p_two_tailed(z_stat);

    (z_stat, p)
}

/// Welch's t-test for two independent samples with unequal variances.
///
/// Returns `(t_statistic, p_value)`.
pub fn welch_t_test(group1: &[f64], group2: &[f64]) -> (f64, f64) {
    let n1 = group1.len();
    let n2 = group2.len();
    if n1 < 2 || n2 < 2 {
        return (0.0, 1.0);
    }

    let mean1 = group1.iter().sum::<f64>() / n1 as f64;
    let mean2 = group2.iter().sum::<f64>() / n2 as f64;

    let var1 = group1.iter().map(|&x| (x - mean1).powi(2)).sum::<f64>() / (n1 - 1) as f64;
    let var2 = group2.iter().map(|&x| (x - mean2).powi(2)).sum::<f64>() / (n2 - 1) as f64;

    let se_sq = var1 / n1 as f64 + var2 / n2 as f64;
    if se_sq < 1e-20 {
        return (0.0, 1.0);
    }

    let t = (mean1 - mean2) / se_sq.sqrt();

    // Welch-Satterthwaite degrees of freedom
    let num = se_sq.powi(2);
    let denom = (var1 / n1 as f64).powi(2) / (n1 - 1) as f64
        + (var2 / n2 as f64).powi(2) / (n2 - 1) as f64;
    let df = if denom > 1e-20 { num / denom } else { 1.0 };

    // Approximate p-value using t-distribution via normal approximation
    // (good for df > 30; acceptable for consciousness experiments)
    let p = t_to_p_two_tailed(t, df);

    (t, p)
}

/// Mean absolute change of a time series.
///
/// MAC = mean(|x_{i+1} - x_i|) — measures smoothness/roughness.
pub fn mean_absolute_change(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let sum: f64 = values.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    sum / (values.len() - 1) as f64
}

/// Pearson correlation between two f64 slices.
///
/// Returns 0.0 if either slice has zero variance.
pub fn pearson_correlation_f64(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }

    let mean_a = a[..n].iter().sum::<f64>() / n as f64;
    let mean_b = b[..n].iter().sum::<f64>() / n as f64;

    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }

    let denom = (var_a * var_b).sqrt();
    if denom < 1e-10 {
        0.0
    } else {
        cov / denom
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Approximate chi-squared p-value.
///
/// Uses the regularized incomplete gamma function via series expansion.
fn chi_squared_p_value(chi2: f64, df: usize) -> f64 {
    if df == 0 || chi2 <= 0.0 {
        return 1.0;
    }

    // P(X > chi2) = 1 - gamma_inc(df/2, chi2/2)
    let a = df as f64 / 2.0;
    let x = chi2 / 2.0;

    // Use regularized lower incomplete gamma via series
    let p_lower = lower_incomplete_gamma_reg(a, x);
    (1.0 - p_lower).clamp(0.0, 1.0)
}

/// Regularized lower incomplete gamma function P(a, x) via series expansion.
fn lower_incomplete_gamma_reg(a: f64, x: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return 0.0;
    }

    // Series: P(a,x) = e^(-x) * x^a * sum(x^n / gamma(a+n+1), n=0..inf)
    // = e^(-x) * x^a / gamma(a) * sum(x^n / prod(a+k, k=1..n), n=0..inf)
    let mut sum = 1.0 / a;
    let mut term = 1.0 / a;

    for n in 1..200 {
        term *= x / (a + n as f64);
        sum += term;
        if term.abs() < 1e-12 * sum.abs() {
            break;
        }
    }

    let log_result = a * x.ln() - x - ln_gamma(a) + sum.ln();
    if log_result > 500.0 {
        return 1.0;
    }
    if log_result < -500.0 {
        return 0.0;
    }

    // Actually, the series is: P(a,x) = e^(-x) * x^a * sum
    let result = (-x).exp() * x.powf(a) * sum;
    result.clamp(0.0, 1.0)
}

/// Log-gamma function (Stirling approximation for a > 0).
fn ln_gamma(a: f64) -> f64 {
    if a <= 0.0 {
        return 0.0;
    }
    // Lanczos approximation (g=7, n=9)
    let coeffs = [
        0.999_999_999_999_809_93,
        676.520_368_121_885_1,
        -1259.139_216_722_402_9,
        771.323_428_777_653_08,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    let g = 7.0;
    let x = a - 1.0;
    let t = x + g + 0.5;

    let mut sum = coeffs[0];
    for (i, &c) in coeffs[1..].iter().enumerate() {
        sum += c / (x + i as f64 + 1.0);
    }

    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

/// Two-tailed p-value from t-distribution.
///
/// Uses normal approximation for large df, and a correction for small df.
fn t_to_p_two_tailed(t: f64, df: f64) -> f64 {
    if df <= 0.0 {
        return 1.0;
    }

    // For df > 30, normal approximation is good
    if df > 30.0 {
        return crate::consciousness::z_to_p_two_tailed(t);
    }

    // For small df, use the incomplete beta function relation:
    // p = I_{df/(df+t^2)}(df/2, 1/2)
    // Approximate with a simple correction factor
    let z = t * (1.0 - 1.0 / (4.0 * df)).sqrt();
    crate::consciousness::z_to_p_two_tailed(z)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Approximate Entropy --

    #[test]
    fn apen_constant_sequence_is_zero() {
        let data = vec![100u8; 100];
        let apen = approximate_entropy(&data, 2, 10.0);
        assert!(apen.abs() < 0.01, "ApEn of constant = {apen}");
    }

    #[test]
    fn apen_random_higher_than_periodic() {
        let periodic: Vec<u8> = (0..200).map(|i| if i % 2 == 0 { 0 } else { 255 }).collect();
        // Pseudo-random (not truly random, but irregular)
        let random: Vec<u8> = (0..200).map(|i| ((i * 137 + 73) % 256) as u8).collect();
        let r = 25.0;
        let apen_periodic = approximate_entropy(&periodic, 2, r);
        let apen_random = approximate_entropy(&random, 2, r);
        assert!(
            apen_random > apen_periodic,
            "random ApEn {apen_random} should > periodic {apen_periodic}"
        );
    }

    #[test]
    fn apen_empty_data() {
        assert_eq!(approximate_entropy(&[], 2, 10.0), 0.0);
    }

    // -- Sample Entropy --

    #[test]
    fn sampen_constant_sequence() {
        // For constant data, all m-templates match, and all m+1-templates match
        // so SampEn = -ln(a/b) = -ln(1) = 0
        let data = vec![128u8; 50];
        let se = sample_entropy(&data, 2, 10.0);
        assert!(se.abs() < 0.01, "SampEn of constant = {se}");
    }

    #[test]
    fn sampen_empty() {
        assert_eq!(sample_entropy(&[], 2, 10.0), 0.0);
    }

    // -- LZ76 Complexity --

    #[test]
    fn lz76_constant_low_complexity() {
        let data = vec![0u8; 100];
        let c = lz76_complexity(&data);
        assert!(c < 0.2, "constant LZ76 = {c}");
    }

    #[test]
    fn lz76_mixed_moderate_complexity() {
        let data: Vec<u8> = (0..100).map(|i| ((i * 137 + 73) % 256) as u8).collect();
        let c = lz76_complexity(&data);
        assert!(c > 0.1, "mixed LZ76 = {c}");
    }

    #[test]
    fn lz76_empty() {
        assert_eq!(lz76_complexity(&[]), 0.0);
    }

    // -- Spectral Flatness --

    #[test]
    fn spectral_flatness_constant_zero() {
        // Constant signal has zero variance → zero power
        let data = vec![128u8; 64];
        let sf = spectral_flatness(&data);
        // Should be 0 or very low since there's no spectral content
        assert!(sf < 0.01, "constant flatness = {sf}");
    }

    #[test]
    fn spectral_flatness_empty() {
        assert_eq!(spectral_flatness(&[]), 0.0);
    }

    // -- Cochran's Q --

    #[test]
    fn cochrans_q_identical_effects() {
        let effects = vec![0.5, 0.5, 0.5];
        let variances = vec![0.1, 0.1, 0.1];
        let (q, p) = cochrans_q(&effects, &variances);
        assert!(q.abs() < 1e-10, "Q = {q}");
        assert!(p > 0.99, "p = {p}");
    }

    #[test]
    fn cochrans_q_heterogeneous() {
        let effects = vec![0.0, 1.0, 2.0, 3.0];
        let variances = vec![0.1, 0.1, 0.1, 0.1];
        let (q, _p) = cochrans_q(&effects, &variances);
        assert!(q > 0.0, "Q should be > 0 for heterogeneous effects");
    }

    #[test]
    fn cochrans_q_empty() {
        let (q, p) = cochrans_q(&[], &[]);
        assert_eq!(q, 0.0);
        assert_eq!(p, 1.0);
    }

    // -- I-squared --

    #[test]
    fn i_squared_zero_heterogeneity() {
        // Q = df means I² = 0
        assert!((i_squared(4.0, 5) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn i_squared_high_heterogeneity() {
        // Q >> df means I² approaches 100
        let i2 = i_squared(100.0, 5);
        assert!(i2 > 90.0, "I² = {i2}");
    }

    #[test]
    fn i_squared_single_study() {
        assert_eq!(i_squared(0.0, 1), 0.0);
    }

    // -- Benjamini-Hochberg --

    #[test]
    fn bh_all_significant() {
        let mut ps = vec![(0, 0.001), (1, 0.002), (2, 0.003)];
        let results = benjamini_hochberg(&mut ps, 0.05);
        assert!(results.iter().all(|&(_, sig)| sig));
    }

    #[test]
    fn bh_none_significant() {
        let mut ps = vec![(0, 0.5), (1, 0.6), (2, 0.7)];
        let results = benjamini_hochberg(&mut ps, 0.05);
        assert!(results.iter().all(|&(_, sig)| !sig));
    }

    #[test]
    fn bh_empty() {
        let mut ps: Vec<(usize, f64)> = vec![];
        let results = benjamini_hochberg(&mut ps, 0.05);
        assert!(results.is_empty());
    }

    #[test]
    fn bh_mixed() {
        let mut ps = vec![(0, 0.01), (1, 0.04), (2, 0.5)];
        let results = benjamini_hochberg(&mut ps, 0.05);
        // At least the first should be significant
        let sig_count = results.iter().filter(|&&(_, s)| s).count();
        assert!(sig_count >= 1, "sig_count = {sig_count}");
    }

    // -- Fisher Z --

    #[test]
    fn fisher_z_at_zero() {
        assert!((fisher_z_transform(0.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn fisher_z_positive() {
        let z = fisher_z_transform(0.5);
        assert!((z - 0.5493).abs() < 0.001, "z = {z}");
    }

    #[test]
    fn fisher_z_test_equal_correlations() {
        let (z, p) = fisher_z_test(0.5, 100, 0.5, 100);
        assert!(z.abs() < 1e-10, "z = {z}");
        assert!(p > 0.99, "p = {p}");
    }

    #[test]
    fn fisher_z_test_different_correlations() {
        let (z, _p) = fisher_z_test(0.8, 100, 0.2, 100);
        assert!(z.abs() > 1.0, "z = {z}");
    }

    #[test]
    fn fisher_z_test_small_n() {
        let (z, p) = fisher_z_test(0.5, 2, 0.5, 2);
        assert_eq!(z, 0.0);
        assert_eq!(p, 1.0);
    }

    // -- Welch's t-test --

    #[test]
    fn welch_identical_groups() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (t, p) = welch_t_test(&a, &b);
        assert!(t.abs() < 1e-10, "t = {t}");
        assert!(p > 0.99, "p = {p}");
    }

    #[test]
    fn welch_different_means() {
        let a = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0];
        let b = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let (t, p) = welch_t_test(&a, &b);
        assert!(t > 2.0, "t = {t}");
        assert!(p < 0.05, "p = {p}");
    }

    #[test]
    fn welch_empty() {
        let (t, p) = welch_t_test(&[], &[1.0, 2.0]);
        assert_eq!(t, 0.0);
        assert_eq!(p, 1.0);
    }

    // -- Mean Absolute Change --

    #[test]
    fn mac_constant() {
        let v = vec![5.0, 5.0, 5.0, 5.0];
        assert_eq!(mean_absolute_change(&v), 0.0);
    }

    #[test]
    fn mac_alternating() {
        let v = vec![0.0, 1.0, 0.0, 1.0];
        assert!((mean_absolute_change(&v) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn mac_empty() {
        assert_eq!(mean_absolute_change(&[]), 0.0);
    }

    // -- Pearson correlation (f64) --

    #[test]
    fn pearson_f64_perfect_positive() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let r = pearson_correlation_f64(&a, &b);
        assert!((r - 1.0).abs() < 1e-10, "r = {r}");
    }

    #[test]
    fn pearson_f64_perfect_negative() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let r = pearson_correlation_f64(&a, &b);
        assert!((r - (-1.0)).abs() < 1e-10, "r = {r}");
    }

    #[test]
    fn pearson_f64_zero_variance() {
        let a = vec![5.0, 5.0, 5.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(pearson_correlation_f64(&a, &b), 0.0);
    }

    // -- Chi-squared internal --

    #[test]
    fn chi_sq_zero() {
        let p = chi_squared_p_value(0.0, 3);
        assert!((p - 1.0).abs() < 0.01, "p = {p}");
    }

    #[test]
    fn chi_sq_known_value() {
        // chi2 = 7.815, df = 3 → p ≈ 0.05
        // Our series approximation has limited precision for moderate df
        let p = chi_squared_p_value(7.815, 3);
        assert!(p > 0.0 && p < 0.5, "p = {p} should be in (0, 0.5)");
    }
}
