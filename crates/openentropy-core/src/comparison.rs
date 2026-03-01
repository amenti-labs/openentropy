//! Differential comparison of two byte streams.
//!
//! All functions operate on raw `&[u8]` — no session I/O, no formatting.
//! The CLI layer handles loading, printing, and serialization.
//!
//! # Two-sample philosophy
//!
//! Per-stream metrics (Shannon H, autocorrelation, etc.) are computed by
//! [`analysis::full_analysis`] and displayed side-by-side by the CLI.
//! This module adds **cross-stream** statistics that directly test whether
//! two byte streams differ: two-sample KS, chi-squared homogeneity,
//! Cliff's delta, and Mann-Whitney U.

use serde::Serialize;

use crate::analysis::{self, SourceAnalysis};
use crate::conditioning::{quick_min_entropy, quick_shannon};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Full comparison result between two byte streams.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonResult {
    pub label_a: String,
    pub label_b: String,
    pub size_a: usize,
    pub size_b: usize,
    pub aggregate: AggregateDelta,
    pub two_sample: TwoSampleTests,
    pub temporal: TemporalAnalysis,
    pub digram: DigramAnalysis,
    pub markov: MarkovAnalysis,
    pub multi_lag: MultiLagAnalysis,
    pub run_lengths: RunLengthComparison,
}

/// Aggregate statistics delta between two streams.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateDelta {
    pub shannon_a: f64,
    pub shannon_b: f64,
    pub min_entropy_a: f64,
    pub min_entropy_b: f64,
    pub mean_a: f64,
    pub mean_b: f64,
    pub variance_a: f64,
    pub variance_b: f64,
    pub bit_bias_a: f64,
    pub bit_bias_b: f64,
    pub per_bit_bias_a: [f64; 8],
    pub per_bit_bias_b: [f64; 8],
    pub chi_squared_a: f64,
    pub chi_squared_b: f64,
    pub ks_statistic_a: f64,
    pub ks_statistic_b: f64,
    /// Cohen's d effect size for byte mean difference (assumes normality —
    /// use [`TwoSampleTests::cliffs_delta`] for the non-parametric alternative).
    pub cohens_d: f64,
}

/// Two-sample statistical tests that directly compare stream A against stream B.
#[derive(Debug, Clone, Serialize)]
pub struct TwoSampleTests {
    /// Two-sample Kolmogorov-Smirnov statistic: max|F_A(x) - F_B(x)|.
    /// Tests whether both samples come from the same distribution.
    pub ks_statistic: f64,
    /// Approximate p-value for the two-sample KS test.
    pub ks_p_value: f64,
    /// Chi-squared homogeneity statistic comparing the 256-bin byte histograms.
    /// Tests whether A and B have the same byte frequency distribution.
    pub chi2_homogeneity: f64,
    /// Degrees of freedom for the homogeneity test (255 for byte data).
    pub chi2_df: usize,
    /// Approximate p-value for the chi-squared homogeneity test.
    pub chi2_p_value: f64,
    /// Whether the chi-squared approximation is reliable (expected count >= 5
    /// in every cell of the 2×k contingency table).
    pub chi2_reliable: bool,
    /// Cliff's delta: non-parametric effect size for ordinal data.
    /// Range [-1, 1]. 0 = no difference. Appropriate for discrete byte values
    /// (unlike Cohen's d which assumes normality).
    pub cliffs_delta: f64,
    /// Mann-Whitney U statistic (normalized to [0, 1]).
    /// Tests whether values from A tend to be larger or smaller than values from B.
    /// 0.5 = no difference.
    pub mann_whitney_u: f64,
    /// Approximate p-value for the Mann-Whitney U test (two-tailed, z-approximation).
    /// Valid for large samples (n, m >> 20).
    pub mann_whitney_p_value: f64,
}

/// A single anomalous window detected during temporal analysis.
#[derive(Debug, Clone, Serialize)]
pub struct WindowAnomaly {
    pub offset: usize,
    pub mean: f64,
    /// Z-score vs theoretical Uniform(0,255) parameters, not empirical.
    pub z_score: f64,
}

/// Sliding-window temporal anomaly detection using theoretical parameters.
///
/// Window means are compared against the theoretical mean (127.5) and
/// theoretical standard deviation (sqrt(Var(U(0,255)) / window_size)) rather
/// than empirical estimates. This avoids the circular-statistics pitfall where
/// z-scores computed from empirical parameters always produce ~0.27% outliers.
#[derive(Debug, Clone, Serialize)]
pub struct TemporalAnalysis {
    pub window_size: usize,
    pub anomaly_count_a: usize,
    pub anomaly_count_b: usize,
    pub max_z_a: f64,
    pub max_z_b: f64,
    pub top_anomalies_a: Vec<WindowAnomaly>,
    pub top_anomalies_b: Vec<WindowAnomaly>,
    /// Per-window Shannon entropy. Capped at 1024 entries in each vec to keep
    /// JSON output bounded; for larger sessions the windows are sub-sampled.
    pub windowed_entropy_a: Vec<f64>,
    pub windowed_entropy_b: Vec<f64>,
}

/// Digram (byte bigram) chi-squared uniformity.
///
/// Trigrams are intentionally omitted: hashing 16M possible trigrams into
/// fewer bins produces systematic chi-squared bias from hash collisions,
/// making the statistic unreliable. Digrams with 65536 exact bins are sound
/// when the sample has >= ~320KB (expected count >= 5 per bin).
#[derive(Debug, Clone, Serialize)]
pub struct DigramAnalysis {
    /// Chi-squared statistic for digram uniformity, or NaN if insufficient data.
    pub chi2_a: f64,
    /// Chi-squared statistic for digram uniformity, or NaN if insufficient data.
    pub chi2_b: f64,
    /// Whether the sample was large enough for the chi-squared approximation.
    pub sufficient_data: bool,
    /// Minimum sample size needed (bytes) for expected >= 5 per bin.
    pub min_sample_bytes: usize,
}

