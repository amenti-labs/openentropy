//! Conformal + E-value fusion for doubly-robust sequential monitoring.
//!
//! Combines two independent, anytime-valid statistical frameworks:
//! 1. **Conformal martingales** — distribution-free anomaly detection
//! 2. **E-values** — likelihood-ratio test martingales
//!
//! The fusion provides doubly-robust rejection: evidence is significant if
//! EITHER the conformal martingale OR the e-value wealth process crosses
//! its threshold. This gives broader sensitivity (conformal detects any
//! distributional shift; e-values detect mean shift) while maintaining
//! strict error control via Bonferroni over the two tests.
//!
//! # Theoretical guarantee
//!
//! If we reject when max(M_conformal) >= 1/(2*alpha) OR max(W_evalue) >= 1/(2*alpha),
//! then P(false positive) <= alpha by Bonferroni + Ville's inequality.
//! This is valid under optional stopping for BOTH tests simultaneously.
//!
//! Based on: Vovk et al. (2021) "E-values: Calibration, combination, and
//! applications" and Ramdas et al. (2022) "Testing exchangeability."

use serde::{Deserialize, Serialize};

use crate::consciousness_conformal;
use crate::consciousness_evalue;

/// Result of the fused conformal + e-value sequential test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedResult {
    /// Conformal martingale values.
    pub conformal_martingale: Vec<f64>,
    /// E-value wealth process.
    pub evalue_wealth: Vec<f64>,
    /// Maximum conformal martingale.
    pub max_conformal: f64,
    /// Maximum e-value wealth.
    pub max_evalue: f64,
    /// Conformal threshold (1 / (2 * alpha) for Bonferroni).
    pub conformal_threshold: f64,
    /// E-value threshold (1 / (2 * alpha) for Bonferroni).
    pub evalue_threshold: f64,
    /// Whether conformal martingale rejected.
    pub conformal_rejected: bool,
    /// Whether e-value wealth rejected.
    pub evalue_rejected: bool,
    /// Whether the fused test rejected (either channel).
    pub fused_rejected: bool,
    /// Which channel triggered rejection (if any).
    pub rejection_channel: Option<String>,
    /// First epoch at which rejection occurred (if any).
    pub first_rejection_epoch: Option<usize>,
    /// Overall alpha level (family-wise).
    pub alpha: f64,
    /// Number of epochs tested.
    pub n_epochs: usize,
    /// Evidence summary.
    pub evidence_summary: String,
    /// Interpretation.
    pub interpretation: String,
}

/// Convert conformal p-values to e-values via the calibrator.
///
/// The simplest valid calibrator: e = 1/p (the inverse p-value is always
/// a valid e-value). More sophisticated: e = p^(epsilon-1) * epsilon
/// for epsilon in (0,1), which is the power martingale betting function.
///
/// We use the GROW (Generalized Reversible Optimal Wagering) calibrator
/// from Ramdas et al.: e = kappa * p^(kappa - 1) with kappa chosen to
/// maximize log-wealth growth against the alternative.
pub fn p_to_evalue(p: f64, kappa: f64) -> f64 {
    let p_clamped = p.max(1e-10);
    kappa * p_clamped.powf(kappa - 1.0)
}

/// Compute the e-value wealth process from conformal p-values.
///
/// This bridges conformal prediction into the e-value framework,
/// allowing both to contribute to the same sequential test.
pub fn conformal_p_to_wealth(p_values: &[f64], kappa: f64) -> Vec<f64> {
    let mut wealth = Vec::with_capacity(p_values.len());
    let mut product = 1.0;

    for &p in p_values {
        let e = p_to_evalue(p, kappa);
        product *= e;
        product = product.clamp(1e-30, 1e30);
        wealth.push(product);
    }

    wealth
}

