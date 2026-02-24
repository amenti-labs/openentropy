//! Decision Augmentation Theory (DAT) vs Force model testing.
//!
//! The Force model (Schmidt, 1987) proposes that consciousness directly shifts
//! the distribution mean (p -> p + delta). DAT (May, Utts & Spottiswoode, 1995)
//! proposes that operators unconsciously select WHEN to start/stop experiments,
//! biasing which pre-existing fluctuations get sampled.
//!
//! Force predicts: mean shift, no excess kurtosis, no temporal clustering.
//! DAT predicts: excess kurtosis (fat tails), temporal clustering at experiment
//! start, no true mean shift (distribution shape changes instead).
//!
//! May et al. (1995) showed DAT fit PEAR data better by 8.6 sigma.
//! No modern platform tests this. OpenEntropy does.

use serde::{Deserialize, Serialize};

/// Per-trial data needed for DAT analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialData {
    /// Number of 1-bits observed.
    pub ones: u32,
    /// Total bits in this trial.
    pub n_bits: usize,
    /// Trial index within phase.
    pub trial_index: usize,
    /// Whether this was a "successful" trial (ones > n/2 for High, < n/2 for Low).
    pub success: bool,
}

/// Result of DAT vs Force model comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DATResult {
    /// Log-likelihood under Force model (mean shift).
    pub force_log_likelihood: f64,
    /// Log-likelihood under DAT model (selection bias).
    pub dat_log_likelihood: f64,
    /// Likelihood ratio (positive favors DAT).
    pub log_likelihood_ratio: f64,
    /// BIC for Force model.
    pub force_bic: f64,
    /// BIC for DAT model.
    pub dat_bic: f64,
    /// Preferred model based on BIC.
    pub preferred_model: String,
    /// Distributional diagnostics.
    pub diagnostics: DistributionalDiags,
    /// Temporal clustering result.
    pub clustering: ClusteringResult,
    /// Interpretation.
    pub interpretation: String,
}

/// Distributional diagnostics to distinguish Force from DAT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionalDiags {
    /// Sample mean of Z-scores.
    pub mean_z: f64,
    /// Sample variance of Z-scores.
    pub variance_z: f64,
    /// Excess kurtosis (0 for normal, >0 for fat tails).
    pub excess_kurtosis: f64,
    /// Skewness.
    pub skewness: f64,
    /// Number of trials in tails (|Z| > 2).
    pub tail_count: usize,
    /// Expected tail count under Normal.
    pub expected_tail_count: f64,
    /// Tail ratio (observed / expected).
    pub tail_ratio: f64,
}

/// Temporal clustering test for DAT prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringResult {
    /// Fraction of successes in first quarter.
    pub first_quarter_success_rate: f64,
    /// Fraction of successes in remaining quarters.
    pub remaining_success_rate: f64,
    /// Z-score comparing first quarter to rest.
    pub clustering_z: f64,
    /// P-value for clustering.
    pub clustering_p: f64,
    /// Whether clustering is significant (p < 0.05).
    pub is_clustered: bool,
}

/// Compute log-likelihood under the Force model.
///
/// Force model: each trial drawn from Binomial(n, 0.5 + delta).
/// MLE for delta: (mean_ones / n) - 0.5.
pub fn force_model_likelihood(trials: &[TrialData], delta: f64) -> f64 {
    let mut log_lik = 0.0;
    let p = 0.5 + delta;

    if p <= 0.0 || p >= 1.0 {
        return f64::NEG_INFINITY;
    }

    for trial in trials {
        let k = trial.ones as f64;
        let n = trial.n_bits as f64;
        // log P(k | n, p) = k*log(p) + (n-k)*log(1-p) + log(C(n,k))
        // We skip the binomial coefficient since it cancels in likelihood ratio
        log_lik += k * p.ln() + (n - k) * (1.0 - p).ln();
    }

    log_lik
}

