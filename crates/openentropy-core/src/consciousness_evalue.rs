//! E-values / anytime-valid inference for consciousness-RNG experiments.
//!
//! Replaces p-values with e-values (test martingales) that remain valid under
//! optional stopping. Ville's inequality guarantees P(max E >= 1/alpha) <= alpha
//! for any stopping rule — solving the "peeking problem" that plagues
//! consciousness-RNG research.
//!
//! # Key insight
//!
//! Under H0 (Binomial(n, 0.5)), the likelihood ratio for H1 (Binomial(n, 0.5+delta))
//! is an e-value: its expected value under H0 is exactly 1. The running product
//! (wealth process) is a test martingale. You can stop at any time and the
//! error control remains valid.

use serde::{Deserialize, Serialize};

/// Result of a sequential e-value test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequentialEResult {
    /// Per-trial e-values.
    pub trial_evalues: Vec<f64>,
    /// Cumulative wealth process (running product).
    pub wealth_process: Vec<f64>,
    /// Final e-value (product of all trial e-values).
    pub final_evalue: f64,
    /// Evidence interpretation.
    pub evidence_level: String,
    /// Approximate p-value (1/e calibration).
    pub approx_p: f64,
    /// Trial at which evidence first exceeded threshold (if any).
    pub first_crossing_trial: Option<usize>,
    /// Delta (effect size) used.
    pub delta: f64,
    /// Number of trials.
    pub n_trials: usize,
}

/// Compute the e-value for a single Bernoulli trial.
///
/// Under H0: p = 0.5, under H1: p = 0.5 + delta.
/// The likelihood ratio is: ((0.5+delta)^k * (0.5-delta)^(n-k)) / (0.5^n)
/// where k = ones_count, n = n_bits.
///
/// This is an e-value because E_H0[LR] = 1 exactly.
pub fn trial_evalue(ones: u32, n_bits: usize, delta: f64) -> f64 {
    if n_bits == 0 || delta.abs() < 1e-15 {
        return 1.0;
    }

    let k = ones as f64;
    let n = n_bits as f64;

    let p1 = 0.5 + delta;
    let p0 = 0.5;
    let q1 = 1.0 - p1;

    // Log-likelihood ratio to avoid overflow:
    // log(LR) = k*log(p1/p0) + (n-k)*log(q1/(1-p0))
    let log_lr = k * (p1 / p0).ln() + (n - k) * (q1 / (1.0 - p0)).ln();

    // Clamp to avoid extreme values
    log_lr.exp().clamp(1e-15, 1e15)
}

/// Compute the running e-process (wealth process) from a sequence of e-values.
///
/// The wealth process W_t = prod(e_i for i=1..t) is a test martingale.
/// By Ville's inequality: P(max W_t >= 1/alpha) <= alpha under H0.
pub fn running_eprocess(evalues: &[f64]) -> Vec<f64> {
    let mut wealth = Vec::with_capacity(evalues.len());
    let mut product = 1.0;
    for &e in evalues {
        product *= e;
        // Clamp to prevent numerical issues
        product = product.clamp(1e-30, 1e30);
        wealth.push(product);
    }
    wealth
}

/// Interpret the evidence strength of an e-value.
///
/// Following Vovk's calibration and analogous to Bayes factor interpretation:
/// - e < 1: no evidence against H0
/// - 1 <= e < 3: anecdotal evidence
/// - 3 <= e < 10: moderate evidence
/// - 10 <= e < 30: strong evidence
/// - 30 <= e < 100: very strong evidence
/// - e >= 100: decisive evidence
pub fn evalue_threshold(e: f64) -> &'static str {
    if e < 1.0 {
        "no evidence"
    } else if e < 3.0 {
        "anecdotal"
    } else if e < 10.0 {
        "moderate"
    } else if e < 30.0 {
        "strong"
    } else if e < 100.0 {
        "very strong"
    } else {
        "decisive"
    }
}

/// Convert an e-value to an approximate p-value via Markov/Ville calibration.
///
/// p <= 1/e is always valid (by Markov's inequality applied to the
/// test martingale). This is conservative but always valid.
pub fn evalue_to_approx_p(e: f64) -> f64 {
    if e <= 0.0 {
        return 1.0;
    }
    (1.0 / e).clamp(0.0, 1.0)
}