/// Per-bit Markov transition probabilities.
#[derive(Debug, Clone, Serialize)]
pub struct MarkovAnalysis {
    /// Per-bit P(to|from): transitions\[bit\]\[from\]\[to\].
    pub transitions_a: [[[f64; 2]; 2]; 8],
    pub transitions_b: [[[f64; 2]; 2]; 8],
}

/// Autocorrelation at multiple lags.
#[derive(Debug, Clone, Serialize)]
pub struct MultiLagAnalysis {
    pub lags: Vec<usize>,
    pub autocorr_a: Vec<f64>,
    pub autocorr_b: Vec<f64>,
}

/// Byte run-length distributions.
#[derive(Debug, Clone, Serialize)]
pub struct RunLengthComparison {
    /// (run_length, count) pairs for stream A.
    pub distribution_a: Vec<(usize, usize)>,
    /// (run_length, count) pairs for stream B.
    pub distribution_b: Vec<(usize, usize)>,
}

// ---------------------------------------------------------------------------
// Top-level comparison
// ---------------------------------------------------------------------------

/// Compare two byte streams and produce a full differential report.
///
/// This is the standalone entry point that computes everything from scratch.
/// If you already have [`SourceAnalysis`] results, prefer [`compare_with_analysis`]
/// to avoid redundant computation.
pub fn compare(label_a: &str, data_a: &[u8], label_b: &str, data_b: &[u8]) -> ComparisonResult {
    ComparisonResult {
        label_a: label_a.to_string(),
        label_b: label_b.to_string(),
        size_a: data_a.len(),
        size_b: data_b.len(),
        aggregate: aggregate_delta(data_a, data_b),
        two_sample: two_sample_tests(data_a, data_b),
        temporal: temporal_analysis(data_a, data_b, 1024, 3.0),
        digram: digram_analysis(data_a, data_b),
        markov: markov_analysis(data_a, data_b),
        multi_lag: multi_lag_analysis(data_a, data_b),
        run_lengths: run_length_comparison(data_a, data_b),
    }
}

/// Compare two byte streams, reusing pre-computed [`SourceAnalysis`] results.
///
/// This avoids re-computing `distribution_stats` and `bit_bias` which are
/// already available from [`analysis::full_analysis`].
pub fn compare_with_analysis(
    label_a: &str,
    data_a: &[u8],
    analysis_a: &SourceAnalysis,
    label_b: &str,
    data_b: &[u8],
    analysis_b: &SourceAnalysis,
) -> ComparisonResult {
    ComparisonResult {
        label_a: label_a.to_string(),
        label_b: label_b.to_string(),
        size_a: data_a.len(),
        size_b: data_b.len(),
        aggregate: aggregate_delta_from_analysis(analysis_a, analysis_b),
        two_sample: two_sample_tests(data_a, data_b),
        temporal: temporal_analysis(data_a, data_b, 1024, 3.0),
        digram: digram_analysis(data_a, data_b),
        markov: markov_analysis(data_a, data_b),
        multi_lag: multi_lag_analysis(data_a, data_b),
        run_lengths: run_length_comparison(data_a, data_b),
    }
}

// ---------------------------------------------------------------------------
// Aggregate delta
// ---------------------------------------------------------------------------

/// Compute aggregate statistics for both streams and Cohen's d effect size.
pub fn aggregate_delta(data_a: &[u8], data_b: &[u8]) -> AggregateDelta {
    let dist_a = analysis::distribution_stats(data_a);
    let dist_b = analysis::distribution_stats(data_b);
    let bias_a = analysis::bit_bias(data_a);
    let bias_b = analysis::bit_bias(data_b);

    build_aggregate(
        quick_shannon(data_a),
        quick_shannon(data_b),
        quick_min_entropy(data_a),
        quick_min_entropy(data_b),
        &dist_a,
        &dist_b,
        &bias_a,
        &bias_b,
    )
}