/// Compute log-likelihood under the DAT model.
///
/// DAT model: trials are drawn from Binomial(n, 0.5) but with selection bias.
/// Successful trials (matching intention) are over-represented by factor (1 + selection_bias).
/// This models the idea that the operator chose to start/stop at favorable moments.
///
/// Simplified: P(trial_i) proportional to (0.5^n) * w_i, where w_i = 1 + selection_bias
/// if the trial is "successful" and w_i = 1 otherwise.
pub fn dat_model_likelihood(trials: &[TrialData], selection_bias: f64) -> f64 {
    let mut log_lik = 0.0;

    for trial in trials {
        let k = trial.ones as f64;
        let n = trial.n_bits as f64;
        // Base probability under Binomial(n, 0.5)
        log_lik += k * 0.5_f64.ln() + (n - k) * 0.5_f64.ln();
        // Selection weight
        let weight = if trial.success {
            1.0 + selection_bias
        } else {
            1.0
        };
        log_lik += weight.ln();
    }

    log_lik
}

/// Run the full DAT vs Force model likelihood ratio test.
///
/// Estimates MLE parameters for both models and compares using
/// likelihood ratio and BIC.
pub fn likelihood_ratio_test(trials: &[TrialData]) -> DATResult {
    if trials.is_empty() {
        return DATResult {
            force_log_likelihood: 0.0,
            dat_log_likelihood: 0.0,
            log_likelihood_ratio: 0.0,
            force_bic: 0.0,
            dat_bic: 0.0,
            preferred_model: "insufficient data".to_string(),
            diagnostics: distributional_diagnostics(trials),
            clustering: temporal_clustering_test(trials),
            interpretation: "No trials provided".to_string(),
        };
    }

    let n = trials.len() as f64;
    let n_bits = trials[0].n_bits;

    // Force model MLE: delta = mean_proportion - 0.5
    let mean_ones: f64 =
        trials.iter().map(|t| t.ones as f64).sum::<f64>() / n;
    let delta_mle = (mean_ones / n_bits as f64) - 0.5;
    let force_ll = force_model_likelihood(trials, delta_mle);

    // DAT model MLE: grid search for selection_bias
    let success_rate = trials.iter().filter(|t| t.success).count() as f64 / n;
    let selection_bias_mle = (success_rate / 0.5 - 1.0).max(0.0);
    let dat_ll = dat_model_likelihood(trials, selection_bias_mle);

    let log_lr = dat_ll - force_ll;

    // BIC: -2*LL + k*log(n), where k = number of parameters
    let force_bic = -2.0 * force_ll + 1.0 * n.ln(); // 1 param: delta
    let dat_bic = -2.0 * dat_ll + 1.0 * n.ln(); // 1 param: selection_bias

    let preferred_model = if dat_bic < force_bic {
        "DAT (selection)".to_string()
    } else {
        "Force (mean shift)".to_string()
    };

    let diagnostics = distributional_diagnostics(trials);
    let clustering = temporal_clustering_test(trials);

    // Build interpretation from multiple lines of evidence
    let mut evidence_for_dat = 0i32;
    let mut evidence_for_force = 0i32;

    if diagnostics.excess_kurtosis > 0.5 {
        evidence_for_dat += 1;
    } else {
        evidence_for_force += 1;
    }

    if clustering.is_clustered {
        evidence_for_dat += 1;
    } else {
        evidence_for_force += 1;
    }

    if dat_bic < force_bic {
        evidence_for_dat += 1;
    } else {
        evidence_for_force += 1;
    }

    if diagnostics.tail_ratio > 1.5 {
        evidence_for_dat += 1;
    }

    let interpretation = if evidence_for_dat > evidence_for_force {
        format!(
            "DAT preferred ({} of {} indicators): excess kurtosis={:.2}, \
             temporal clustering p={:.4}, BIC advantage={:.1}. \
             Suggests operator selected favorable moments rather than \
             directly shifting the distribution.",
            evidence_for_dat,
            evidence_for_dat + evidence_for_force,
            diagnostics.excess_kurtosis,
            clustering.clustering_p,
            force_bic - dat_bic
        )
    } else if evidence_for_force > evidence_for_dat {
        format!(
            "Force model preferred ({} of {} indicators): \
             normal kurtosis={:.2}, no temporal clustering. \
             Consistent with direct mean shift mechanism.",
            evidence_for_force,
            evidence_for_dat + evidence_for_force,
            diagnostics.excess_kurtosis,
        )
    } else {
        "Inconclusive — equal evidence for both models. More data needed.".to_string()
    };

    DATResult {
        force_log_likelihood: force_ll,
        dat_log_likelihood: dat_ll,
        log_likelihood_ratio: log_lr,
        force_bic,
        dat_bic,
        preferred_model,
        diagnostics,
        clustering,
        interpretation,
    }
}