/// Run the fused conformal + e-value sequential test.
///
/// Takes:
/// - `conformal_p_values`: from conformal prediction (distribution-free)
/// - `ones_counts`: per-epoch bit counts for e-value testing (mean-shift)
/// - `n_bits`: bits per epoch
/// - `delta`: effect size for the e-value test (e.g., 0.01)
/// - `alpha`: family-wise error rate
///
/// Returns a `FusedResult` with both channels and the fused decision.
pub fn fused_sequential_test(
    conformal_p_values: &[f64],
    ones_counts: &[u32],
    n_bits: usize,
    delta: f64,
    alpha: f64,
) -> FusedResult {
    let n = conformal_p_values.len().min(ones_counts.len());
    if n == 0 {
        return FusedResult {
            conformal_martingale: Vec::new(),
            evalue_wealth: Vec::new(),
            max_conformal: 0.0,
            max_evalue: 0.0,
            conformal_threshold: 1.0 / alpha,
            evalue_threshold: 1.0 / alpha,
            conformal_rejected: false,
            evalue_rejected: false,
            fused_rejected: false,
            rejection_channel: None,
            first_rejection_epoch: None,
            alpha,
            n_epochs: 0,
            evidence_summary: "No data".to_string(),
            interpretation: "No epochs to test".to_string(),
        };
    }

    // Bonferroni split: each channel gets alpha/2
    let channel_alpha = alpha / 2.0;
    let channel_threshold = 1.0 / channel_alpha;

    // Channel 1: Conformal martingale (distribution-free)
    let conformal_martingale = consciousness_conformal::conformal_martingale(
        &conformal_p_values[..n],
        0.5, // power martingale epsilon
    );

    // Channel 2: E-value wealth process (parametric mean-shift)
    let trial_evalues: Vec<f64> = ones_counts[..n]
        .iter()
        .map(|&ones| consciousness_evalue::trial_evalue(ones, n_bits, delta))
        .collect();
    let evalue_wealth = consciousness_evalue::running_eprocess(&trial_evalues);

    let max_conformal = conformal_martingale
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);
    let max_evalue = evalue_wealth
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);

    let conformal_rejected = max_conformal >= channel_threshold;
    let evalue_rejected = max_evalue >= channel_threshold;
    let fused_rejected = conformal_rejected || evalue_rejected;

    // Determine which channel rejected first
    let conformal_crossing = conformal_martingale
        .iter()
        .position(|&m| m >= channel_threshold);
    let evalue_crossing = evalue_wealth
        .iter()
        .position(|&w| w >= channel_threshold);

    let (rejection_channel, first_rejection_epoch) = match (conformal_crossing, evalue_crossing) {
        (Some(c), Some(e)) => {
            if c <= e {
                (Some("conformal".to_string()), Some(c))
            } else {
                (Some("evalue".to_string()), Some(e))
            }
        }
        (Some(c), None) => (Some("conformal".to_string()), Some(c)),
        (None, Some(e)) => (Some("evalue".to_string()), Some(e)),
        (None, None) => (None, None),
    };

    let evidence_summary = format!(
        "Conformal: {:.2} / {:.1} ({}), E-value: {} / {:.1} ({})",
        max_conformal,
        channel_threshold,
        if conformal_rejected { "REJECT" } else { "retain" },
        consciousness_evalue::format_evalue(max_evalue),
        channel_threshold,
        if evalue_rejected { "REJECT" } else { "retain" },
    );

    let interpretation = if fused_rejected {
        let channel_name = rejection_channel.as_deref().unwrap_or("unknown");
        let epoch = first_rejection_epoch.unwrap_or(0) + 1;
        format!(
            "REJECTED at epoch {} via {} channel (Bonferroni-corrected alpha={:.3}). \
             {}. \
             The fused test detects {} — {} than either test alone.",
            epoch,
            channel_name,
            alpha,
            if conformal_rejected && evalue_rejected {
                "Both channels independently reject H0"
            } else if conformal_rejected {
                "Distributional anomaly detected (conformal) without significant mean shift (e-value)"
            } else {
                "Mean shift detected (e-value) without distributional anomaly (conformal)"
            },
            if conformal_rejected && evalue_rejected {
                "both mean shift and distributional anomaly"
            } else if conformal_rejected {
                "subtle distributional changes"
            } else {
                "directional mean shift"
            },
            if conformal_rejected && evalue_rejected {
                "providing stronger evidence"
            } else {
                "providing broader sensitivity"
            },
        )
    } else {
        format!(
            "No rejection at alpha={:.3} (Bonferroni-corrected). \
             Conformal max={:.2} (need {:.1}), E-value max={} (need {:.1}). \
             Neither distributional shift nor mean shift detected.",
            alpha,
            max_conformal,
            channel_threshold,
            consciousness_evalue::format_evalue(max_evalue),
            channel_threshold,
        )
    };

    FusedResult {
        conformal_martingale,
        evalue_wealth,
        max_conformal,
        max_evalue,
        conformal_threshold: channel_threshold,
        evalue_threshold: channel_threshold,
        conformal_rejected,
        evalue_rejected,
        fused_rejected,
        rejection_channel,
        first_rejection_epoch,
        alpha,
        n_epochs: n,
        evidence_summary,
        interpretation,
    }
}

