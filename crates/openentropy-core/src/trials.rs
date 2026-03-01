//! PEAR-style trial analysis for entropy data.
//!
//! The PEAR Lab (Princeton Engineering Anomalies Research) used 200-bit trials
//! at ~1/sec with cumulative deviation tracking, terminal Z-scores, and effect
//! sizes at ~10^-4. This module provides those metrics on raw byte streams so
//! recorded sessions can be analyzed with PEAR-standard statistics.
//! Historical references and formulas are documented in `docs/TRIALS.md`.
//!
//! All functions are pure: `&[u8]` in, structs out. No I/O.

use serde::{Deserialize, Serialize};

use crate::analysis;
use crate::conditioning::quick_shannon;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration for trial slicing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialConfig {
    /// Bits per trial (must be divisible by 8). Default: 200.
    pub bits_per_trial: usize,
}

impl Default for TrialConfig {
    fn default() -> Self {
        Self {
            bits_per_trial: 200,
        }
    }
}

/// A single trial result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    /// Zero-based trial index.
    pub index: usize,
    /// Number of 1-bits in this trial.
    pub ones_count: u32,
    /// Z-score for this trial: (ones - N/2) / sqrt(N/4).
    pub z_score: f64,
    /// Running cumulative deviation: sum of (ones_i - N/2) up to this trial.
    pub cumulative_deviation: f64,
}

/// Complete trial analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialAnalysis {
    /// Config used for this analysis.
    pub config: TrialConfig,
    /// Total bytes consumed from input.
    pub bytes_consumed: usize,
    /// Number of complete trials extracted.
    pub num_trials: usize,
    /// Bits per trial (copied from config for convenience).
    pub bits_per_trial: usize,
    /// Per-trial results.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trials: Vec<Trial>,
    /// Final cumulative deviation after all trials.
    pub terminal_cumulative_deviation: f64,
    /// Terminal Z-score: cum_dev / sqrt(num_trials * N/4).
    pub terminal_z: f64,
    /// Effect size: terminal_z / sqrt(num_trials).
    pub effect_size: f64,
    /// Mean of per-trial Z-scores (should be ~0 for unbiased data).
    pub mean_z: f64,
    /// Std deviation of per-trial Z-scores (should be ~1).
    pub std_z: f64,
    /// Two-tailed p-value from the terminal Z-score.
    pub terminal_p_value: f64,
}

/// Result of combining multiple sessions via Stouffer's method.
#[derive(Debug, Clone, Serialize)]
pub struct StoufferResult {
    /// Number of sessions combined (non-empty trial analyses only).
    pub num_sessions: usize,
    /// Terminal Z-score from each session.
    pub session_z_scores: Vec<f64>,
    /// Combined Z using weighted Stouffer:
    /// sum(w_i * Z_i) / sqrt(sum(w_i^2)), where w_i = sqrt(num_trials_i).
    pub stouffer_z: f64,
    /// Two-tailed p-value from the combined Z.
    pub p_value: f64,
    /// Combined effect size: stouffer_z / sqrt(total_trials).
    pub combined_effect_size: f64,
    /// Total trials across all sessions.
    pub total_trials: usize,
}