/// Compute distributional diagnostics (kurtosis, skewness, tail behavior).
pub fn distributional_diagnostics(trials: &[TrialData]) -> DistributionalDiags {
    if trials.is_empty() {
        return DistributionalDiags {
            mean_z: 0.0,
            variance_z: 0.0,
            excess_kurtosis: 0.0,
            skewness: 0.0,
            tail_count: 0,
            expected_tail_count: 0.0,
            tail_ratio: 0.0,
        };
    }

    let z_scores: Vec<f64> = trials
        .iter()
        .map(|t| crate::consciousness::trial_z_score(t.ones, t.n_bits))
        .collect();

    let n = z_scores.len() as f64;
    let mean = z_scores.iter().sum::<f64>() / n;
    let variance = z_scores.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / n;
    let sd = variance.sqrt().max(1e-10);

    // Skewness = E[(X-mu)^3] / sigma^3
    let skewness = z_scores
        .iter()
        .map(|z| ((z - mean) / sd).powi(3))
        .sum::<f64>()
        / n;

    // Excess kurtosis = E[(X-mu)^4] / sigma^4 - 3
    let kurtosis = z_scores
        .iter()
        .map(|z| ((z - mean) / sd).powi(4))
        .sum::<f64>()
        / n
        - 3.0;

    let tail_count = z_scores.iter().filter(|z| z.abs() > 2.0).count();
    let expected_tail_count = n * 2.0 * 0.0228; // P(|Z| > 2) under normal
    let tail_ratio = if expected_tail_count > 0.0 {
        tail_count as f64 / expected_tail_count
    } else {
        0.0
    };

    DistributionalDiags {
        mean_z: mean,
        variance_z: variance,
        excess_kurtosis: kurtosis,
        skewness,
        tail_count,
        expected_tail_count,
        tail_ratio,
    }
}