/// Run the complete fused analysis from raw baseline/intention features.
///
/// This is the high-level entry point that:
/// 1. Calibrates conformal prediction from baseline features
/// 2. Computes conformal p-values for intention features
/// 3. Counts ones in intention byte data
/// 4. Runs the fused sequential test
pub fn fused_analysis(
    baseline_features: &[Vec<f64>],
    intention_features: &[Vec<f64>],
    intention_bytes: &[Vec<u8>],
    n_bits: usize,
    delta: f64,
    alpha: f64,
) -> FusedResult {
    if baseline_features.len() < 3 || intention_features.is_empty() {
        return fused_sequential_test(&[], &[], n_bits, delta, alpha);
    }

    let k = 3.min(baseline_features.len() - 1).max(1);

    // Step 1: Calibrate conformal prediction
    let cal = consciousness_conformal::calibrate(baseline_features, k);

    // Step 2: Compute conformal p-values for intention epochs
    let conformal_p_values: Vec<f64> = intention_features
        .iter()
        .map(|features| {
            let score = consciousness_conformal::nonconformity_score(
                features,
                &cal.baseline_features,
                cal.k,
            );
            consciousness_conformal::conformal_p_value(score, &cal.calibration_scores)
        })
        .collect();

    // Step 3: Count ones in intention byte data
    let ones_counts: Vec<u32> = intention_bytes
        .iter()
        .map(|bytes| crate::consciousness::count_ones_n(bytes, n_bits))
        .collect();

    // Step 4: Run fused test
    fused_sequential_test(&conformal_p_values, &ones_counts, n_bits, delta, alpha)
}

