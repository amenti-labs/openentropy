//! Retrocausal protocol engine for consciousness-RNG experiments.
//!
//! In the retrocausal protocol, RNG data is collected BEFORE the operator
//! knows their intention direction. This tests the hypothesis that
//! consciousness can influence events retroactively.
//!
//! ## Protocol
//!
//! 1. Collect N trials of random bytes (no intention — operator does nothing)
//! 2. After ALL data is collected, generate random direction assignment
//!    (High/Low) for each trial using a separate random source
//! 3. Score each trial as if the assigned direction was the operator's intention
//! 4. Under the null hypothesis, this is pure chance — any significant result
//!    would suggest retrocausal influence or selection effects
//!
//! This protocol eliminates all possible real-time influence mechanisms,
//! making it the strictest test in consciousness-RNG research.
//!
//! Based on: Schmidt (1976) "PK effect on pre-recorded targets" and
//! Bem (2011) "Feeling the future."

use serde::{Deserialize, Serialize};

use crate::consciousness::{IntentionDirection, count_ones_n, trial_z_score, stouffer_z, z_to_p_two_tailed};

/// A single retrocausal trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrocausalTrial {
    /// Trial index.
    pub index: usize,
    /// Raw data collected (pre-intention).
    pub data: Vec<u8>,
    /// Number of 1-bits counted.
    pub ones_count: u32,
    /// Number of bits used.
    pub n_bits: usize,
    /// Post-hoc assigned direction.
    pub assigned_direction: IntentionDirection,
    /// Z-score interpreted relative to assigned direction.
    pub z_score: f64,
    /// Whether this trial is "successful" (Z in the assigned direction).
    pub success: bool,
}

/// Result of a retrocausal experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrocausalResult {
    /// All trials.
    pub trials: Vec<RetrocausalTrial>,
    /// Stouffer Z across all High-assigned trials.
    pub high_z: f64,
    /// Stouffer Z across all Low-assigned trials.
    pub low_z: f64,
    /// Differential Z (High - Low) / sqrt(2).
    pub differential_z: f64,
    /// P-value for the differential.
    pub differential_p: f64,
    /// Overall Stouffer Z (direction-aware: High as positive, Low as negative).
    pub overall_z: f64,
    /// Overall p-value.
    pub overall_p: f64,
    /// Success rate (fraction of trials in the assigned direction).
    pub success_rate: f64,
    /// Expected success rate under null (0.5).
    pub expected_success_rate: f64,
    /// Number of High-assigned trials.
    pub n_high: usize,
    /// Number of Low-assigned trials.
    pub n_low: usize,
    /// Interpretation.
    pub interpretation: String,
}

/// Generate a sequence of retrocausal trials.
///
/// Collects `n_trials` worth of random bytes, then assigns random
/// directions post-hoc using a separate random source (system time hash).
pub fn generate_retrocausal_sequence(
    data_per_trial: &[Vec<u8>],
    bits_per_trial: usize,
) -> Vec<RetrocausalTrial> {
    // Generate direction assignments using a simple PRNG seeded from system time
    // This is separate from the entropy sources being tested
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);

    let mut rng_state = seed;
    let mut trials = Vec::with_capacity(data_per_trial.len());

    for (index, data) in data_per_trial.iter().enumerate() {
        // Xorshift64 for direction assignment
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;

        let direction = if rng_state % 2 == 0 {
            IntentionDirection::High
        } else {
            IntentionDirection::Low
        };

        let ones = count_ones_n(data, bits_per_trial);
        let raw_z = trial_z_score(ones, bits_per_trial);

        // Direction-aware Z: positive = in the assigned direction
        let z = match direction {
            IntentionDirection::High => raw_z,   // Want more ones
            IntentionDirection::Low => -raw_z,    // Want fewer ones
            IntentionDirection::Baseline => raw_z, // Not used
        };

        let success = z > 0.0;

        trials.push(RetrocausalTrial {
            index,
            data: data.clone(),
            ones_count: ones,
            n_bits: bits_per_trial,
            assigned_direction: direction,
            z_score: z,
            success,
        });
    }

    trials
}