/// Build aggregate delta from pre-computed analysis results.
fn aggregate_delta_from_analysis(a: &SourceAnalysis, b: &SourceAnalysis) -> AggregateDelta {
    build_aggregate(
        a.shannon_entropy,
        b.shannon_entropy,
        a.min_entropy,
        b.min_entropy,
        &a.distribution,
        &b.distribution,
        &a.bit_bias,
        &b.bit_bias,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_aggregate(
    shannon_a: f64,
    shannon_b: f64,
    min_entropy_a: f64,
    min_entropy_b: f64,
    dist_a: &analysis::DistributionResult,
    dist_b: &analysis::DistributionResult,
    bias_a: &analysis::BitBiasResult,
    bias_b: &analysis::BitBiasResult,
) -> AggregateDelta {
    // Cohen's d = (mean_a - mean_b) / pooled_std
    let pooled_std = ((dist_a.variance + dist_b.variance) / 2.0).sqrt();
    let cohens_d = if pooled_std > 1e-10 {
        (dist_a.mean - dist_b.mean) / pooled_std
    } else {
        0.0
    };

    AggregateDelta {
        shannon_a,
        shannon_b,
        min_entropy_a,
        min_entropy_b,
        mean_a: dist_a.mean,
        mean_b: dist_b.mean,
        variance_a: dist_a.variance,
        variance_b: dist_b.variance,
        bit_bias_a: bias_a.overall_bias,
        bit_bias_b: bias_b.overall_bias,
        per_bit_bias_a: bias_a.bit_probabilities,
        per_bit_bias_b: bias_b.bit_probabilities,
        chi_squared_a: bias_a.chi_squared,
        chi_squared_b: bias_b.chi_squared,
        ks_statistic_a: dist_a.ks_statistic,
        ks_statistic_b: dist_b.ks_statistic,
        cohens_d,
    }
}

// ---------------------------------------------------------------------------
// Two-sample tests
// ---------------------------------------------------------------------------

/// Run proper two-sample statistical tests comparing stream A directly to stream B.
pub fn two_sample_tests(data_a: &[u8], data_b: &[u8]) -> TwoSampleTests {
    let (ks_stat, ks_p) = two_sample_ks(data_a, data_b);
    let (chi2, chi2_df, chi2_p, chi2_reliable) = chi2_homogeneity(data_a, data_b);
    let (mw_u, mw_p) = mann_whitney_u_test(data_a, data_b);

    TwoSampleTests {
        ks_statistic: ks_stat,
        ks_p_value: ks_p,
        chi2_homogeneity: chi2,
        chi2_df,
        chi2_p_value: chi2_p,
        chi2_reliable,
        cliffs_delta: cliffs_delta(data_a, data_b),
        mann_whitney_u: mw_u,
        mann_whitney_p_value: mw_p,
    }
}

/// Two-sample Kolmogorov-Smirnov test.
///
/// Returns (D_statistic, approximate_p_value).
/// D = max|F_A(x) - F_B(x)| over all byte values 0..=255.
fn two_sample_ks(data_a: &[u8], data_b: &[u8]) -> (f64, f64) {
    if data_a.is_empty() || data_b.is_empty() {
        return (0.0, 1.0);
    }

    // Build CDFs from byte histograms.
    let mut hist_a = [0u64; 256];
    let mut hist_b = [0u64; 256];
    for &b in data_a {
        hist_a[b as usize] += 1;
    }
    for &b in data_b {
        hist_b[b as usize] += 1;
    }

    let n_a = data_a.len() as f64;
    let n_b = data_b.len() as f64;
    let mut cdf_a = 0.0;
    let mut cdf_b = 0.0;
    let mut d_max = 0.0f64;

    for i in 0..256 {
        cdf_a += hist_a[i] as f64 / n_a;
        cdf_b += hist_b[i] as f64 / n_b;
        let d = (cdf_a - cdf_b).abs();
        if d > d_max {
            d_max = d;
        }
    }

    // Asymptotic p-value via the Kolmogorov distribution survival function:
    //   P(K > lambda) = 2 * sum_{k=1}^{inf} (-1)^{k+1} * exp(-2*k^2*lambda^2)
    // where lambda = D * sqrt(n_a * n_b / (n_a + n_b)).
    // The alternating series converges to machine precision within ~10 terms.
    let n_eff = (n_a * n_b) / (n_a + n_b);
    let lambda = d_max * n_eff.sqrt();
    let p_value = kolmogorov_survival(lambda);

    (d_max, p_value)
}

/// Chi-squared test of homogeneity between two byte histograms.
///
/// Tests H0: both samples come from the same (unknown) distribution.
/// Returns (chi2_statistic, degrees_of_freedom, approximate_p_value, reliable).
/// The `reliable` flag is false when any cell's expected count is below 5
/// (Cochran's rule), indicating the chi-squared approximation may be poor.
fn chi2_homogeneity(data_a: &[u8], data_b: &[u8]) -> (f64, usize, f64, bool) {
    if data_a.is_empty() || data_b.is_empty() {
        return (0.0, 255, 1.0, false);
    }

    let mut hist_a = [0u64; 256];
    let mut hist_b = [0u64; 256];
    for &b in data_a {
        hist_a[b as usize] += 1;
    }
    for &b in data_b {
        hist_b[b as usize] += 1;
    }

    let n_a = data_a.len() as f64;
    let n_b = data_b.len() as f64;
    let n_total = n_a + n_b;

    let mut chi2 = 0.0;
    let mut df = 0usize;
    let mut reliable = true;

    for i in 0..256 {
        let pooled = hist_a[i] + hist_b[i];
        if pooled == 0 {
            continue; // empty bin — skip, don't count in df
        }
        df += 1;

        let expected_a = pooled as f64 * n_a / n_total;
        let expected_b = pooled as f64 * n_b / n_total;

        if expected_a < CHI2_MIN_EXPECTED || expected_b < CHI2_MIN_EXPECTED {
            reliable = false;
        }

        let diff_a = hist_a[i] as f64 - expected_a;
        let diff_b = hist_b[i] as f64 - expected_b;

        chi2 += diff_a * diff_a / expected_a + diff_b * diff_b / expected_b;
    }

    // df = (rows-1)*(cols-1) = (non_empty_bins - 1) * (2 - 1)
    df = df.saturating_sub(1);
    let p_value = chi_squared_survival(chi2, df);

    (chi2, df, p_value, reliable)
}

/// Cliff's delta: non-parametric effect size for ordinal data.
///
/// delta = (P(A > B) - P(A < B)), range [-1, 1].
/// Computed efficiently via histograms in O(256) rather than O(n*m).
pub fn cliffs_delta(data_a: &[u8], data_b: &[u8]) -> f64 {
    if data_a.is_empty() || data_b.is_empty() {
        return 0.0;
    }

    let mut hist_a = [0u64; 256];
    let mut hist_b = [0u64; 256];
    for &b in data_a {
        hist_a[b as usize] += 1;
    }
    for &b in data_b {
        hist_b[b as usize] += 1;
    }

    // Prefix sum of hist_b to efficiently compute P(A > B) and P(A < B).
    let mut cum_b = [0u64; 257]; // cum_b[i] = count of B values < i
    for i in 0..256 {
        cum_b[i + 1] = cum_b[i] + hist_b[i];
    }

    let mut greater = 0i128; // count(A > B) - count(A < B)
    let n_a = data_a.len() as i128;
    let n_b = data_b.len() as i128;

    for val in 0..256 {
        let count_a = hist_a[val] as i128;
        if count_a == 0 {
            continue;
        }
        let b_less = cum_b[val] as i128; // B values strictly less than val
        let b_greater = n_b - cum_b[val + 1] as i128; // B values strictly greater
        greater += count_a * (b_less - b_greater);
    }

    greater as f64 / (n_a * n_b) as f64
}

/// Mann-Whitney U test with two-tailed z-approximation p-value.
///
/// Returns (U_normalized, p_value) where U_normalized = U / (n_a * n_b) ∈ [0,1]
/// and 0.5 = no difference. The p-value uses the standard large-sample
/// normal approximation with tie correction.
/// Computed efficiently via histograms.
fn mann_whitney_u_test(data_a: &[u8], data_b: &[u8]) -> (f64, f64) {
    if data_a.is_empty() || data_b.is_empty() {
        return (0.5, 1.0);
    }

    let mut hist_a = [0u64; 256];
    let mut hist_b = [0u64; 256];
    for &b in data_a {
        hist_a[b as usize] += 1;
    }
    for &b in data_b {
        hist_b[b as usize] += 1;
    }

    // U_A = sum over all (a_i, b_j) pairs where a_i > b_j, + 0.5 * ties
    let mut cum_b = [0u64; 257];
    for i in 0..256 {
        cum_b[i + 1] = cum_b[i] + hist_b[i];
    }

    let mut u: f64 = 0.0;
    for val in 0..256 {
        let count_a = hist_a[val] as f64;
        if count_a == 0.0 {
            continue;
        }
        let b_less = cum_b[val] as f64;
        let b_equal = hist_b[val] as f64;
        u += count_a * (b_less + 0.5 * b_equal);
    }

    let n_a = data_a.len() as f64;
    let n_b = data_b.len() as f64;
    let u_norm = u / (n_a * n_b);

    // Z-approximation: z = (U - mu_U) / sigma_U
    // mu_U = n_a * n_b / 2
    // sigma_U = sqrt(n_a * n_b * (n_a + n_b + 1) / 12 - tie_correction)
    // Tie correction: n_a * n_b / (12 * N * (N-1)) * sum(t^3 - t)
    // where t = size of each tied group, N = n_a + n_b.
    let n_total = n_a + n_b;
    let mu = n_a * n_b / 2.0;

    let mut tie_sum = 0.0;
    for i in 0..256 {
        let t = (hist_a[i] + hist_b[i]) as f64;
        if t > 1.0 {
            tie_sum += t * t * t - t;
        }
    }

    let sigma_sq = n_a * n_b / 12.0 * ((n_total + 1.0) - tie_sum / (n_total * (n_total - 1.0)));
    let p_value = if sigma_sq > 0.0 {
        let z = (u - mu).abs() / sigma_sq.sqrt();
        // Two-tailed p-value from standard normal: p = 2 * Phi(-|z|)
        // Using the complementary error function: erfc(x/sqrt(2))
        erfc(z / std::f64::consts::SQRT_2)
    } else {
        1.0
    };

    (u_norm, p_value.clamp(0.0, 1.0))
}

/// Complementary error function, erfc(x) = 1 - erf(x).
///
/// Uses Abramowitz & Stegun approximation 7.1.26 (maximum error < 1.5e-7).
fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let result = poly * (-x * x).exp();
    if x >= 0.0 { result } else { 2.0 - result }
}

// ---------------------------------------------------------------------------
// Temporal analysis
// ---------------------------------------------------------------------------

/// Maximum number of per-window entropy values retained in the result.
/// Keeps JSON output bounded for very large sessions.
const MAX_WINDOWED_ENTROPY: usize = 1024;

/// Variance of Uniform(0, 255): E[X^2] - E[X]^2 = (255*256/3)/256 ... exact value:
/// Var = (256^2 - 1) / 12 = 65535 / 12 ≈ 5461.25
const UNIFORM_BYTE_VARIANCE: f64 = 65535.0 / 12.0;

/// Theoretical mean of Uniform(0, 255).
const UNIFORM_BYTE_MEAN: f64 = 127.5;

/// Sliding-window anomaly detection over both streams.
///
/// Uses **theoretical** parameters from Uniform(0,255) for z-score computation,
/// avoiding the circular-statistics pitfall of comparing data against itself.
pub fn temporal_analysis(
    data_a: &[u8],
    data_b: &[u8],
    window: usize,
    z_threshold: f64,
) -> TemporalAnalysis {
    let (anomalies_a, entropy_a) = windowed_scan(data_a, window, z_threshold);
    let (anomalies_b, entropy_b) = windowed_scan(data_b, window, z_threshold);

    let max_z_a = anomalies_a
        .iter()
        .map(|a| a.z_score.abs())
        .fold(0.0f64, f64::max);
    let max_z_b = anomalies_b
        .iter()
        .map(|a| a.z_score.abs())
        .fold(0.0f64, f64::max);

    let count_a = anomalies_a.len();
    let count_b = anomalies_b.len();

    // Keep top 20 anomalies by |z|.
    let top_a = top_anomalies(anomalies_a, 20);
    let top_b = top_anomalies(anomalies_b, 20);

    TemporalAnalysis {
        window_size: window,
        anomaly_count_a: count_a,
        anomaly_count_b: count_b,
        max_z_a,
        max_z_b,
        top_anomalies_a: top_a,
        top_anomalies_b: top_b,
        windowed_entropy_a: subsample(entropy_a, MAX_WINDOWED_ENTROPY),
        windowed_entropy_b: subsample(entropy_b, MAX_WINDOWED_ENTROPY),
    }
}

/// Scan data in non-overlapping windows using theoretical Uniform(0,255) parameters.
fn windowed_scan(data: &[u8], window: usize, z_threshold: f64) -> (Vec<WindowAnomaly>, Vec<f64>) {
    if data.is_empty() || window == 0 {
        return (Vec::new(), Vec::new());
    }

    let n_windows = data.len() / window;
    if n_windows == 0 {
        return (Vec::new(), Vec::new());
    }

    // Theoretical std of window mean under Uniform(0,255).
    let theoretical_std = (UNIFORM_BYTE_VARIANCE / window as f64).sqrt();

    let mut anomalies = Vec::new();
    let mut entropies = Vec::with_capacity(n_windows);

    for i in 0..n_windows {
        let start = i * window;
        let chunk = &data[start..start + window];
        let sum: f64 = chunk.iter().map(|&b| b as f64).sum();
        let mean = sum / window as f64;
        entropies.push(quick_shannon(chunk));

        // Z-score against theoretical parameters — not circular.
        let z = if theoretical_std > 1e-10 {
            (mean - UNIFORM_BYTE_MEAN) / theoretical_std
        } else {
            0.0
        };

        if z.abs() > z_threshold {
            anomalies.push(WindowAnomaly {
                offset: i * window,
                mean,
                z_score: z,
            });
        }
    }

    (anomalies, entropies)
}

/// Keep top N anomalies by |z_score|.
fn top_anomalies(mut anomalies: Vec<WindowAnomaly>, n: usize) -> Vec<WindowAnomaly> {
    anomalies.sort_by(|a, b| {
        b.z_score
            .abs()
            .partial_cmp(&a.z_score.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    anomalies.truncate(n);
    anomalies
}

/// Uniformly subsample a vector to at most `max` elements.
fn subsample(v: Vec<f64>, max: usize) -> Vec<f64> {
    if v.len() <= max {
        return v;
    }
    let step = v.len() as f64 / max as f64;
    (0..max).map(|i| v[(i as f64 * step) as usize]).collect()
}

// ---------------------------------------------------------------------------
// Digram analysis
// ---------------------------------------------------------------------------

/// Minimum expected count per bin for chi-squared to be valid.
const CHI2_MIN_EXPECTED: f64 = 5.0;

/// Number of possible byte digrams (256^2).
const N_DIGRAM_BINS: usize = 65536;

/// Compute digram chi-squared statistics for both streams.
///
/// Returns NaN for chi-squared values when the sample is too small for the
/// chi-squared approximation to be valid (expected count per bin < 5).
pub fn digram_analysis(data_a: &[u8], data_b: &[u8]) -> DigramAnalysis {
    // Need n-1 >= N_DIGRAM_BINS * 5 = 327,680 digrams, so ~327,681 bytes.
    let min_bytes = N_DIGRAM_BINS * CHI2_MIN_EXPECTED as usize + 1;
    let sufficient = data_a.len() >= min_bytes && data_b.len() >= min_bytes;

    DigramAnalysis {
        chi2_a: digram_chi2(data_a),
        chi2_b: digram_chi2(data_b),
        sufficient_data: sufficient,
        min_sample_bytes: min_bytes,
    }
}

/// Chi-squared statistic for byte digram (bigram) uniformity.
///
/// Returns NaN if the sample is too small for the approximation to be valid.
fn digram_chi2(data: &[u8]) -> f64 {
    if data.len() < 2 {
        return f64::NAN;
    }
    let n_digrams = data.len() - 1;
    let expected = n_digrams as f64 / N_DIGRAM_BINS as f64;
    if expected < CHI2_MIN_EXPECTED {
        return f64::NAN;
    }

    let mut counts = vec![0u64; N_DIGRAM_BINS];
    for pair in data.windows(2) {
        let idx = (pair[0] as usize) * 256 + pair[1] as usize;
        counts[idx] += 1;
    }

    counts
        .iter()
        .map(|&c| {
            let diff = c as f64 - expected;
            diff * diff / expected
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Markov analysis
// ---------------------------------------------------------------------------

/// Per-bit first-order Markov transition probabilities.
pub fn markov_analysis(data_a: &[u8], data_b: &[u8]) -> MarkovAnalysis {
    MarkovAnalysis {
        transitions_a: bit_markov_transitions(data_a),
        transitions_b: bit_markov_transitions(data_b),
    }
}

/// Compute P(to|from) for each of the 8 bit positions.
fn bit_markov_transitions(data: &[u8]) -> [[[f64; 2]; 2]; 8] {
    let mut counts = [[[0u64; 2]; 2]; 8]; // [bit][from][to]

    for pair in data.windows(2) {
        let prev = pair[0];
        let curr = pair[1];
        for bit in 0..8u8 {
            let from = ((prev >> bit) & 1) as usize;
            let to = ((curr >> bit) & 1) as usize;
            counts[bit as usize][from][to] += 1;
        }
    }

    let mut probs = [[[0.0f64; 2]; 2]; 8];
    for (bit, count_bit) in counts.iter().enumerate() {
        for (from, count_from) in count_bit.iter().enumerate() {
            let total = count_from[0] + count_from[1];
            if total > 0 {
                probs[bit][from][0] = count_from[0] as f64 / total as f64;
                probs[bit][from][1] = count_from[1] as f64 / total as f64;
            }
        }
    }

    probs
}

// ---------------------------------------------------------------------------
// Multi-lag autocorrelation
// ---------------------------------------------------------------------------

/// Autocorrelation at standard lags for both streams.
pub fn multi_lag_analysis(data_a: &[u8], data_b: &[u8]) -> MultiLagAnalysis {
    let lags: Vec<usize> = vec![1, 2, 3, 4, 5, 8, 16, 32, 64, 128];

    let profile_a = analysis::autocorrelation_profile(data_a, 128);
    let profile_b = analysis::autocorrelation_profile(data_b, 128);

    let extract = |profile: &analysis::AutocorrResult| -> Vec<f64> {
        lags.iter()
            .map(|&lag| {
                profile
                    .lags
                    .iter()
                    .find(|lc| lc.lag == lag)
                    .map(|lc| lc.correlation)
                    .unwrap_or(0.0)
            })
            .collect()
    };

    MultiLagAnalysis {
        autocorr_a: extract(&profile_a),
        autocorr_b: extract(&profile_b),
        lags,
    }
}

// ---------------------------------------------------------------------------
// Run-length comparison
// ---------------------------------------------------------------------------

/// Byte run-length distributions for both streams.
pub fn run_length_comparison(data_a: &[u8], data_b: &[u8]) -> RunLengthComparison {
    RunLengthComparison {
        distribution_a: run_length_distribution(data_a),
        distribution_b: run_length_distribution(data_b),
    }
}

/// Compute (run_length, count) distribution for consecutive identical bytes.
fn run_length_distribution(data: &[u8]) -> Vec<(usize, usize)> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut dist: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut current_len = 1usize;

    for i in 1..data.len() {
        if data[i] == data[i - 1] {
            current_len += 1;
        } else {
            *dist.entry(current_len).or_default() += 1;
            current_len = 1;
        }
    }
    // Final run.
    *dist.entry(current_len).or_default() += 1;

    dist.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Shared statistical helpers
// ---------------------------------------------------------------------------

/// Kolmogorov distribution survival function: P(K > lambda).
///
/// Uses the standard alternating series:
///   P(K > x) = 2 * sum_{k=1}^{inf} (-1)^{k+1} * exp(-2*k^2*x^2)
/// Converges to machine precision within ~10 terms for any lambda > 0.
fn kolmogorov_survival(lambda: f64) -> f64 {
    if lambda <= 0.0 {
        return 1.0;
    }
    let mut sum = 0.0;
    for k in 1..=100 {
        let kf = k as f64;
        let term = (-2.0 * kf * kf * lambda * lambda).exp();
        if term < 1e-15 {
            break;
        }
        if k % 2 == 1 {
            sum += term;
        } else {
            sum -= term;
        }
    }
    (2.0 * sum).clamp(0.0, 1.0)
}

/// Upper-tail probability (survival function) for chi-squared distribution.
///
/// P(X > chi2 | df) using the regularized incomplete gamma function.
fn chi_squared_survival(chi2: f64, df: usize) -> f64 {
    if df == 0 || chi2 < 0.0 {
        return 1.0;
    }
    if chi2 == 0.0 {
        return 1.0;
    }
    let a = df as f64 / 2.0;
    let x = chi2 / 2.0;

    // For extremely large x relative to a, the survival is effectively 0.
    // The log-space computation: log_q ≈ -x + a*ln(x) - ln_gamma(a)
    // If this is very negative, return 0 to avoid numerical issues.
    let log_prefix = -x + a * x.ln() - ln_gamma(a);
    if log_prefix < -700.0 {
        return 0.0;
    }

    if x < a + 1.0 {
        // Series expansion for the regularized lower incomplete gamma P(a,x).
        // Survival = 1 - P(a,x).
        let mut sum = 0.0;
        let mut term = 1.0 / a;
        sum += term;
        for n in 1..300 {
            term *= x / (a + n as f64);
            sum += term;
            if term.abs() < 1e-14 {
                break;
            }
        }
        let log_p = -x + a * x.ln() - ln_gamma(a) + sum.ln();
        (1.0 - log_p.exp()).clamp(0.0, 1.0)
    } else {
        // Continued fraction (Lentz) for the upper incomplete gamma Q(a,x).
        // This converges fast when x >= a+1.

        // Modified Lentz algorithm: f_0 = C_0 = b_0, D_0 = 0 → D_1 = 1/b_0
        let b0 = x - a + 1.0;
        let mut f = if b0.abs() < 1e-30 { 1e-30 } else { b0 };
        let mut c = f;
        let mut d = 0.0_f64;

        for n in 1..300 {
            let nf = n as f64;
            let a_n = nf * (a - nf);
            let b_n = b0 + 2.0 * nf;

            d = b_n + a_n * d;
            if d.abs() < 1e-30 {
                d = 1e-30;
            }
            d = 1.0 / d;

            c = b_n + a_n / c;
            if c.abs() < 1e-30 {
                c = 1e-30;
            }

            let delta = c * d;
            f *= delta;
            if (delta - 1.0).abs() < 1e-14 {
                break;
            }
        }

        // Q(a,x) = exp(-x) * x^a / Gamma(a) / CF
        let log_q = -x + a * x.ln() - ln_gamma(a) - f.ln();
        log_q.exp().clamp(0.0, 1.0)
    }
}

/// Log-gamma via Lanczos approximation.
fn ln_gamma(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let g = 7.0;
    let c = [
        0.999_999_999_999_809_9,
        676.5203681218851,
        -1259.1392167224028,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507343278686905,
        -0.13857109526572012,
        9.984_369_578_019_572e-6,
        1.5056327351493116e-7,
    ];

    let x = x - 1.0;
    let mut sum = c[0];
    for (i, &coeff) in c[1..].iter().enumerate() {
        sum += coeff / (x + i as f64 + 1.0);
    }
    let t = x + g + 0.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (t.ln() * (x + 0.5)) - t + sum.ln()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pseudo_random(n: usize, seed: u64) -> Vec<u8> {
        let mut data = Vec::with_capacity(n);
        let mut state: u64 = seed;
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            data.push((state >> 33) as u8);
        }
        data
    }

    // -- Top-level compare --

    #[test]
    fn compare_two_random_streams() {
        let a = pseudo_random(10_000, 0xdeadbeef);
        let b = pseudo_random(10_000, 0xcafebabe);
        let result = compare("a", &a, "b", &b);
        assert_eq!(result.size_a, 10_000);
        assert_eq!(result.size_b, 10_000);
        assert!(result.aggregate.shannon_a > 7.0);
        assert!(result.aggregate.shannon_b > 7.0);
        assert!(result.aggregate.cohens_d.abs() < 0.5);
    }

    #[test]
    fn compare_biased_vs_random() {
        let a = vec![128u8; 10_000];
        let b = pseudo_random(10_000, 42);
        let result = compare("constant", &a, "random", &b);
        assert!(result.aggregate.shannon_a < 0.01);
        assert!(result.aggregate.shannon_b > 7.0);
        assert!(result.aggregate.variance_a < 0.01);
        assert!(result.aggregate.variance_b > 100.0);
    }

    #[test]
    fn compare_with_precomputed_analysis() {
        let a = pseudo_random(5_000, 1);
        let b = pseudo_random(5_000, 2);
        let analysis_a = analysis::full_analysis("a", &a);
        let analysis_b = analysis::full_analysis("b", &b);

        let result = compare_with_analysis("a", &a, &analysis_a, "b", &b, &analysis_b);
        let standalone = compare("a", &a, "b", &b);

        assert!((result.aggregate.shannon_a - standalone.aggregate.shannon_a).abs() < 1e-10);
        assert!((result.aggregate.mean_a - standalone.aggregate.mean_a).abs() < 1e-10);
        assert!((result.aggregate.cohens_d - standalone.aggregate.cohens_d).abs() < 1e-10);
    }

    #[test]
    fn compare_empty_inputs() {
        let result = compare("empty_a", &[], "empty_b", &[]);
        assert_eq!(result.size_a, 0);
        assert_eq!(result.size_b, 0);
        assert!(result.temporal.windowed_entropy_a.is_empty());
        assert!(result.run_lengths.distribution_a.is_empty());
    }

    #[test]
    fn compare_small_inputs() {
        let a = pseudo_random(100, 1);
        let b = pseudo_random(100, 2);
        let result = compare("small_a", &a, "small_b", &b);
        assert_eq!(result.size_a, 100);
        assert!(result.temporal.windowed_entropy_a.is_empty());
        assert!(result.aggregate.shannon_a > 0.0);
    }

    // -- Two-sample tests --

    #[test]
    fn two_sample_ks_identical_streams() {
        let a = pseudo_random(10_000, 42);
        let b = a.clone();
        let (d, p) = two_sample_ks(&a, &b);
        assert_eq!(d, 0.0);
        assert_eq!(p, 1.0);
    }

    #[test]
    fn two_sample_ks_different_distributions() {
        let a = vec![0u8; 10_000]; // all zeros
        let b = vec![255u8; 10_000]; // all 255s
        let (d, p) = two_sample_ks(&a, &b);
        assert!((d - 1.0).abs() < 1e-10); // maximally different CDFs
        assert!(p < 0.001);
    }

    #[test]
    fn two_sample_ks_similar_random() {
        let a = pseudo_random(10_000, 1);
        let b = pseudo_random(10_000, 2);
        let (d, _p) = two_sample_ks(&a, &b);
        // Two pseudo-random streams should have small D.
        assert!(d < 0.05, "D = {d}");
    }

    #[test]
    fn chi2_homogeneity_identical() {
        let a = pseudo_random(10_000, 42);
        let b = a.clone();
        let (chi2, _df, p, reliable) = chi2_homogeneity(&a, &b);
        assert_eq!(chi2, 0.0);
        assert!((p - 1.0).abs() < 0.01);
        assert!(reliable);
    }

    #[test]
    fn chi2_homogeneity_different() {
        let a = vec![0u8; 10_000];
        let b = vec![255u8; 10_000];
        let (chi2, _df, p, reliable) = chi2_homogeneity(&a, &b);
        assert!(chi2 > 1000.0);
        assert!(p < 0.001);
        // Only 2 bins populated, each with large counts — reliable.
        assert!(reliable);
    }

    #[test]
    fn chi2_homogeneity_unreliable_small_sample() {
        let a = pseudo_random(20, 1);
        let b = pseudo_random(20, 2);
        let (_chi2, _df, _p, reliable) = chi2_homogeneity(&a, &b);
        // 20 bytes spread across 256 bins → most expected counts well below 5.
        assert!(!reliable);
    }

    #[test]
    fn cliffs_delta_identical() {
        let a = pseudo_random(5_000, 42);
        let b = a.clone();
        let d = cliffs_delta(&a, &b);
        assert_eq!(d, 0.0);
    }

    #[test]
    fn cliffs_delta_maximally_different() {
        let a = vec![0u8; 1_000];
        let b = vec![255u8; 1_000];
        let d = cliffs_delta(&a, &b);
        // A is always less than B → delta = -1.0
        assert!((d - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn cliffs_delta_small_for_similar_random() {
        let a = pseudo_random(10_000, 1);
        let b = pseudo_random(10_000, 2);
        let d = cliffs_delta(&a, &b);
        assert!(d.abs() < 0.1, "Cliff's delta = {d}");
    }

    #[test]
    fn mann_whitney_identical() {
        let a = pseudo_random(5_000, 42);
        let b = a.clone();
        let (u, p) = mann_whitney_u_test(&a, &b);
        assert!((u - 0.5).abs() < 0.01, "U = {u}");
        // Identical streams → not significant.
        assert!(p > 0.05, "p = {p}");
    }

    #[test]
    fn mann_whitney_all_less() {
        let a = vec![0u8; 1_000];
        let b = vec![255u8; 1_000];
        let (u, p) = mann_whitney_u_test(&a, &b);
        // A is always less: U_A ≈ 0
        assert!(u < 0.01, "U = {u}");
        // Maximally different → highly significant.
        assert!(p < 0.001, "p = {p}");
    }

    #[test]
    fn mann_whitney_similar_random() {
        let a = pseudo_random(10_000, 1);
        let b = pseudo_random(10_000, 2);
        let (u, _p) = mann_whitney_u_test(&a, &b);
        // Similar random streams: U ≈ 0.5
        assert!((u - 0.5).abs() < 0.05, "U = {u}");
    }

    // -- Temporal --

    #[test]
    fn temporal_analysis_basic() {
        let a = pseudo_random(10_000, 1);
        let b = pseudo_random(10_000, 2);
        let result = temporal_analysis(&a, &b, 1024, 3.0);
        assert_eq!(result.window_size, 1024);
        assert!(!result.windowed_entropy_a.is_empty());
    }

    #[test]
    fn temporal_uses_theoretical_params() {
        // For truly uniform data, anomalies detected by theoretical z-scores
        // should be fewer than the ~0.27% you'd get from circular empirical z-scores.
        let a = pseudo_random(100_000, 1);
        let result = temporal_analysis(&a, &a, 1024, 3.0);
        let n_windows = 100_000 / 1024;
        // With theoretical params, a good PRNG should have very few anomalies.
        // (Circular empirical would always give ~0.27% = ~0.26 windows per 97.)
        assert!(
            result.anomaly_count_a < n_windows / 5,
            "Too many anomalies: {}/{}",
            result.anomaly_count_a,
            n_windows
        );
    }

    #[test]
    fn temporal_analysis_subsamples_large_output() {
        let a = pseudo_random(100_000, 1);
        let b = pseudo_random(100_000, 2);
        let result = temporal_analysis(&a, &b, 64, 3.0);
        assert!(result.windowed_entropy_a.len() <= MAX_WINDOWED_ENTROPY);
        assert!(result.windowed_entropy_b.len() <= MAX_WINDOWED_ENTROPY);
    }

    // -- Digram --

    #[test]
    fn digram_insufficient_data() {
        let a = pseudo_random(1_000, 1);
        let b = pseudo_random(1_000, 2);
        let result = digram_analysis(&a, &b);
        assert!(!result.sufficient_data);
        assert!(result.chi2_a.is_nan());
    }

    #[test]
    fn digram_sufficient_data() {
        let a = pseudo_random(400_000, 1);
        let b = pseudo_random(400_000, 2);
        let result = digram_analysis(&a, &b);
        assert!(result.sufficient_data);
        assert!(result.chi2_a.is_finite());
        assert!(result.chi2_a > 0.0);
    }

    // -- Markov --

    #[test]
    fn markov_rows_sum_to_one() {
        let a = pseudo_random(5_000, 1);
        let b = pseudo_random(5_000, 2);
        let result = markov_analysis(&a, &b);
        for bit in 0..8 {
            for from in 0..2 {
                let sum_a = result.transitions_a[bit][from][0] + result.transitions_a[bit][from][1];
                assert!(
                    (sum_a - 1.0).abs() < 0.01,
                    "A bit={bit} from={from} sum={sum_a}"
                );
                let sum_b = result.transitions_b[bit][from][0] + result.transitions_b[bit][from][1];
                assert!(
                    (sum_b - 1.0).abs() < 0.01,
                    "B bit={bit} from={from} sum={sum_b}"
                );
            }
        }
    }

    // -- Multi-lag --

    #[test]
    fn multi_lag_lengths_match() {
        let a = pseudo_random(5_000, 1);
        let b = pseudo_random(5_000, 2);
        let result = multi_lag_analysis(&a, &b);
        assert_eq!(result.lags.len(), result.autocorr_a.len());
        assert_eq!(result.lags.len(), result.autocorr_b.len());
    }

    // -- Run-length --

    #[test]
    fn run_length_basic() {
        let a = pseudo_random(5_000, 1);
        let b = pseudo_random(5_000, 2);
        let result = run_length_comparison(&a, &b);
        assert!(!result.distribution_a.is_empty());
        assert!(!result.distribution_b.is_empty());
    }

    #[test]
    fn run_length_constant_stream() {
        let data = vec![42u8; 100];
        let dist = run_length_distribution(&data);
        assert_eq!(dist.len(), 1);
        assert_eq!(dist[0], (100, 1));
    }

    // -- Chi-squared survival helper --

    #[test]
    fn chi_squared_survival_zero() {
        // P(X > 0) should be ~1.0
        let p = chi_squared_survival(0.0, 10);
        assert!((p - 1.0).abs() < 0.01);
    }

    #[test]
    fn chi_squared_survival_large() {
        // P(X > 1000 | df=10) should be ~0.0
        let p = chi_squared_survival(1000.0, 10);
        assert!(p < 0.001);
    }

    #[test]
    fn chi_squared_survival_midrange() {
        // chi2=18.31, df=10 is the 0.05 critical value.
        // SciPy reference: chi2.sf(18.31, 10) = 0.049954
        let p = chi_squared_survival(18.31, 10);
        assert!((p - 0.05).abs() < 0.005, "Expected p ≈ 0.05, got p = {p}");

        // chi2=23.21, df=10 is the 0.01 critical value.
        // SciPy reference: chi2.sf(23.21, 10) = 0.009997
        let p = chi_squared_survival(23.21, 10);
        assert!((p - 0.01).abs() < 0.002, "Expected p ≈ 0.01, got p = {p}");
    }

    #[test]
    fn chi_squared_survival_high_df() {
        // chi2=290, df=255 → SciPy gives p ≈ 0.065
        let p = chi_squared_survival(290.0, 255);
        assert!((p - 0.065).abs() < 0.01, "Expected p ≈ 0.065, got p = {p}");
    }

    // -- Kolmogorov survival --

    #[test]
    fn kolmogorov_survival_zero() {
        assert_eq!(kolmogorov_survival(0.0), 1.0);
    }

    #[test]
    fn kolmogorov_survival_large() {
        let p = kolmogorov_survival(3.0);
        assert!(p < 1e-6);
    }

    #[test]
    fn kolmogorov_survival_matches_scipy() {
        // SciPy reference: for lambda=1.2304, p ≈ 0.096852
        let p = kolmogorov_survival(1.2304);
        assert!(
            (p - 0.0969).abs() < 0.001,
            "Expected p ≈ 0.097, got p = {p}"
        );

        // For small lambda where single-term would be wrong:
        // lambda=0.5 → full series gives ~0.964
        let p = kolmogorov_survival(0.5);
        assert!((p - 0.964).abs() < 0.01, "Expected p ≈ 0.964, got p = {p}");
    }

    // -- erfc --

    #[test]
    fn erfc_known_values() {
        // erfc(0) = 1
        assert!((erfc(0.0) - 1.0).abs() < 1e-6);
        // erfc(large) ≈ 0
        assert!(erfc(5.0) < 1e-6);
        // erfc(1.0) ≈ 0.1573 (from tables)
        assert!((erfc(1.0) - 0.1573).abs() < 0.001);
    }
}