/// Compute the harmonic mean of conformal and e-value evidence.
///
/// The harmonic mean p-value (Wilson 2019) provides a valid combined
/// test without requiring Bonferroni correction. However, it's less
/// powerful than Bonferroni when only one channel detects the signal.
pub fn harmonic_mean_evidence(conformal_max: f64, evalue_max: f64) -> f64 {
    if conformal_max <= 0.0 || evalue_max <= 0.0 {
        return 0.0;
    }
    2.0 / (1.0 / conformal_max + 1.0 / evalue_max)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p_to_evalue_basic() {
        // Low p-value should give high e-value
        let e_low = p_to_evalue(0.01, 0.5);
        let e_high = p_to_evalue(0.5, 0.5);
        assert!(e_low > e_high, "e_low={e_low}, e_high={e_high}");
    }

    #[test]
    fn p_to_evalue_at_one() {
        // p=1 should give e near kappa
        let e = p_to_evalue(1.0, 0.5);
        assert!((e - 0.5).abs() < 1e-10);
    }

    #[test]
    fn p_to_evalue_power_martingale() {
        // E[e(U)] = 1 for uniform p-values (martingale property)
        // Approximate by averaging over grid
        let n = 10000;
        let kappa = 0.5;
        let mean_e: f64 = (1..=n)
            .map(|i| {
                let p = i as f64 / n as f64;
                p_to_evalue(p, kappa)
            })
            .sum::<f64>()
            / n as f64;
        assert!(
            (mean_e - 1.0).abs() < 0.05,
            "mean_e = {mean_e} (should be ~1.0)"
        );
    }

    #[test]
    fn conformal_p_to_wealth_grows_on_anomaly() {
        // Low p-values should make wealth grow
        let p_values = vec![0.01, 0.02, 0.01, 0.03, 0.01];
        let wealth = conformal_p_to_wealth(&p_values, 0.5);
        assert!(wealth.last().unwrap() > &1.0);
    }

    #[test]
    fn conformal_p_to_wealth_stable_on_null() {
        // High p-values should keep wealth near 1
        let p_values = vec![0.5, 0.6, 0.4, 0.55, 0.45];
        let wealth = conformal_p_to_wealth(&p_values, 0.5);
        assert!(wealth.last().unwrap() < &10.0);
    }

    #[test]
    fn fused_test_null_data() {
        // Null data: high p-values, 50% ones
        let p_values = vec![0.5, 0.6, 0.4, 0.55, 0.45, 0.5, 0.5, 0.5];
        let ones = vec![100u32; 8]; // exactly at null for 200 bits
        let result = fused_sequential_test(&p_values, &ones, 200, 0.01, 0.05);

        assert_eq!(result.n_epochs, 8);
        assert!(!result.fused_rejected);
        assert!(result.rejection_channel.is_none());
    }

    #[test]
    fn fused_test_conformal_anomaly() {
        // Very low conformal p-values (distributional anomaly)
        let p_values = vec![0.001, 0.002, 0.001, 0.001, 0.002, 0.001, 0.001, 0.001];
        let ones = vec![100u32; 8]; // no mean shift
        let result = fused_sequential_test(&p_values, &ones, 200, 0.01, 0.05);

        // Conformal should detect, e-value should not
        assert!(result.max_conformal > result.max_evalue);
        // May or may not cross threshold depending on power
    }

    #[test]
    fn fused_test_mean_shift() {
        // High ones count (mean shift) but normal conformal p-values
        let p_values = vec![0.5; 20];
        let ones = vec![115u32; 20]; // 7.5% shift for 200 bits
        let result = fused_sequential_test(&p_values, &ones, 200, 0.05, 0.05);

        // E-value should grow, conformal should stay low
        assert!(result.max_evalue > 1.0);
    }

    #[test]
    fn fused_test_empty() {
        let result = fused_sequential_test(&[], &[], 200, 0.01, 0.05);
        assert_eq!(result.n_epochs, 0);
        assert!(!result.fused_rejected);
    }

    #[test]
    fn fused_test_bonferroni_threshold() {
        let result = fused_sequential_test(&[0.5], &[100], 200, 0.01, 0.05);
        // Bonferroni: each channel gets alpha/2 = 0.025, threshold = 1/0.025 = 40
        assert!((result.conformal_threshold - 40.0).abs() < 1e-10);
        assert!((result.evalue_threshold - 40.0).abs() < 1e-10);
    }

    #[test]
    fn harmonic_mean_basic() {
        let hm = harmonic_mean_evidence(10.0, 10.0);
        assert!((hm - 10.0).abs() < 1e-10);
    }

    #[test]
    fn harmonic_mean_asymmetric() {
        // If one channel is much stronger, harmonic mean is pulled down
        let hm = harmonic_mean_evidence(100.0, 1.0);
        assert!(hm < 2.0, "hm = {hm}");
    }

    #[test]
    fn harmonic_mean_zero() {
        assert_eq!(harmonic_mean_evidence(0.0, 10.0), 0.0);
    }

    #[test]
    fn fused_analysis_empty_baseline() {
        let result = fused_analysis(&[], &[vec![1.0, 2.0]], &[vec![128u8; 25]], 200, 0.01, 0.05);
        assert_eq!(result.n_epochs, 0);
    }

    #[test]
    fn fused_result_serializable() {
        let result = fused_sequential_test(&[0.5, 0.3], &[100, 105], 200, 0.01, 0.05);
        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }
}