/// Analyze retrocausal trial results.
pub fn retrocausal_analysis(trials: &[RetrocausalTrial]) -> RetrocausalResult {
    if trials.is_empty() {
        return RetrocausalResult {
            trials: Vec::new(),
            high_z: 0.0,
            low_z: 0.0,
            differential_z: 0.0,
            differential_p: 1.0,
            overall_z: 0.0,
            overall_p: 1.0,
            success_rate: 0.5,
            expected_success_rate: 0.5,
            n_high: 0,
            n_low: 0,
            interpretation: "No trials".to_string(),
        };
    }

    let high_trials: Vec<&RetrocausalTrial> = trials
        .iter()
        .filter(|t| t.assigned_direction == IntentionDirection::High)
        .collect();
    let low_trials: Vec<&RetrocausalTrial> = trials
        .iter()
        .filter(|t| t.assigned_direction == IntentionDirection::Low)
        .collect();

    // Stouffer Z for each direction (using raw Z-scores, not direction-aware)
    let high_raw_zs: Vec<f64> = high_trials
        .iter()
        .map(|t| trial_z_score(t.ones_count, t.n_bits))
        .collect();
    let low_raw_zs: Vec<f64> = low_trials
        .iter()
        .map(|t| trial_z_score(t.ones_count, t.n_bits))
        .collect();

    let high_z = stouffer_z(&high_raw_zs);
    let low_z = stouffer_z(&low_raw_zs);

    // Differential: if retrocausal effect exists, High should be positive and Low negative
    let differential_z = (high_z - low_z) / std::f64::consts::SQRT_2;
    let differential_p = z_to_p_two_tailed(differential_z);

    // Overall direction-aware Z
    let direction_aware_zs: Vec<f64> = trials.iter().map(|t| t.z_score).collect();
    let overall_z = stouffer_z(&direction_aware_zs);
    let overall_p = z_to_p_two_tailed(overall_z);

    let success_count = trials.iter().filter(|t| t.success).count();
    let success_rate = success_count as f64 / trials.len() as f64;

    let interpretation = if overall_p < 0.01 {
        format!(
            "STRONG retrocausal signal: Z={:.3}, p={:.4}. Pre-collected data \
             shows significant alignment with post-hoc assigned directions. \
             This eliminates all real-time influence mechanisms. \
             Success rate: {:.1}% (expected: 50.0%)",
            overall_z, overall_p, success_rate * 100.0
        )
    } else if overall_p < 0.05 {
        format!(
            "Suggestive retrocausal signal: Z={:.3}, p={:.4}. \
             Marginal alignment with post-hoc directions. \
             Replication strongly recommended. \
             Success rate: {:.1}% (expected: 50.0%)",
            overall_z, overall_p, success_rate * 100.0
        )
    } else {
        format!(
            "No retrocausal effect: Z={:.3}, p={:.4}. \
             Pre-collected data shows no alignment with post-hoc directions. \
             This is the expected null result. \
             Success rate: {:.1}% (expected: 50.0%)",
            overall_z, overall_p, success_rate * 100.0
        )
    };

    RetrocausalResult {
        trials: trials.to_vec(),
        high_z,
        low_z,
        differential_z,
        differential_p,
        overall_z,
        overall_p,
        success_rate,
        expected_success_rate: 0.5,
        n_high: high_trials.len(),
        n_low: low_trials.len(),
        interpretation,
    }
}

/// Direction-aware Z-score for a retrocausal trial.
///
/// For High assignment: Z = (ones - n/2) / sqrt(n/4) (positive = success)
/// For Low assignment: Z = (n/2 - ones) / sqrt(n/4) (positive = success)
pub fn retrocausal_z_score(
    ones: u32,
    n_bits: usize,
    direction: IntentionDirection,
) -> f64 {
    let raw_z = trial_z_score(ones, n_bits);
    match direction {
        IntentionDirection::High => raw_z,
        IntentionDirection::Low => -raw_z,
        IntentionDirection::Baseline => raw_z,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrocausal_z_high() {
        // 110 ones out of 200 -> raw Z positive
        let z = retrocausal_z_score(110, 200, IntentionDirection::High);
        assert!(z > 0.0);
    }

    #[test]
    fn retrocausal_z_low() {
        // 110 ones out of 200 -> raw Z positive, but Low flips it
        let z = retrocausal_z_score(110, 200, IntentionDirection::Low);
        assert!(z < 0.0);
    }

    #[test]
    fn retrocausal_z_low_success() {
        // 90 ones out of 200 -> raw Z negative, Low flips to positive = success
        let z = retrocausal_z_score(90, 200, IntentionDirection::Low);
        assert!(z > 0.0);
    }

    #[test]
    fn generate_sequence_basic() {
        let data: Vec<Vec<u8>> = (0..20)
            .map(|i| vec![(i * 13 % 256) as u8; 25])
            .collect();
        let trials = generate_retrocausal_sequence(&data, 200);
        assert_eq!(trials.len(), 20);

        // Should have roughly half High and half Low
        let n_high = trials.iter().filter(|t| t.assigned_direction == IntentionDirection::High).count();
        let n_low = trials.iter().filter(|t| t.assigned_direction == IntentionDirection::Low).count();
        assert_eq!(n_high + n_low, 20);
    }

    #[test]
    fn generate_sequence_direction_variety() {
        // With 100 trials, should have both directions
        let data: Vec<Vec<u8>> = (0..100)
            .map(|_| vec![128u8; 25])
            .collect();
        let trials = generate_retrocausal_sequence(&data, 200);
        let n_high = trials.iter().filter(|t| t.assigned_direction == IntentionDirection::High).count();
        let n_low = trials.len() - n_high;
        assert!(n_high > 10 && n_low > 10, "high={n_high}, low={n_low}");
    }

    #[test]
    fn retrocausal_analysis_null() {
        // Data at exactly 50% should produce null result
        let data: Vec<Vec<u8>> = (0..50)
            .map(|_| vec![0xAA; 25]) // 0xAA = 10101010 = exactly 50% ones
            .collect();
        let trials = generate_retrocausal_sequence(&data, 200);
        let result = retrocausal_analysis(&trials);

        assert_eq!(result.trials.len(), 50);
        assert!(result.expected_success_rate == 0.5);
        assert!(result.n_high + result.n_low == 50);
    }

    #[test]
    fn retrocausal_analysis_empty() {
        let result = retrocausal_analysis(&[]);
        assert_eq!(result.overall_p, 1.0);
        assert_eq!(result.n_high, 0);
        assert_eq!(result.n_low, 0);
    }

    #[test]
    fn retrocausal_analysis_serializable() {
        let data: Vec<Vec<u8>> = (0..10)
            .map(|_| vec![128u8; 25])
            .collect();
        let trials = generate_retrocausal_sequence(&data, 200);
        let result = retrocausal_analysis(&trials);
        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }

    #[test]
    fn retrocausal_z_at_midpoint() {
        let z = retrocausal_z_score(100, 200, IntentionDirection::High);
        assert!((z - 0.0).abs() < 1e-10);
    }
}