/// Run a full sequential e-value test on a series of trials.
///
/// - `ones_counts`: number of 1-bits in each trial
/// - `n_bits`: bits per trial (same for all)
/// - `delta`: effect size under H1 (e.g., 0.01 for 1% shift)
///
/// Returns the complete sequential test result including the wealth process,
/// evidence level, and optional stopping point.
pub fn sequential_evalue_test(
    ones_counts: &[u32],
    n_bits: usize,
    delta: f64,
) -> SequentialEResult {
    let trial_evalues: Vec<f64> = ones_counts
        .iter()
        .map(|&ones| trial_evalue(ones, n_bits, delta))
        .collect();

    let wealth_process = running_eprocess(&trial_evalues);
    let final_evalue = wealth_process.last().copied().unwrap_or(1.0);

    // Find first crossing of e >= 20 threshold (strong evidence)
    let first_crossing_trial = wealth_process
        .iter()
        .position(|&w| w >= 20.0);

    SequentialEResult {
        trial_evalues,
        wealth_process,
        final_evalue,
        evidence_level: evalue_threshold(final_evalue).to_string(),
        approx_p: evalue_to_approx_p(final_evalue),
        first_crossing_trial,
        delta,
        n_trials: ones_counts.len(),
    }
}

/// Format an e-value for display.
pub fn format_evalue(e: f64) -> String {
    if e >= 1000.0 {
        format!("{:.0}", e)
    } else if e >= 10.0 {
        format!("{:.1}", e)
    } else {
        format!("{:.3}", e)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evalue_at_null() {
        // At expected value (n/2 ones), e-value should be close to 1
        let e = trial_evalue(100, 200, 0.01);
        // Not exactly 1 because delta != 0, but should be close
        assert!(e > 0.5 && e < 2.0, "e = {e}");
    }

    #[test]
    fn evalue_above_null() {
        // More ones than expected should give e > 1 for positive delta
        let e = trial_evalue(120, 200, 0.05);
        assert!(e > 1.0, "e = {e}");
    }

    #[test]
    fn evalue_below_null() {
        // Fewer ones than expected should give e < 1 for positive delta
        let e = trial_evalue(80, 200, 0.05);
        assert!(e < 1.0, "e = {e}");
    }

    #[test]
    fn evalue_zero_delta() {
        let e = trial_evalue(110, 200, 0.0);
        assert!((e - 1.0).abs() < 1e-10);
    }

    #[test]
    fn evalue_zero_bits() {
        let e = trial_evalue(0, 0, 0.05);
        assert!((e - 1.0).abs() < 1e-10);
    }

    #[test]
    fn wealth_process_monotone_product() {
        let evalues = vec![1.1, 1.2, 0.9, 1.3];
        let wealth = running_eprocess(&evalues);
        assert_eq!(wealth.len(), 4);
        // First element = 1.1
        assert!((wealth[0] - 1.1).abs() < 1e-10);
        // Second = 1.1 * 1.2 = 1.32
        assert!((wealth[1] - 1.32).abs() < 1e-10);
    }

    #[test]
    fn wealth_process_empty() {
        let wealth = running_eprocess(&[]);
        assert!(wealth.is_empty());
    }

    #[test]
    fn threshold_levels() {
        assert_eq!(evalue_threshold(0.5), "no evidence");
        assert_eq!(evalue_threshold(2.0), "anecdotal");
        assert_eq!(evalue_threshold(5.0), "moderate");
        assert_eq!(evalue_threshold(15.0), "strong");
        assert_eq!(evalue_threshold(50.0), "very strong");
        assert_eq!(evalue_threshold(200.0), "decisive");
    }

    #[test]
    fn evalue_to_p_calibration() {
        // e = 20 → p <= 0.05
        let p = evalue_to_approx_p(20.0);
        assert!((p - 0.05).abs() < 1e-10);
        // e = 100 → p <= 0.01
        let p = evalue_to_approx_p(100.0);
        assert!((p - 0.01).abs() < 1e-10);
    }

    #[test]
    fn evalue_to_p_edge_cases() {
        assert_eq!(evalue_to_approx_p(0.0), 1.0);
        assert_eq!(evalue_to_approx_p(-1.0), 1.0);
        assert!(evalue_to_approx_p(1.0) <= 1.0);
    }

    #[test]
    fn sequential_test_null_data() {
        // 100 ones per trial with 200 bits = exactly at null
        let ones = vec![100u32; 20];
        let result = sequential_evalue_test(&ones, 200, 0.01);
        assert_eq!(result.n_trials, 20);
        // Under null, wealth should stay near 1
        assert!(result.final_evalue < 10.0, "e = {}", result.final_evalue);
    }

    #[test]
    fn sequential_test_strong_signal() {
        // 115 ones per trial (7.5% shift) with delta matching
        let ones = vec![115u32; 50];
        let result = sequential_evalue_test(&ones, 200, 0.05);
        // Strong signal should produce large e-value
        assert!(result.final_evalue > 1.0);
        assert_eq!(result.n_trials, 50);
    }

    #[test]
    fn format_evalue_ranges() {
        assert_eq!(format_evalue(0.5), "0.500");
        assert_eq!(format_evalue(15.3), "15.3");
        assert_eq!(format_evalue(1500.0), "1500");
    }
}
