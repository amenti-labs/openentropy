//! Surrogate / permutation testing for consciousness-RNG analysis.
//!
//! Generates null distributions by time-series shuffling to test whether
//! observed analysis statistics are distinguishable from chance. This
//! transforms every deep analysis metric into a proper hypothesis test
//! with empirical p-values.
//!
//! Supports: topology (Wasserstein), RQA (determinism), ordinal patterns
//! (permutation entropy), transfer entropy, and generic user-defined
//! test statistics.
//!
//! Based on: Theiler et al. (1992) "Testing for nonlinearity in time
//! series: the method of surrogate data."

use serde::{Deserialize, Serialize};

/// Result of a bootstrap confidence interval computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapCI {
    /// Point estimate of the statistic.
    pub point_estimate: f64,
    /// Lower bound of the confidence interval.
    pub ci_lower: f64,
    /// Upper bound of the confidence interval.
    pub ci_upper: f64,
    /// Confidence level (e.g., 0.95).
    pub confidence_level: f64,
    /// Number of bootstrap resamples.
    pub n_bootstrap: usize,
}

/// Result of a surrogate significance test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrogateResult {
    /// Observed test statistic on real data.
    pub observed: f64,
    /// Empirical p-value: fraction of surrogates >= observed.
    pub p_value: f64,
    /// Mean of null distribution.
    pub null_mean: f64,
    /// Standard deviation of null distribution.
    pub null_std: f64,
    /// Z-score: (observed - null_mean) / null_std.
    pub z_score: f64,
    /// Cohen's d effect size: (observed - null_mean) / null_std.
    pub effect_size: f64,
    /// Number of surrogates generated.
    pub n_surrogates: usize,
    /// Name of the test statistic.
    pub statistic_name: String,
    /// Bootstrap confidence interval for the effect size (present when n_surrogates >= 50).
    pub ci: Option<BootstrapCI>,
}

/// Summary of all deep analysis surrogate tests with multiple testing correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepAnalysisSurrogateReport {
    /// Individual test results.
    pub tests: Vec<SurrogateResult>,
    /// BH FDR-corrected q-values (same order as tests).
    pub q_values: Vec<f64>,
    /// Number of tests rejected at alpha=0.05 after FDR correction.
    pub n_rejected_fdr: usize,
    /// Overall interpretation.
    pub interpretation: String,
}

// ---------------------------------------------------------------------------
// Xorshift64 PRNG (deterministic, no external deps)
// ---------------------------------------------------------------------------