/// Calibration assessment for a data source.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationResult {
    /// Trial analysis of the calibration data.
    pub analysis: TrialAnalysis,
    /// Whether this source passed calibration.
    pub is_suitable: bool,
    /// Any warnings or failure reasons.
    pub warnings: Vec<String>,
    /// Shannon entropy of the calibration data (bits/byte, max 8.0).
    pub shannon_entropy: f64,
    /// Overall bit bias (deviation from 0.5).
    pub bit_bias: f64,
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// PEAR-style trial analysis on raw bytes.
///
/// Slices `data` into trials of `config.bits_per_trial` bits each.
/// Requires `bits_per_trial % 8 == 0`.
///
/// Returns a `TrialAnalysis` with per-trial Z-scores, cumulative deviation,
/// terminal Z, effect size, and p-value.
pub fn trial_analysis(data: &[u8], config: &TrialConfig) -> TrialAnalysis {
    assert!(
        config.bits_per_trial > 0 && config.bits_per_trial.is_multiple_of(8),
        "bits_per_trial must be a positive multiple of 8"
    );

    let bytes_per_trial = config.bits_per_trial / 8;
    let num_trials = data.len() / bytes_per_trial;
    let bytes_consumed = num_trials * bytes_per_trial;

    let n = config.bits_per_trial as f64;
    let expected = n / 2.0;
    let std_dev = (n / 4.0).sqrt(); // sqrt(N * p * (1-p)) = sqrt(N/4)

    let mut trials = Vec::with_capacity(num_trials);
    let mut cum_dev = 0.0;

    for i in 0..num_trials {
        let start = i * bytes_per_trial;
        let end = start + bytes_per_trial;
        let chunk = &data[start..end];

        let ones: u32 = chunk.iter().map(|b| b.count_ones()).sum();
        let deviation = ones as f64 - expected;
        cum_dev += deviation;

        let z = if std_dev > 0.0 {
            deviation / std_dev
        } else {
            0.0
        };

        trials.push(Trial {
            index: i,
            ones_count: ones,
            z_score: z,
            cumulative_deviation: cum_dev,
        });
    }

    // Terminal Z: cumulative deviation normalized by sqrt(num_trials * N/4)
    let terminal_z = if num_trials > 0 && std_dev > 0.0 {
        cum_dev / (std_dev * (num_trials as f64).sqrt())
    } else {
        0.0
    };

    // Effect size: terminal_z / sqrt(num_trials)
    let effect_size = if num_trials > 0 {
        terminal_z / (num_trials as f64).sqrt()
    } else {
        0.0
    };

    // Z-score statistics
    let (mean_z, std_z) = if !trials.is_empty() {
        let sum: f64 = trials.iter().map(|t| t.z_score).sum();
        let mean = sum / trials.len() as f64;
        let var: f64 = trials
            .iter()
            .map(|t| (t.z_score - mean).powi(2))
            .sum::<f64>()
            / trials.len() as f64;
        (mean, var.sqrt())
    } else {
        (0.0, 0.0)
    };

    let terminal_p_value = 2.0 * (1.0 - normal_cdf(terminal_z.abs()));

    TrialAnalysis {
        config: config.clone(),
        bytes_consumed,
        num_trials,
        bits_per_trial: config.bits_per_trial,
        trials,
        terminal_cumulative_deviation: cum_dev,
        terminal_z,
        effect_size,
        mean_z,
        std_z,
        terminal_p_value,
    }
}

/// Combine terminal Z-scores from multiple sessions using weighted Stouffer's method.
///
/// Uses weights `w_i = sqrt(num_trials_i)`, which is the natural weighting for
/// per-session terminal Z values derived from trial counts.
///
/// Sessions with zero trials are ignored.
pub fn stouffer_combine(analyses: &[&TrialAnalysis]) -> StoufferResult {
    let contributing: Vec<&TrialAnalysis> = analyses
        .iter()
        .copied()
        .filter(|a| a.num_trials > 0)
        .collect();
    let k = contributing.len();
    let session_z_scores: Vec<f64> = contributing.iter().map(|a| a.terminal_z).collect();
    let total_trials: usize = contributing.iter().map(|a| a.num_trials).sum();

    let weighted_z_sum: f64 = contributing
        .iter()
        .map(|a| a.terminal_z * (a.num_trials as f64).sqrt())
        .sum();
    let stouffer_z = if total_trials > 0 {
        weighted_z_sum / (total_trials as f64).sqrt()
    } else {
        0.0
    };

    let p_value = 2.0 * (1.0 - normal_cdf(stouffer_z.abs()));

    let combined_effect_size = if total_trials > 0 {
        stouffer_z / (total_trials as f64).sqrt()
    } else {
        0.0
    };

    StoufferResult {
        num_sessions: k,
        session_z_scores,
        stouffer_z,
        p_value,
        combined_effect_size,
        total_trials,
    }
}