/// Test whether successful trials cluster at the beginning of the experiment.
///
/// DAT predicts that operators choose to start at favorable moments, so
/// successes should be concentrated in the first trials. Force predicts
/// uniform distribution of successes across trials.
pub fn temporal_clustering_test(trials: &[TrialData]) -> ClusteringResult {
    if trials.len() < 4 {
        return ClusteringResult {
            first_quarter_success_rate: 0.0,
            remaining_success_rate: 0.0,
            clustering_z: 0.0,
            clustering_p: 1.0,
            is_clustered: false,
        };
    }

    let quarter = trials.len() / 4;
    let first_q = &trials[..quarter];
    let rest = &trials[quarter..];

    let first_successes = first_q.iter().filter(|t| t.success).count() as f64;
    let rest_successes = rest.iter().filter(|t| t.success).count() as f64;

    let first_rate = first_successes / first_q.len() as f64;
    let rest_rate = rest_successes / rest.len() as f64;

    // Z-test comparing two proportions
    let n1 = first_q.len() as f64;
    let n2 = rest.len() as f64;
    let p_hat = (first_successes + rest_successes) / (n1 + n2);

    let se = if p_hat > 0.0 && p_hat < 1.0 {
        (p_hat * (1.0 - p_hat) * (1.0 / n1 + 1.0 / n2)).sqrt()
    } else {
        1.0
    };

    let z = if se > 1e-10 {
        (first_rate - rest_rate) / se
    } else {
        0.0
    };

    let p = crate::consciousness::z_to_p_one_tailed(z);

    ClusteringResult {
        first_quarter_success_rate: first_rate,
        remaining_success_rate: rest_rate,
        clustering_z: z,
        clustering_p: p,
        is_clustered: p < 0.05 && first_rate > rest_rate,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_null_trials(n: usize) -> Vec<TrialData> {
        // Trials at exactly null expectation
        (0..n)
            .map(|i| TrialData {
                ones: 100,
                n_bits: 200,
                trial_index: i,
                success: false, // exactly at midpoint
            })
            .collect()
    }

    #[test]
    fn force_likelihood_at_null() {
        let trials = make_null_trials(20);
        let ll = force_model_likelihood(&trials, 0.0);
        assert!(ll.is_finite());
    }

    #[test]
    fn force_likelihood_with_shift() {
        let trials: Vec<TrialData> = (0..20)
            .map(|i| TrialData {
                ones: 110, // shifted up
                n_bits: 200,
                trial_index: i,
                success: true,
            })
            .collect();
        let ll_shift = force_model_likelihood(&trials, 0.05);
        let ll_null = force_model_likelihood(&trials, 0.0);
        // Should fit better with shift matching the data
        assert!(ll_shift > ll_null);
    }

    #[test]
    fn dat_likelihood_with_selection() {
        let trials: Vec<TrialData> = (0..20)
            .map(|i| TrialData {
                ones: if i % 2 == 0 { 110 } else { 90 },
                n_bits: 200,
                trial_index: i,
                success: i % 2 == 0,
            })
            .collect();
        let ll_sel = dat_model_likelihood(&trials, 0.5);
        let ll_none = dat_model_likelihood(&trials, 0.0);
        // Selection bias should increase likelihood for successful trials
        assert!(ll_sel > ll_none);
    }

    #[test]
    fn distributional_diags_null() {
        let trials = make_null_trials(50);
        let diags = distributional_diagnostics(&trials);
        // All Z-scores are 0, so everything should be 0
        assert!((diags.mean_z - 0.0).abs() < 1e-10);
        assert!((diags.variance_z - 0.0).abs() < 1e-10);
    }

    #[test]
    fn distributional_diags_fat_tails() {
        // Create data with excess kurtosis
        let mut trials = Vec::new();
        for i in 0..100 {
            let ones = if i < 5 || i >= 95 {
                130 // extreme values
            } else {
                100 // normal values
            };
            trials.push(TrialData {
                ones,
                n_bits: 200,
                trial_index: i,
                success: ones > 100,
            });
        }
        let diags = distributional_diagnostics(&trials);
        assert!(diags.excess_kurtosis > 0.0, "kurt = {}", diags.excess_kurtosis);
    }

    #[test]
    fn temporal_clustering_none() {
        // Uniform success distribution
        let trials: Vec<TrialData> = (0..40)
            .map(|i| TrialData {
                ones: if i % 2 == 0 { 105 } else { 95 },
                n_bits: 200,
                trial_index: i,
                success: i % 2 == 0,
            })
            .collect();
        let result = temporal_clustering_test(&trials);
        assert!(!result.is_clustered);
    }

    #[test]
    fn temporal_clustering_present() {
        // Heavy success clustering in first quarter
        let trials: Vec<TrialData> = (0..40)
            .map(|i| TrialData {
                ones: if i < 10 { 115 } else { 100 },
                n_bits: 200,
                trial_index: i,
                success: i < 10,
            })
            .collect();
        let result = temporal_clustering_test(&trials);
        assert!(result.first_quarter_success_rate > result.remaining_success_rate);
    }

    #[test]
    fn temporal_clustering_too_few() {
        let trials = make_null_trials(3);
        let result = temporal_clustering_test(&trials);
        assert_eq!(result.clustering_p, 1.0);
    }

    #[test]
    fn likelihood_ratio_test_empty() {
        let result = likelihood_ratio_test(&[]);
        assert_eq!(result.preferred_model, "insufficient data");
    }

    #[test]
    fn likelihood_ratio_test_basic() {
        let trials: Vec<TrialData> = (0..50)
            .map(|i| TrialData {
                ones: 100 + (i % 5) as u32,
                n_bits: 200,
                trial_index: i,
                success: (i % 5) > 0,
            })
            .collect();
        let result = likelihood_ratio_test(&trials);
        assert!(!result.preferred_model.is_empty());
        assert!(!result.interpretation.is_empty());
        assert!(result.force_bic.is_finite());
        assert!(result.dat_bic.is_finite());
    }
}