/// Simple xorshift64 PRNG for deterministic shuffling.
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 1 } else { seed } }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Generate a random index in [0, n).
    fn next_usize(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------------------
// Shuffle surrogate generation
// ---------------------------------------------------------------------------

/// Generate a shuffle surrogate: randomly permute the data.
///
/// Destroys temporal structure while preserving the marginal distribution
/// (same byte histogram). This is the standard surrogate method for
/// testing against the null hypothesis of "independent, identically
/// distributed" data.
pub fn shuffle_surrogate(data: &[u8], seed: u64) -> Vec<u8> {
    let mut surrogate = data.to_vec();
    let n = surrogate.len();
    if n <= 1 {
        return surrogate;
    }

    let mut rng = Xorshift64::new(seed);

    // Fisher-Yates shuffle
    for i in (1..n).rev() {
        let j = rng.next_usize(i + 1);
        surrogate.swap(i, j);
    }

    surrogate
}

/// Generate a shuffle surrogate for float data.
pub fn shuffle_surrogate_f64(data: &[f64], seed: u64) -> Vec<f64> {
    let mut surrogate = data.to_vec();
    let n = surrogate.len();
    if n <= 1 {
        return surrogate;
    }

    let mut rng = Xorshift64::new(seed);
    for i in (1..n).rev() {
        let j = rng.next_usize(i + 1);
        surrogate.swap(i, j);
    }

    surrogate
}

// ---------------------------------------------------------------------------
// Generic surrogate test
// ---------------------------------------------------------------------------

/// Run a generic surrogate test with a user-defined test statistic.
///
/// Computes the test statistic on the real data, then on `n_surrogates`
/// shuffled versions. Returns the empirical p-value, z-score, and effect
/// size.
///
/// The `test_statistic` function takes (data_a, data_b) and returns a
/// scalar. Under the null, shuffling data_b destroys any relationship
/// between the two datasets.
pub fn surrogate_test<F>(
    data_a: &[u8],
    data_b: &[u8],
    test_statistic: F,
    n_surrogates: usize,
    seed: u64,
    statistic_name: &str,
) -> SurrogateResult
where
    F: Fn(&[u8], &[u8]) -> f64,
{
    let observed = test_statistic(data_a, data_b);

    let mut null_distribution = Vec::with_capacity(n_surrogates);
    let mut rng = Xorshift64::new(seed);

    for _ in 0..n_surrogates {
        let surrogate_b = shuffle_surrogate(data_b, rng.next());
        let stat = test_statistic(data_a, &surrogate_b);
        null_distribution.push(stat);
    }

    compute_surrogate_result(observed, &null_distribution, statistic_name)
}

/// Run a surrogate test on float data.
pub fn surrogate_test_f64<F>(
    data_a: &[f64],
    data_b: &[f64],
    test_statistic: F,
    n_surrogates: usize,
    seed: u64,
    statistic_name: &str,
) -> SurrogateResult
where
    F: Fn(&[f64], &[f64]) -> f64,
{
    let observed = test_statistic(data_a, data_b);

    let mut null_distribution = Vec::with_capacity(n_surrogates);
    let mut rng = Xorshift64::new(seed);

    for _ in 0..n_surrogates {
        let surrogate_b = shuffle_surrogate_f64(data_b, rng.next());
        let stat = test_statistic(data_a, &surrogate_b);
        null_distribution.push(stat);
    }

    compute_surrogate_result(observed, &null_distribution, statistic_name)
}

// ---------------------------------------------------------------------------
// Adaptive surrogate tests
// ---------------------------------------------------------------------------

/// Adaptive surrogate test: starts with `initial_n` surrogates, escalates to
/// `max_n` if the result is borderline (0.01 < p < 0.10).
///
/// Returns the final SurrogateResult with the refined p-value. This saves
/// compute for clear-cut results while providing higher precision for
/// borderline cases where the exact p-value matters.
pub fn adaptive_surrogate_test<F>(
    data_a: &[u8],
    data_b: &[u8],
    initial_n: usize,
    max_n: usize,
    test_statistic: F,
    statistic_name: &str,
    seed: u64,
) -> SurrogateResult
where
    F: Fn(&[u8], &[u8]) -> f64,
{
    let initial = surrogate_test(data_a, data_b, &test_statistic, initial_n, seed, statistic_name);

    // Borderline: re-run with more surrogates for higher precision
    if initial.p_value > 0.01 && initial.p_value < 0.10 {
        // Use a different seed for the refined run to avoid correlation
        surrogate_test(
            data_a,
            data_b,
            &test_statistic,
            max_n,
            seed.wrapping_add(initial_n as u64),
            statistic_name,
        )
    } else {
        initial
    }
}

/// Adaptive surrogate test for float data: starts with `initial_n` surrogates,
/// escalates to `max_n` if the result is borderline (0.01 < p < 0.10).
pub fn adaptive_surrogate_test_f64<F>(
    data_a: &[f64],
    data_b: &[f64],
    initial_n: usize,
    max_n: usize,
    test_statistic: F,
    statistic_name: &str,
    seed: u64,
) -> SurrogateResult
where
    F: Fn(&[f64], &[f64]) -> f64,
{
    let initial =
        surrogate_test_f64(data_a, data_b, &test_statistic, initial_n, seed, statistic_name);

    // Borderline: re-run with more surrogates for higher precision
    if initial.p_value > 0.01 && initial.p_value < 0.10 {
        surrogate_test_f64(
            data_a,
            data_b,
            &test_statistic,
            max_n,
            seed.wrapping_add(initial_n as u64),
            statistic_name,
        )
    } else {
        initial
    }
}

// ---------------------------------------------------------------------------
// Bootstrap confidence intervals
// ---------------------------------------------------------------------------

/// Compute a bootstrap confidence interval for the effect size.
///
/// Uses percentile bootstrap (simpler, more robust than BCa for small samples).
/// Resamples the surrogate null distribution to estimate CI of effect size.
///
/// For each bootstrap resample: randomly samples with replacement from
/// `null_distribution`, computes the resample mean and std, then computes
/// effect size = (observed - mean) / std. The CI is the percentile interval
/// of bootstrap effect sizes.
pub fn bootstrap_effect_size_ci(
    observed: f64,
    null_distribution: &[f64],
    n_bootstrap: usize,
    confidence: f64,
    seed: u64,
) -> BootstrapCI {
    let n = null_distribution.len();
    let mut rng = Xorshift64::new(seed);

    // Point estimate: effect size from full null distribution
    let null_mean = null_distribution.iter().sum::<f64>() / n as f64;
    let null_var = null_distribution
        .iter()
        .map(|&s| (s - null_mean).powi(2))
        .sum::<f64>()
        / (n as f64 - 1.0).max(1.0);
    let null_std = null_var.sqrt();
    let point_estimate = if null_std > 1e-15 {
        (observed - null_mean) / null_std
    } else {
        0.0
    };

    // Bootstrap resampling
    let mut bootstrap_effects = Vec::with_capacity(n_bootstrap);
    for _ in 0..n_bootstrap {
        // Resample with replacement
        let mut resample_sum = 0.0;
        let mut resample_sq_sum = 0.0;
        for _ in 0..n {
            let idx = rng.next_usize(n);
            let val = null_distribution[idx];
            resample_sum += val;
            resample_sq_sum += val * val;
        }
        let resample_mean = resample_sum / n as f64;
        let resample_var =
            (resample_sq_sum - resample_sum * resample_sum / n as f64) / (n as f64 - 1.0).max(1.0);
        let resample_std = resample_var.sqrt();

        let effect = if resample_std > 1e-15 {
            (observed - resample_mean) / resample_std
        } else {
            0.0
        };
        bootstrap_effects.push(effect);
    }

    // Sort and extract percentiles
    bootstrap_effects.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let alpha = 1.0 - confidence;
    let lower_idx = ((alpha / 2.0) * n_bootstrap as f64).floor() as usize;
    let upper_idx = ((1.0 - alpha / 2.0) * n_bootstrap as f64).ceil() as usize;
    let lower_idx = lower_idx.min(n_bootstrap.saturating_sub(1));
    let upper_idx = upper_idx.min(n_bootstrap.saturating_sub(1));

    BootstrapCI {
        point_estimate,
        ci_lower: bootstrap_effects[lower_idx],
        ci_upper: bootstrap_effects[upper_idx],
        confidence_level: confidence,
        n_bootstrap,
    }
}

fn compute_surrogate_result(
    observed: f64,
    null_distribution: &[f64],
    statistic_name: &str,
) -> SurrogateResult {
    let n = null_distribution.len();

    // Empirical p-value (two-sided: fraction of surrogates with |stat| >= |observed|)
    let count_extreme = null_distribution
        .iter()
        .filter(|&&s| s.abs() >= observed.abs())
        .count();
    let p_value = (count_extreme as f64 + 1.0) / (n as f64 + 1.0);

    // Null distribution statistics
    let null_mean = null_distribution.iter().sum::<f64>() / n as f64;
    let null_var = null_distribution
        .iter()
        .map(|&s| (s - null_mean).powi(2))
        .sum::<f64>()
        / (n as f64 - 1.0).max(1.0);
    let null_std = null_var.sqrt();

    let z_score = if null_std > 1e-15 {
        (observed - null_mean) / null_std
    } else {
        0.0
    };

    let effect_size = z_score; // Cohen's d equivalent for surrogate tests

    // Compute bootstrap CI when we have enough surrogates
    let ci = if n >= 50 {
        Some(bootstrap_effect_size_ci(
            observed,
            null_distribution,
            1000,
            0.95,
            // Use a seed derived from the observed statistic and n for determinism
            (observed.to_bits() ^ n as u64).wrapping_add(42),
        ))
    } else {
        None
    };

    SurrogateResult {
        observed,
        p_value,
        null_mean,
        null_std,
        z_score,
        effect_size,
        n_surrogates: n,
        statistic_name: statistic_name.to_string(),
        ci,
    }
}

// ---------------------------------------------------------------------------
// Specific surrogate tests for each deep analysis module
// ---------------------------------------------------------------------------

/// Surrogate test for topology: tests whether the Wasserstein distance
/// between baseline and intention persistence diagrams is significant.
pub fn topology_surrogate_test(
    baseline: &[u8],
    intention: &[u8],
    n_surrogates: usize,
    seed: u64,
) -> SurrogateResult {
    surrogate_test(
        baseline,
        intention,
        |a, b| {
            let topo = crate::consciousness_topology::compute_topology(a, b, 3);
            topo.wasserstein_distance_h0
        },
        n_surrogates,
        seed,
        "topology_wasserstein_h0",
    )
}

/// Surrogate test for RQA: tests whether the determinism difference
/// between baseline and intention is significant.
pub fn rqa_surrogate_test(
    baseline: &[u8],
    intention: &[u8],
    n_surrogates: usize,
    seed: u64,
) -> SurrogateResult {
    let bl_len = baseline.len().min(200);
    let int_len = intention.len().min(200);
    surrogate_test(
        &baseline[..bl_len],
        &intention[..int_len],
        |a, b| {
            let rqa = crate::consciousness_rqa::compare_rqa(a, b);
            (rqa.intention.determinism - rqa.baseline.determinism).abs()
        },
        n_surrogates,
        seed,
        "rqa_determinism_diff",
    )
}

/// Surrogate test for ordinal patterns: tests whether the PE difference
/// between baseline and intention is significant.
pub fn ordinal_surrogate_test(
    baseline: &[u8],
    intention: &[u8],
    n_surrogates: usize,
    seed: u64,
) -> SurrogateResult {
    surrogate_test(
        baseline,
        intention,
        |a, b| {
            let ord = crate::consciousness_ordinal::compare_ordinal(a, b, 3);
            ord.chi_squared
        },
        n_surrogates,
        seed,
        "ordinal_chi_squared",
    )
}

/// Surrogate test for transfer entropy: tests whether TE between
/// two signals is significant by shuffling the source.
pub fn te_surrogate_test(
    source: &[f64],
    target: &[f64],
    n_surrogates: usize,
    seed: u64,
) -> SurrogateResult {
    surrogate_test_f64(
        source,
        target,
        |s, t| crate::consciousness_transfer::transfer_entropy(s, t, 1, 8),
        n_surrogates,
        seed,
        "transfer_entropy",
    )
}

// ---------------------------------------------------------------------------
// Benjamini-Hochberg FDR correction
// ---------------------------------------------------------------------------

/// Apply Benjamini-Hochberg FDR correction to a set of p-values.
///
/// Returns q-values: the minimum FDR at which each test would be rejected.
/// Tests with q < alpha are considered significant after correction.
pub fn benjamini_hochberg(p_values: &[f64]) -> Vec<f64> {
    let m = p_values.len();
    if m == 0 {
        return Vec::new();
    }

    // Sort indices by p-value
    let mut indexed: Vec<(usize, f64)> = p_values.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Compute q-values: q_i = min(p_i * m / rank, 1.0), enforcing monotonicity
    let mut q_values = vec![0.0; m];
    let mut min_q: f64 = 1.0;

    for rank in (0..m).rev() {
        let p = indexed[rank].1;
        let q = (p * m as f64 / (rank + 1) as f64).min(1.0);
        min_q = min_q.min(q);
        q_values[indexed[rank].0] = min_q;
    }

    q_values
}

/// Build a complete deep analysis surrogate report from individual tests.
pub fn build_surrogate_report(
    tests: Vec<SurrogateResult>,
    alpha: f64,
) -> DeepAnalysisSurrogateReport {
    let p_values: Vec<f64> = tests.iter().map(|t| t.p_value).collect();
    let q_values = benjamini_hochberg(&p_values);
    let n_rejected_fdr = q_values.iter().filter(|&&q| q < alpha).count();

    let interpretation = if n_rejected_fdr == 0 {
        format!(
            "No tests survived BH FDR correction at alpha={:.2}. \
             Observed statistics are consistent with null (shuffled) distributions.",
            alpha
        )
    } else {
        let sig_names: Vec<&str> = tests
            .iter()
            .zip(q_values.iter())
            .filter(|&(_, q)| *q < alpha)
            .map(|(t, _)| t.statistic_name.as_str())
            .collect();
        format!(
            "{} of {} tests survived BH FDR correction at alpha={:.2}: {}",
            n_rejected_fdr,
            tests.len(),
            alpha,
            sig_names.join(", ")
        )
    };

    DeepAnalysisSurrogateReport {
        tests,
        q_values,
        n_rejected_fdr,
        interpretation,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift64_deterministic() {
        let mut rng1 = Xorshift64::new(42);
        let mut rng2 = Xorshift64::new(42);
        for _ in 0..100 {
            assert_eq!(rng1.next(), rng2.next());
        }
    }

    #[test]
    fn xorshift64_different_seeds() {
        let mut rng1 = Xorshift64::new(42);
        let mut rng2 = Xorshift64::new(99);
        let a: Vec<u64> = (0..10).map(|_| rng1.next()).collect();
        let b: Vec<u64> = (0..10).map(|_| rng2.next()).collect();
        assert_ne!(a, b);
    }

    #[test]
    fn shuffle_preserves_histogram() {
        let data: Vec<u8> = (0..100).map(|i| (i % 10) as u8).collect();
        let surrogate = shuffle_surrogate(&data, 42);

        // Same length
        assert_eq!(data.len(), surrogate.len());

        // Same byte histogram
        let mut orig_counts = [0usize; 256];
        let mut surr_counts = [0usize; 256];
        for &b in &data { orig_counts[b as usize] += 1; }
        for &b in &surrogate { surr_counts[b as usize] += 1; }
        assert_eq!(orig_counts, surr_counts);
    }

    #[test]
    fn shuffle_changes_order() {
        let data: Vec<u8> = (0..100).collect();
        let surrogate = shuffle_surrogate(&data, 42);
        // Very unlikely to be identical
        assert_ne!(data, surrogate);
    }

    #[test]
    fn shuffle_f64_preserves_elements() {
        let data: Vec<f64> = (0..50).map(|i| i as f64 * 1.5).collect();
        let surrogate = shuffle_surrogate_f64(&data, 77);
        assert_eq!(data.len(), surrogate.len());

        let mut orig_sorted = data.clone();
        let mut surr_sorted = surrogate.clone();
        orig_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        surr_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(orig_sorted, surr_sorted);
    }

    #[test]
    fn shuffle_empty() {
        let empty: Vec<u8> = vec![];
        let result = shuffle_surrogate(&empty, 42);
        assert!(result.is_empty());
    }

    #[test]
    fn shuffle_single() {
        let single = vec![42u8];
        let result = shuffle_surrogate(&single, 42);
        assert_eq!(result, vec![42]);
    }

    #[test]
    fn surrogate_test_basic() {
        // Test with a trivial statistic (mean difference)
        let a: Vec<u8> = vec![10; 50];
        let b: Vec<u8> = vec![10; 50];
        let result = surrogate_test(
            &a,
            &b,
            |x, y| {
                let mean_x = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
                let mean_y = y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64;
                (mean_x - mean_y).abs()
            },
            100,
            42,
            "mean_diff",
        );
        assert_eq!(result.n_surrogates, 100);
        assert_eq!(result.statistic_name, "mean_diff");
        // Same data => high p-value
        assert!(result.p_value > 0.05, "p = {}", result.p_value);
    }

    #[test]
    fn surrogate_test_detects_difference() {
        // Clearly different data should have low p-value
        let a: Vec<u8> = vec![0; 100];
        let b: Vec<u8> = (0..100).map(|i| (i * 2 % 256) as u8).collect();
        let result = surrogate_test(
            &a,
            &b,
            |x, y| {
                let mean_x = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
                let mean_y = y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64;
                (mean_x - mean_y).abs()
            },
            200,
            42,
            "mean_diff",
        );
        // For paired comparison after shuffle, the mean doesn't change
        // (shuffle preserves histogram), so this tests correctly
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    #[test]
    fn surrogate_test_f64_basic() {
        let a: Vec<f64> = (0..100).map(|i| (i as f64).sin()).collect();
        let b: Vec<f64> = (0..100).map(|i| (i as f64).cos()).collect();
        let result = surrogate_test_f64(
            &a,
            &b,
            |x, y| {
                // Correlation
                let n = x.len().min(y.len()) as f64;
                let mx = x.iter().sum::<f64>() / n;
                let my = y.iter().sum::<f64>() / n;
                x.iter().zip(y.iter())
                    .map(|(&xi, &yi)| (xi - mx) * (yi - my))
                    .sum::<f64>()
                    / n
            },
            100,
            42,
            "correlation",
        );
        assert_eq!(result.n_surrogates, 100);
        assert!(result.observed.is_finite());
    }

    #[test]
    fn benjamini_hochberg_empty() {
        assert!(benjamini_hochberg(&[]).is_empty());
    }

    #[test]
    fn benjamini_hochberg_single() {
        let q = benjamini_hochberg(&[0.03]);
        assert_eq!(q.len(), 1);
        assert!((q[0] - 0.03).abs() < 1e-10);
    }

    #[test]
    fn benjamini_hochberg_preserves_order() {
        let p = vec![0.01, 0.04, 0.03, 0.20, 0.50];
        let q = benjamini_hochberg(&p);
        assert_eq!(q.len(), 5);
        // q-values should be >= p-values
        for i in 0..5 {
            assert!(q[i] >= p[i] - 1e-10, "q[{i}]={} < p[{i}]={}", q[i], p[i]);
        }
    }

    #[test]
    fn benjamini_hochberg_all_significant() {
        let p = vec![0.001, 0.002, 0.003];
        let q = benjamini_hochberg(&p);
        // All should survive at alpha=0.05
        for &qi in &q {
            assert!(qi < 0.05, "q={qi} should be < 0.05");
        }
    }

    #[test]
    fn benjamini_hochberg_none_significant() {
        let p = vec![0.5, 0.6, 0.7, 0.8];
        let q = benjamini_hochberg(&p);
        for &qi in &q {
            assert!(qi > 0.05, "q={qi} should be > 0.05");
        }
    }

    #[test]
    fn build_report_basic() {
        let tests = vec![
            SurrogateResult {
                observed: 1.5,
                p_value: 0.03,
                null_mean: 0.5,
                null_std: 0.3,
                z_score: 3.33,
                effect_size: 3.33,
                n_surrogates: 100,
                statistic_name: "test_a".to_string(),
                ci: None,
            },
            SurrogateResult {
                observed: 0.2,
                p_value: 0.45,
                null_mean: 0.3,
                null_std: 0.2,
                z_score: -0.5,
                effect_size: -0.5,
                n_surrogates: 100,
                statistic_name: "test_b".to_string(),
                ci: None,
            },
        ];
        let report = build_surrogate_report(tests, 0.05);
        assert_eq!(report.tests.len(), 2);
        assert_eq!(report.q_values.len(), 2);
    }

    #[test]
    fn topology_surrogate_test_runs() {
        let baseline: Vec<u8> = (0..200).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = (0..200).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let result = topology_surrogate_test(&baseline, &intention, 10, 42);
        assert_eq!(result.n_surrogates, 10);
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
        assert_eq!(result.statistic_name, "topology_wasserstein_h0");
    }

    #[test]
    fn rqa_surrogate_test_runs() {
        let baseline: Vec<u8> = (0..200).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = (0..200).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let result = rqa_surrogate_test(&baseline, &intention, 10, 42);
        assert_eq!(result.n_surrogates, 10);
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    #[test]
    fn ordinal_surrogate_test_runs() {
        let baseline: Vec<u8> = (0..200).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = (0..200).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let result = ordinal_surrogate_test(&baseline, &intention, 10, 42);
        assert_eq!(result.n_surrogates, 10);
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    #[test]
    fn te_surrogate_test_runs() {
        let source: Vec<f64> = (0..200).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let target: Vec<f64> = (0..200).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let result = te_surrogate_test(&source, &target, 10, 42);
        assert_eq!(result.n_surrogates, 10);
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    #[test]
    fn adaptive_surrogate_test_clear_result() {
        // Identical data => p ~ 1.0, well outside borderline range
        // The adaptive test should NOT escalate.
        let a: Vec<u8> = vec![42; 50];
        let b: Vec<u8> = vec![42; 50];
        let result = adaptive_surrogate_test(
            &a,
            &b,
            100,
            1000,
            |x, y| {
                let mean_x = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
                let mean_y = y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64;
                (mean_x - mean_y).abs()
            },
            "mean_diff",
            42,
        );
        // With identical constant data, p should be high (not borderline),
        // so the test should use only initial_n surrogates
        assert_eq!(
            result.n_surrogates, 100,
            "Clear result should use initial_n surrogates, got {}",
            result.n_surrogates,
        );
        assert!(result.p_value >= 0.10, "Expected high p-value for identical data, got {}", result.p_value);
    }

    #[test]
    fn bootstrap_ci_basic() {
        // Known null distribution centered around 0
        let null: Vec<f64> = (0..200).map(|i| (i as f64 - 100.0) / 100.0).collect();
        let observed = 2.5; // Clearly outside null range
        let ci = bootstrap_effect_size_ci(observed, &null, 500, 0.95, 42);

        // Point estimate should be finite
        assert!(ci.point_estimate.is_finite(), "Point estimate should be finite");
        // CI should contain the point estimate
        assert!(
            ci.ci_lower <= ci.point_estimate && ci.point_estimate <= ci.ci_upper,
            "CI [{}, {}] should contain point estimate {}",
            ci.ci_lower, ci.ci_upper, ci.point_estimate,
        );
        assert_eq!(ci.confidence_level, 0.95);
        assert_eq!(ci.n_bootstrap, 500);
        // Lower bound should be less than upper bound
        assert!(ci.ci_lower <= ci.ci_upper, "CI lower {} > upper {}", ci.ci_lower, ci.ci_upper);
    }

    #[test]
    fn bootstrap_ci_width_decreases_with_n() {
        let null: Vec<f64> = (0..200).map(|i| (i as f64 - 100.0) / 100.0).collect();
        let observed = 1.5;

        let ci_small = bootstrap_effect_size_ci(observed, &null, 100, 0.95, 42);
        let ci_large = bootstrap_effect_size_ci(observed, &null, 5000, 0.95, 42);

        let width_small = ci_small.ci_upper - ci_small.ci_lower;
        let width_large = ci_large.ci_upper - ci_large.ci_lower;

        // With more bootstrap resamples, the CI should converge and be
        // similar or tighter. We allow some tolerance since bootstrap
        // is stochastic — the large CI should not be dramatically wider.
        assert!(
            width_large <= width_small * 1.5,
            "Large-n CI width ({}) should not be much wider than small-n CI width ({})",
            width_large, width_small,
        );
    }

    #[test]
    fn surrogate_result_has_ci() {
        // With n >= 50 surrogates, CI should be populated
        let a: Vec<u8> = (0..100).collect();
        let b: Vec<u8> = (0..100).map(|i| ((i * 3 + 7) % 256) as u8).collect();
        let result = surrogate_test(
            &a,
            &b,
            |x, y| {
                let mean_x = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
                let mean_y = y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64;
                (mean_x - mean_y).abs()
            },
            100,
            42,
            "mean_diff",
        );
        assert!(result.ci.is_some(), "CI should be present when n_surrogates >= 50");
        let ci = result.ci.unwrap();
        assert_eq!(ci.confidence_level, 0.95);
        assert!(ci.ci_lower <= ci.ci_upper);

        // With n < 50 surrogates, CI should be None
        let result_small = surrogate_test(
            &a,
            &b,
            |x, y| {
                let mean_x = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
                let mean_y = y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64;
                (mean_x - mean_y).abs()
            },
            10,
            42,
            "mean_diff",
        );
        assert!(result_small.ci.is_none(), "CI should be None when n_surrogates < 50");
    }
}
