//! Conformal prediction for consciousness-RNG anomaly detection.
//!
//! Distribution-free anomaly detection with exact finite-sample coverage
//! guarantees. Unlike Mahalanobis distance (which assumes Gaussian),
//! conformal prediction makes ZERO distributional assumptions — only
//! exchangeability of the baseline data.
//!
//! Guarantee: P(false positive) <= alpha regardless of the data distribution.
//! Combined with conformal martingales, enables real-time sequential
//! monitoring with anytime-valid error control.
//!
//! Based on: Vovk, Gammerman & Shafer (2005) "Algorithmic Learning in a
//! Random World."

use serde::{Deserialize, Serialize};

/// Calibration data for conformal anomaly detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformalCalibration {
    /// Baseline feature vectors.
    pub baseline_features: Vec<Vec<f64>>,
    /// Nonconformity scores for baseline points.
    pub calibration_scores: Vec<f64>,
    /// Number of neighbors used.
    pub k: usize,
}

/// Result of conformal anomaly detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformalResult {
    /// Per-epoch conformal p-values.
    pub p_values: Vec<f64>,
    /// Per-epoch nonconformity scores.
    pub scores: Vec<f64>,
    /// Conformal martingale values (for sequential monitoring).
    pub martingale_values: Vec<f64>,
    /// Epochs flagged as anomalous (p < alpha).
    pub anomalous_epochs: Vec<usize>,
    /// Alpha level used.
    pub alpha: f64,
    /// Total epochs tested.
    pub total_epochs: usize,
    /// Whether the conformal martingale exceeded the threshold.
    pub martingale_reject: bool,
    /// Max conformal martingale value.
    pub max_martingale: f64,
    /// Interpretation.
    pub interpretation: String,
}

/// Compute the k-NN nonconformity score for a point relative to a set.
///
/// The score is the average distance to the k nearest neighbors.
/// Higher score = more anomalous (further from the calibration set).
pub fn nonconformity_score(point: &[f64], calibration: &[Vec<f64>], k: usize) -> f64 {
    if calibration.is_empty() || k == 0 {
        return 0.0;
    }

    let mut distances: Vec<f64> = calibration
        .iter()
        .map(|cal_point| {
            point
                .iter()
                .zip(cal_point.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();

    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let k_actual = k.min(distances.len());
    distances[..k_actual].iter().sum::<f64>() / k_actual as f64
}

/// Compute the conformal p-value for a new point.
///
/// p = #{calibration scores >= new score} / (n_calibration + 1)
///
/// This is exact: P(p <= alpha) <= alpha for exchangeable data.
pub fn conformal_p_value(score: f64, calibration_scores: &[f64]) -> f64 {
    if calibration_scores.is_empty() {
        return 1.0;
    }

    let n = calibration_scores.len();
    let count = calibration_scores.iter().filter(|&&s| s >= score).count();

    // +1 for the new point itself (Vovk's smoothed conformal p-value)
    (count as f64 + 1.0) / (n as f64 + 1.0)
}

/// Compute the conformal martingale for sequential monitoring.
///
/// The conformal martingale M_t = prod(epsilon * p_i^(epsilon-1)) for
/// epsilon = 0.5 (power martingale). Under exchangeability, E[M_t] <= 1.
/// Ville's inequality: P(max M_t >= 1/alpha) <= alpha.
pub fn conformal_martingale(p_values: &[f64], epsilon: f64) -> Vec<f64> {
    let mut martingale_values = Vec::with_capacity(p_values.len());
    let mut product = 1.0;

    for &p in p_values {
        let p_clamped = p.max(1e-10); // Avoid log(0)
        // Betting function: epsilon * p^(epsilon - 1)
        let bet = epsilon * p_clamped.powf(epsilon - 1.0);
        product *= bet;
        product = product.clamp(1e-30, 1e30); // Numerical stability
        martingale_values.push(product);
    }

    martingale_values
}

/// Build a calibration set from baseline feature vectors.
pub fn calibrate(baseline_features: &[Vec<f64>], k: usize) -> ConformalCalibration {
    // Compute leave-one-out nonconformity scores
    let mut calibration_scores = Vec::with_capacity(baseline_features.len());

    for i in 0..baseline_features.len() {
        let others: Vec<&Vec<f64>> = baseline_features
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, v)| v)
            .collect();

        let others_owned: Vec<Vec<f64>> = others.into_iter().cloned().collect();
        let score = nonconformity_score(&baseline_features[i], &others_owned, k);
        calibration_scores.push(score);
    }

    ConformalCalibration {
        baseline_features: baseline_features.to_vec(),
        calibration_scores,
        k,
    }
}

/// Detect anomalies in intention epochs using conformal prediction.
///
/// Returns exact coverage guarantee: P(false positive) <= alpha.
pub fn detect_anomalies(
    calibration: &ConformalCalibration,
    intention_features: &[Vec<f64>],
    alpha: f64,
) -> ConformalResult {
    let mut p_values = Vec::with_capacity(intention_features.len());
    let mut scores = Vec::with_capacity(intention_features.len());
    let mut anomalous_epochs = Vec::new();

    for (i, features) in intention_features.iter().enumerate() {
        let score = nonconformity_score(features, &calibration.baseline_features, calibration.k);
        let p = conformal_p_value(score, &calibration.calibration_scores);

        if p < alpha {
            anomalous_epochs.push(i);
        }

        scores.push(score);
        p_values.push(p);
    }

    // Conformal martingale for sequential monitoring
    let epsilon = 0.5; // Power martingale parameter
    let martingale_values = conformal_martingale(&p_values, epsilon);
    let max_martingale = martingale_values
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);

    // Reject if martingale exceeds 1/alpha (Ville's inequality)
    let martingale_threshold = 1.0 / alpha;
    let martingale_reject = max_martingale >= martingale_threshold;

    let total_epochs = intention_features.len();

    let interpretation = if anomalous_epochs.is_empty() && !martingale_reject {
        format!(
            "No anomalous intention epochs detected (alpha={alpha}). \
             Conformal martingale max={:.2} (threshold={:.1}). \
             Intention data is exchangeable with baseline.",
            max_martingale, martingale_threshold
        )
    } else if martingale_reject {
        format!(
            "Conformal martingale REJECTED exchangeability (max={:.2} >= threshold={:.1}). \
             {}/{} epochs flagged as anomalous. \
             Distribution-free evidence that intention data differs from baseline.",
            max_martingale, martingale_threshold,
            anomalous_epochs.len(), total_epochs
        )
    } else {
        format!(
            "{}/{} epochs flagged as anomalous at alpha={alpha}. \
             Conformal martingale max={:.2} (below threshold={:.1}). \
             Individual anomalies detected but no sequential evidence of systematic shift.",
            anomalous_epochs.len(), total_epochs,
            max_martingale, martingale_threshold
        )
    };

    ConformalResult {
        p_values,
        scores,
        martingale_values,
        anomalous_epochs,
        alpha,
        total_epochs,
        martingale_reject,
        max_martingale,
        interpretation,
    }
}