/// Assess whether data is suitable as a calibration baseline.
///
/// Checks:
/// - `|terminal_z| < 2.0` (no significant deviation)
/// - `bit_bias < 0.005` (negligible bias)
/// - `shannon_entropy > 7.9` (near-maximal entropy)
/// - `std_z` in `[0.85, 1.15]` (Z-scores normally distributed)
pub fn calibration_check(data: &[u8], config: &TrialConfig) -> CalibrationResult {
    let analysis = trial_analysis(data, config);
    let shannon = quick_shannon(data);
    let bias = analysis::bit_bias(data).overall_bias;

    let mut warnings = Vec::new();
    let mut suitable = true;

    if analysis.terminal_z.abs() >= 2.0 {
        warnings.push(format!(
            "Terminal Z = {:.4} exceeds +/-2.0 threshold",
            analysis.terminal_z
        ));
        suitable = false;
    }

    if bias >= 0.005 {
        warnings.push(format!("Bit bias = {:.6} exceeds 0.005 threshold", bias));
        suitable = false;
    }

    if shannon <= 7.9 {
        warnings.push(format!(
            "Shannon entropy = {:.4} below 7.9 threshold",
            shannon
        ));
        suitable = false;
    }

    if analysis.num_trials > 1 && (analysis.std_z < 0.85 || analysis.std_z > 1.15) {
        warnings.push(format!(
            "Z-score std = {:.4} outside [0.85, 1.15] range",
            analysis.std_z
        ));
        suitable = false;
    }

    if analysis.num_trials == 0 {
        warnings.push("Insufficient data for any trials".to_string());
        suitable = false;
    }

    CalibrationResult {
        analysis,
        is_suitable: suitable,
        warnings,
        shannon_entropy: shannon,
        bit_bias: bias,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Standard normal CDF using the Abramowitz & Stegun approximation (26.2.17).
///
/// Accurate to ~7.5 x 10^-8 for all x.
fn normal_cdf(x: f64) -> f64 {
    if x.is_nan() {
        return 0.5;
    }

    // Constants from A&S 26.2.17
    const B1: f64 = 0.319381530;
    const B2: f64 = -0.356563782;
    const B3: f64 = 1.781477937;
    const B4: f64 = -1.821255978;
    const B5: f64 = 1.330274429;
    const P: f64 = 0.2316419;

    let abs_x = x.abs();
    let t = 1.0 / (1.0 + P * abs_x);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;

    let pdf = (-0.5 * abs_x * abs_x).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let cdf = 1.0 - pdf * (B1 * t + B2 * t2 + B3 * t3 + B4 * t4 + B5 * t5);

    if x >= 0.0 {
        cdf
    } else {
        1.0 - cdf
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_ones() {
        // 50 bytes of 0xFF = 400 bits, all ones → 2 trials of 200 bits
        let data = vec![0xFF; 50];
        let config = TrialConfig::default(); // 200 bits
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 2);
        assert_eq!(result.bits_per_trial, 200);

        // Each trial: 200 ones, expected 100, z = (200-100)/sqrt(50) = 100/7.07 ≈ 14.14
        for trial in &result.trials {
            assert_eq!(trial.ones_count, 200);
            assert!(trial.z_score > 14.0);
        }
        assert!(result.terminal_z > 10.0);
    }

    #[test]
    fn test_alternating_bits() {
        // 0xAA = 10101010 → 4 ones per byte, so 100 ones per 25-byte trial
        // That's exactly 50% → Z ≈ 0
        let data = vec![0xAA; 50];
        let config = TrialConfig::default();
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 2);
        for trial in &result.trials {
            assert_eq!(trial.ones_count, 100); // exactly 50%
            assert!(trial.z_score.abs() < 0.001);
        }
        assert!(result.terminal_z.abs() < 0.001);
    }

    #[test]
    fn test_pseudo_random() {
        // Use a simple PRNG for reproducibility
        let mut state: u64 = 42;
        let data: Vec<u8> = (0..2500)
            .map(|_| {
                // xorshift64
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & 0xFF) as u8
            })
            .collect();

        let config = TrialConfig::default();
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 100); // 2500 / 25 = 100 trials
                                            // For pseudo-random data, Z should be small and effect size tiny
        assert!(
            result.terminal_z.abs() < 4.0,
            "terminal_z={} too large for PRNG",
            result.terminal_z
        );
        assert!(
            result.effect_size.abs() < 0.5,
            "effect_size={} too large",
            result.effect_size
        );
        // std_z should be near 1.0
        assert!(
            result.std_z > 0.7 && result.std_z < 1.3,
            "std_z={} not near 1.0",
            result.std_z
        );
    }

    #[test]
    fn test_stouffer_combine() {
        // Create three analyses with known terminal Z-scores
        let config = TrialConfig::default();
        let data = vec![0xAA; 50]; // 2 trials, Z ≈ 0

        let a1 = trial_analysis(&data, &config);
        let a2 = trial_analysis(&data, &config);
        let a3 = trial_analysis(&data, &config);

        let result = stouffer_combine(&[&a1, &a2, &a3]);
        assert_eq!(result.num_sessions, 3);
        assert_eq!(result.total_trials, 6);
        // All Z ≈ 0, so Stouffer Z ≈ 0
        assert!(result.stouffer_z.abs() < 0.01);
    }

    #[test]
    fn test_stouffer_scaling() {
        // With equal trial counts, weighted Stouffer reduces to sum/sqrt(k).
        let config = TrialConfig::default();

        // Create biased data that gives consistent positive Z
        let data = vec![0xFF; 25]; // 1 trial, all ones, large Z
        let a = trial_analysis(&data, &config);
        let z = a.terminal_z;

        // Combine 4 identical sessions
        let result = stouffer_combine(&[&a, &a, &a, &a]);
        // stouffer_z = 4*z / sqrt(4) = 2*z
        let expected = 4.0 * z / 2.0;
        assert!(
            (result.stouffer_z - expected).abs() < 0.001,
            "stouffer_z={} expected={}",
            result.stouffer_z,
            expected
        );
    }

    #[test]
    fn test_stouffer_weighted_by_trial_count() {
        let config = TrialConfig::default();

        // 1 trial with strong positive Z.
        let short = trial_analysis(&[0xFF; 25], &config);
        // 100 trials with ~zero Z.
        let long = trial_analysis(&[0xAA; 2500], &config);

        let result = stouffer_combine(&[&short, &long]);
        assert_eq!(result.num_sessions, 2);
        assert_eq!(result.total_trials, 101);

        let expected = (short.terminal_z * (short.num_trials as f64).sqrt()
            + long.terminal_z * (long.num_trials as f64).sqrt())
            / (result.total_trials as f64).sqrt();
        assert!((result.stouffer_z - expected).abs() < 1e-10);

        // Weighted by trial count: one short outlier should be diluted by the long run.
        assert!(result.stouffer_z.abs() < short.terminal_z.abs());
    }

    #[test]
    fn test_stouffer_ignores_zero_trial_sessions() {
        let config = TrialConfig::default();
        let empty = trial_analysis(&[], &config);
        let non_empty = trial_analysis(&[0xFF; 25], &config);

        let result = stouffer_combine(&[&empty, &non_empty]);
        assert_eq!(result.num_sessions, 1);
        assert_eq!(result.total_trials, 1);
        assert_eq!(result.session_z_scores.len(), 1);
        assert!((result.stouffer_z - non_empty.terminal_z).abs() < 1e-10);
    }

    #[test]
    fn test_calibration_pass() {
        // Pseudo-random data should pass calibration — use enough data to reduce bias
        let mut state: u64 = 12345;
        let data: Vec<u8> = (0..50_000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & 0xFF) as u8
            })
            .collect();

        let config = TrialConfig::default();
        let result = calibration_check(&data, &config);
        assert!(
            result.is_suitable,
            "PRNG data should pass calibration, warnings: {:?}",
            result.warnings
        );
        assert!(result.warnings.is_empty());
        assert!(result.shannon_entropy > 7.9);
        assert!(result.bit_bias < 0.005);
    }

    #[test]
    fn test_calibration_fail_biased() {
        // Heavily biased data (all 0xFF) should fail
        let data = vec![0xFF; 5000];
        let config = TrialConfig::default();
        let result = calibration_check(&data, &config);

        assert!(!result.is_suitable);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_normal_cdf_known_values() {
        // CDF(0) = 0.5
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-6);

        // CDF(1.96) ≈ 0.975
        assert!((normal_cdf(1.96) - 0.975).abs() < 0.001);

        // CDF(-1.96) ≈ 0.025
        assert!((normal_cdf(-1.96) - 0.025).abs() < 0.001);

        // CDF(3.0) ≈ 0.99865
        assert!((normal_cdf(3.0) - 0.99865).abs() < 0.001);

        // Symmetry
        assert!((normal_cdf(1.0) + normal_cdf(-1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_insufficient_data() {
        // Less than 25 bytes → 0 trials
        let data = vec![0x42; 10];
        let config = TrialConfig::default();
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 0);
        assert_eq!(result.bytes_consumed, 0);
        assert!(result.trials.is_empty());
        assert_eq!(result.terminal_z, 0.0);
        assert_eq!(result.effect_size, 0.0);
    }

    #[test]
    fn test_empty_data() {
        let config = TrialConfig::default();
        let result = trial_analysis(&[], &config);
        assert_eq!(result.num_trials, 0);
        assert_eq!(result.terminal_z, 0.0);
    }

    #[test]
    fn test_custom_bits_per_trial() {
        // 8 bits per trial = 1 byte per trial
        let config = TrialConfig { bits_per_trial: 8 };
        let data = vec![0xFF; 10]; // 10 trials
        let result = trial_analysis(&data, &config);
        assert_eq!(result.num_trials, 10);

        // Each trial: 8 ones, expected 4, z = (8-4)/sqrt(2) = 4/1.414 ≈ 2.828
        for trial in &result.trials {
            assert_eq!(trial.ones_count, 8);
            assert!((trial.z_score - 2.828).abs() < 0.01);
        }
    }

    #[test]
    #[should_panic(expected = "bits_per_trial must be a positive multiple of 8")]
    fn test_zero_bits_per_trial_panics() {
        let config = TrialConfig { bits_per_trial: 0 };
        trial_analysis(&[0x42], &config);
    }

    #[test]
    #[should_panic(expected = "bits_per_trial must be a positive multiple of 8")]
    fn test_non_multiple_of_8_panics() {
        let config = TrialConfig { bits_per_trial: 13 };
        trial_analysis(&[0x42; 100], &config);
    }

    #[test]
    fn test_stouffer_empty() {
        let result = stouffer_combine(&[]);
        assert_eq!(result.num_sessions, 0);
        assert_eq!(result.stouffer_z, 0.0);
        assert_eq!(result.total_trials, 0);
    }

    #[test]
    fn test_calibration_empty_data() {
        let config = TrialConfig::default();
        let result = calibration_check(&[], &config);
        assert!(!result.is_suitable);
        assert!(
            result.warnings.iter().any(|w| w.contains("Insufficient")),
            "Should warn about insufficient data"
        );
    }

    #[test]
    fn test_normal_cdf_extreme_values() {
        // Very large positive: CDF → 1.0
        assert!((normal_cdf(10.0) - 1.0).abs() < 1e-10);
        // Very large negative: CDF → 0.0
        assert!(normal_cdf(-10.0) < 1e-10);
        // NaN → 0.5
        assert_eq!(normal_cdf(f64::NAN), 0.5);
    }

    #[test]
    fn test_p_value_range() {
        // P-value should always be in [0.0, 1.0]
        let config = TrialConfig::default();

        // Extreme bias (all ones)
        let result = trial_analysis(&vec![0xFF; 250], &config);
        assert!(result.terminal_p_value >= 0.0 && result.terminal_p_value <= 1.0);

        // No bias (alternating)
        let result = trial_analysis(&vec![0xAA; 250], &config);
        assert!(result.terminal_p_value >= 0.0 && result.terminal_p_value <= 1.0);
    }

    #[test]
    fn test_single_trial() {
        // Exactly 1 trial: terminal_z should equal the single trial's z_score
        let config = TrialConfig::default(); // 200 bits = 25 bytes
        let data = vec![0xFF; 25]; // 1 trial
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 1);
        assert_eq!(result.trials.len(), 1);
        assert!(
            (result.terminal_z - result.trials[0].z_score).abs() < 1e-10,
            "With 1 trial, terminal_z should equal the trial's z_score"
        );
        // effect_size = terminal_z / sqrt(1) = terminal_z
        assert!(
            (result.effect_size - result.terminal_z).abs() < 1e-10,
            "With 1 trial, effect_size should equal terminal_z"
        );
        // std_z should be 0 (population std with 1 sample)
        assert_eq!(result.std_z, 0.0);
    }

    #[test]
    fn test_trailing_bytes_ignored() {
        // 26 bytes with 25-byte trial size → 1 trial, 1 byte ignored
        let config = TrialConfig::default();
        let data = vec![0xAA; 26];
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 1);
        assert_eq!(result.bytes_consumed, 25);
    }

    #[test]
    fn test_cumulative_deviation_tracking() {
        // 0xAA (4 ones) then 0xFF (8 ones) per byte
        // Trial of 8 bits: expected 4 ones
        let config = TrialConfig { bits_per_trial: 8 };
        let data = vec![0xAA, 0xFF, 0x00];
        let result = trial_analysis(&data, &config);

        assert_eq!(result.num_trials, 3);
        // Trial 0: 4 ones, dev = 0
        assert_eq!(result.trials[0].ones_count, 4);
        assert!((result.trials[0].cumulative_deviation - 0.0).abs() < 0.001);
        // Trial 1: 8 ones, dev = +4, cum = 4
        assert_eq!(result.trials[1].ones_count, 8);
        assert!((result.trials[1].cumulative_deviation - 4.0).abs() < 0.001);
        // Trial 2: 0 ones, dev = -4, cum = 0
        assert_eq!(result.trials[2].ones_count, 0);
        assert!((result.trials[2].cumulative_deviation - 0.0).abs() < 0.001);
    }
}