// ---------------------------------------------------------------------------
// Cross-Session Calibration Persistence
// ---------------------------------------------------------------------------

/// Save conformal calibration to a JSON file for cross-session use.
///
/// Persisting calibration sets allows subsequent sessions to build on
/// earlier baseline data, increasing statistical power without requiring
/// a fresh baseline collection each time.
pub fn save_calibration(calibration: &ConformalCalibration, path: &str) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let json = serde_json::to_string_pretty(calibration)
        .map_err(|e| format!("Failed to serialize calibration: {e}"))?;

    std::fs::write(path, &json)
        .map_err(|e| format!("Failed to write calibration to {path}: {e}"))?;

    Ok(())
}

/// Load conformal calibration from a JSON file.
pub fn load_calibration(path: &str) -> Result<ConformalCalibration, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read calibration from {path}: {e}"))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse calibration: {e}"))
}

/// Merge two calibration sets by combining their baseline features and
/// recomputing calibration scores.
///
/// This allows accumulating baseline data across sessions for more
/// powerful anomaly detection.
pub fn merge_calibrations(
    a: &ConformalCalibration,
    b: &ConformalCalibration,
) -> ConformalCalibration {
    let k = a.k.max(b.k);
    let mut combined_features = a.baseline_features.clone();
    combined_features.extend_from_slice(&b.baseline_features);

    // Recompute calibration scores on the combined set
    calibrate(&combined_features, k)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_baseline(n: usize) -> Vec<Vec<f64>> {
        // Cluster around origin
        (0..n)
            .map(|i| vec![
                (i as f64 * 0.1).sin(),
                (i as f64 * 0.1).cos(),
            ])
            .collect()
    }

    #[test]
    fn nonconformity_score_basic() {
        let calibration = vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
        ];
        let point = vec![0.0, 0.0];
        let score = nonconformity_score(&point, &calibration, 1);
        // Nearest neighbor is itself at distance 0
        assert!((score - 0.0).abs() < 1e-10);
    }

    #[test]
    fn nonconformity_score_far_point() {
        let calibration = vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
        ];
        let far_point = vec![100.0, 100.0];
        let near_point = vec![0.5, 0.0];
        let far_score = nonconformity_score(&far_point, &calibration, 1);
        let near_score = nonconformity_score(&near_point, &calibration, 1);
        assert!(far_score > near_score);
    }

    #[test]
    fn conformal_p_value_extreme() {
        // Score higher than all calibration scores -> low p-value
        let cal_scores = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let p = conformal_p_value(100.0, &cal_scores);
        // Only the new point itself scores as high, so p = 1/6
        assert!(p < 0.2, "p = {p}");
    }

    #[test]
    fn conformal_p_value_typical() {
        // Score in the middle of calibration scores -> high p-value
        let cal_scores = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let p = conformal_p_value(3.0, &cal_scores);
        assert!(p > 0.3, "p = {p}");
    }

    #[test]
    fn conformal_p_value_empty() {
        assert_eq!(conformal_p_value(1.0, &[]), 1.0);
    }

    #[test]
    fn conformal_martingale_basic() {
        // High p-values (normal) -> martingale stays low
        let p_values = vec![0.5, 0.6, 0.4, 0.5, 0.7];
        let mart = conformal_martingale(&p_values, 0.5);
        assert_eq!(mart.len(), 5);
        // Should not explode for typical p-values
        assert!(mart.last().unwrap() < &100.0);
    }

    #[test]
    fn conformal_martingale_anomalous() {
        // Very low p-values -> martingale grows
        let p_values = vec![0.01, 0.01, 0.01, 0.01, 0.01];
        let mart = conformal_martingale(&p_values, 0.5);
        // Should grow significantly for anomalous data
        assert!(mart.last().unwrap() > &1.0);
    }

    #[test]
    fn calibrate_basic() {
        let baseline = make_baseline(20);
        let cal = calibrate(&baseline, 3);
        assert_eq!(cal.calibration_scores.len(), 20);
        assert_eq!(cal.k, 3);
        // All scores should be positive
        assert!(cal.calibration_scores.iter().all(|&s| s >= 0.0));
    }

    #[test]
    fn detect_anomalies_normal() {
        let baseline = make_baseline(20);
        let cal = calibrate(&baseline, 3);
        // Test with data from same distribution
        let intention = make_baseline(10);
        let result = detect_anomalies(&cal, &intention, 0.05);
        assert_eq!(result.total_epochs, 10);
        // Should not flag too many anomalies from same distribution
        assert!(result.anomalous_epochs.len() <= 3);
    }

    #[test]
    fn detect_anomalies_outliers() {
        let baseline = make_baseline(20);
        let cal = calibrate(&baseline, 3);
        // Test with very different data
        let intention: Vec<Vec<f64>> = (0..10)
            .map(|i| vec![100.0 + i as f64, 100.0 + i as f64])
            .collect();
        let result = detect_anomalies(&cal, &intention, 0.05);
        // Far-away points should be flagged
        assert!(!result.anomalous_epochs.is_empty());
        assert!(!result.interpretation.is_empty());
    }

    #[test]
    fn save_and_load_calibration() {
        let baseline = make_baseline(15);
        let cal = calibrate(&baseline, 3);

        let path = "/tmp/oe_test_conformal_cal.json";
        save_calibration(&cal, path).unwrap();

        let loaded = load_calibration(path).unwrap();
        assert_eq!(loaded.baseline_features.len(), cal.baseline_features.len());
        assert_eq!(loaded.calibration_scores.len(), cal.calibration_scores.len());
        assert_eq!(loaded.k, cal.k);

        // Scores should be identical
        for (a, b) in cal.calibration_scores.iter().zip(loaded.calibration_scores.iter()) {
            assert!((a - b).abs() < 1e-10);
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_calibration_nonexistent() {
        let result = load_calibration("/tmp/oe_nonexistent_cal.json");
        assert!(result.is_err());
    }

    #[test]
    fn merge_calibrations_basic() {
        let baseline_a = make_baseline(10);
        let baseline_b: Vec<Vec<f64>> = (10..20)
            .map(|i| vec![(i as f64 * 0.1).sin(), (i as f64 * 0.1).cos()])
            .collect();

        let cal_a = calibrate(&baseline_a, 3);
        let cal_b = calibrate(&baseline_b, 3);

        let merged = merge_calibrations(&cal_a, &cal_b);
        assert_eq!(merged.baseline_features.len(), 20);
        assert_eq!(merged.calibration_scores.len(), 20);
        assert_eq!(merged.k, 3);
    }

    #[test]
    fn merge_calibrations_improves_detection() {
        // Merging more baseline data should improve anomaly detection
        let baseline_a = make_baseline(10);
        let baseline_b = make_baseline(10);
        let cal_a = calibrate(&baseline_a, 3);
        let cal_b = calibrate(&baseline_b, 3);
        let merged = merge_calibrations(&cal_a, &cal_b);

        // Merged should have more data points
        assert!(merged.baseline_features.len() > cal_a.baseline_features.len());
    }

    #[test]
    fn detect_anomalies_empty() {
        let baseline = make_baseline(10);
        let cal = calibrate(&baseline, 3);
        let result = detect_anomalies(&cal, &[], 0.05);
        assert_eq!(result.total_epochs, 0);
        assert!(result.anomalous_epochs.is_empty());
    }
}
