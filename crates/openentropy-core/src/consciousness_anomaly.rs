//! ML-lite anomaly detection for consciousness-RNG experiments.
//!
//! Instead of parametric tests (Z-scores, t-tests), uses multivariate
//! feature extraction and Mahalanobis distance to detect *any* distributional
//! shift between baseline and intention epochs. This catches effects that
//! hand-picked test statistics might miss.
//!
//! No external ML crates — pure Rust linear algebra.

use serde::{Deserialize, Serialize};

use crate::consciousness_stats;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Feature vector for a single epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochFeatures {
    /// Epoch index.
    pub epoch_index: usize,
    /// Direction label.
    pub direction: String,
    /// Raw feature values (named).
    pub features: Vec<(String, f64)>,
    /// Mahalanobis distance from baseline distribution (if computed).
    pub mahalanobis_distance: Option<f64>,
    /// Is this epoch flagged as anomalous?
    pub is_anomalous: bool,
}

/// Complete anomaly detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResult {
    /// Feature names used.
    pub feature_names: Vec<String>,
    /// Per-epoch features and scores.
    pub epoch_features: Vec<EpochFeatures>,
    /// Baseline distribution mean vector.
    pub baseline_mean: Vec<f64>,
    /// Number of anomalous intention epochs detected.
    pub anomalous_count: usize,
    /// Total intention epochs.
    pub total_intention_epochs: usize,
    /// Chi-squared threshold used (based on feature dimensions and alpha=0.05).
    pub threshold: f64,
    /// Mean Mahalanobis distance for baseline epochs (should be low).
    pub baseline_mean_distance: f64,
    /// Mean Mahalanobis distance for intention epochs.
    pub intention_mean_distance: f64,
    /// Interpretation.
    pub interpretation: String,
}

// ---------------------------------------------------------------------------
// Feature extraction
// ---------------------------------------------------------------------------

/// Feature names for epoch characterization.
pub const FEATURE_NAMES: &[&str] = &[
    "mean",
    "variance",
    "skewness",
    "kurtosis",
    "bit_bias",
    "approximate_entropy",
    "lz76_complexity",
    "spectral_flatness",
    "max_run_length",
    "mean_absolute_change",
];

/// Extract feature vector from raw byte data.
pub fn extract_features(data: &[u8]) -> Vec<f64> {
    if data.is_empty() {
        return vec![0.0; FEATURE_NAMES.len()];
    }

    let n = data.len() as f64;
    let values: Vec<f64> = data.iter().map(|&b| b as f64).collect();

    // Mean
    let mean = values.iter().sum::<f64>() / n;

    // Variance
    let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / n;
    let sd = variance.sqrt().max(1e-10);

    // Skewness
    let skewness = values
        .iter()
        .map(|&x| ((x - mean) / sd).powi(3))
        .sum::<f64>()
        / n;

    // Kurtosis (excess)
    let kurtosis = values
        .iter()
        .map(|&x| ((x - mean) / sd).powi(4))
        .sum::<f64>()
        / n
        - 3.0;

    // Bit bias (proportion of 1-bits)
    let total_ones: u32 = data.iter().map(|b| b.count_ones()).sum();
    let bit_bias = total_ones as f64 / (n * 8.0) - 0.5; // deviation from 0.5

    // Information-theoretic measures
    let r = 0.2 * sd;
    let apen = consciousness_stats::approximate_entropy(data, 2, r);
    let lz76 = consciousness_stats::lz76_complexity(data);
    let flatness = consciousness_stats::spectral_flatness(data);

    // Max run length (longest consecutive same bit)
    let max_run = max_run_length(data);

    // Mean absolute change
    let mac = consciousness_stats::mean_absolute_change(&values);

    vec![
        mean, variance, skewness, kurtosis, bit_bias, apen, lz76, flatness,
        max_run as f64, mac,
    ]
}

/// Find the longest run of identical bits.
fn max_run_length(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }

    let mut max_run = 1usize;
    let mut current_run = 1usize;
    let mut prev_bit: Option<u8> = None;

    for &byte in data {
        for bit_idx in (0..8).rev() {
            let bit = (byte >> bit_idx) & 1;
            if Some(bit) == prev_bit {
                current_run += 1;
                if current_run > max_run {
                    max_run = current_run;
                }
            } else {
                current_run = 1;
            }
            prev_bit = Some(bit);
        }
    }

    max_run
}

// ---------------------------------------------------------------------------
// Mahalanobis distance computation
// ---------------------------------------------------------------------------

/// Estimate mean and covariance matrix from feature vectors.
pub fn estimate_distribution(samples: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = samples.len();
    let d = if n > 0 { samples[0].len() } else { 0 };

    if n < 2 || d == 0 {
        return (vec![0.0; d], vec![vec![0.0; d]; d]);
    }

    // Mean
    let mut mean = vec![0.0; d];
    for sample in samples {
        for (j, &v) in sample.iter().enumerate() {
            mean[j] += v;
        }
    }
    for m in &mut mean {
        *m /= n as f64;
    }

    // Covariance matrix (with regularization)
    let mut cov = vec![vec![0.0; d]; d];
    for sample in samples {
        for i in 0..d {
            for j in 0..d {
                cov[i][j] += (sample[i] - mean[i]) * (sample[j] - mean[j]);
            }
        }
    }
    for row in &mut cov {
        for val in row.iter_mut() {
            *val /= (n - 1) as f64;
        }
    }

    // Regularize: add small diagonal to prevent singularity
    for i in 0..d {
        cov[i][i] += 1e-6;
    }

    (mean, cov)
}

/// Invert a matrix using Gauss-Jordan elimination.
///
/// Returns None if the matrix is singular.
pub fn invert_matrix(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    if n == 0 || m[0].len() != n {
        return None;
    }

    // Augmented matrix [M | I]
    let mut aug: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend(vec![0.0; n]);
            r[n + i] = 1.0;
            r
        })
        .collect();

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_val = aug[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }

        if max_val < 1e-12 {
            return None; // Singular
        }

        // Swap rows
        if max_row != col {
            aug.swap(col, max_row);
        }

        // Scale pivot row
        let pivot = aug[col][col];
        for j in 0..(2 * n) {
            aug[col][j] /= pivot;
        }

        // Eliminate column
        for row in 0..n {
            if row != col {
                let factor = aug[row][col];
                for j in 0..(2 * n) {
                    aug[row][j] -= factor * aug[col][j];
                }
            }
        }
    }

    // Extract inverse
    let inverse: Vec<Vec<f64>> = aug
        .iter()
        .map(|row| row[n..].to_vec())
        .collect();

    Some(inverse)
}

/// Compute Mahalanobis distance of a point from a distribution.
///
/// d = sqrt((x - mu)^T * Sigma^{-1} * (x - mu))
pub fn mahalanobis_distance(x: &[f64], mean: &[f64], inv_cov: &[Vec<f64>]) -> f64 {
    let d = x.len();
    if d == 0 || mean.len() != d || inv_cov.len() != d {
        return 0.0;
    }

    let diff: Vec<f64> = x.iter().zip(mean).map(|(a, b)| a - b).collect();

    // diff^T * inv_cov * diff
    let mut result = 0.0;
    for i in 0..d {
        let mut inner = 0.0;
        for j in 0..d {
            inner += inv_cov[i][j] * diff[j];
        }
        result += diff[i] * inner;
    }

    result.max(0.0).sqrt()
}

/// Chi-squared critical value approximation for alpha=0.05.
///
/// Uses Wilson-Hilferty approximation: chi2_alpha ≈ d * (1 - 2/(9d) + z_alpha * sqrt(2/(9d)))^3
fn chi_squared_critical(df: usize, alpha: f64) -> f64 {
    let d = df as f64;
    // z_alpha for common alpha values
    let z = if alpha <= 0.01 {
        2.326
    } else if alpha <= 0.05 {
        1.645
    } else {
        1.282
    };

    let term = 1.0 - 2.0 / (9.0 * d) + z * (2.0 / (9.0 * d)).sqrt();
    d * term.powi(3)
}

// ---------------------------------------------------------------------------
// Main computation
// ---------------------------------------------------------------------------

/// Compute anomaly detection analysis.
///
/// - `baseline_data`: byte data from baseline epochs
/// - `intention_data`: byte data from intention epochs (with direction labels)
pub fn compute_anomaly(
    baseline_epochs: &[(usize, Vec<u8>)],
    intention_epochs: &[(usize, String, Vec<u8>)],
) -> AnomalyResult {
    let feature_names: Vec<String> = FEATURE_NAMES.iter().map(|&s| s.to_string()).collect();

    // Extract features from all epochs
    let baseline_features: Vec<Vec<f64>> = baseline_epochs
        .iter()
        .map(|(_, data)| extract_features(data))
        .collect();
    let intention_features: Vec<Vec<f64>> = intention_epochs
        .iter()
        .map(|(_, _, data)| extract_features(data))
        .collect();

    // Estimate baseline distribution
    let (mean, cov) = estimate_distribution(&baseline_features);

    // Invert covariance
    let inv_cov = invert_matrix(&cov).unwrap_or_else(|| {
        // Fallback: identity matrix (L2 distance)
        let d = mean.len();
        let mut id = vec![vec![0.0; d]; d];
        for i in 0..d {
            id[i][i] = 1.0;
        }
        id
    });

    // Threshold: chi-squared critical value at alpha=0.05 with d degrees of freedom
    let d = feature_names.len();
    let threshold = chi_squared_critical(d, 0.05);

    // Score all epochs
    let mut epoch_features_vec = Vec::new();

    // Baseline epochs
    for (i, (epoch_idx, _)) in baseline_epochs.iter().enumerate() {
        let dist = mahalanobis_distance(&baseline_features[i], &mean, &inv_cov);
        epoch_features_vec.push(EpochFeatures {
            epoch_index: *epoch_idx,
            direction: "BASELINE".to_string(),
            features: feature_names
                .iter()
                .zip(&baseline_features[i])
                .map(|(n, &v)| (n.clone(), v))
                .collect(),
            mahalanobis_distance: Some(dist),
            is_anomalous: dist > threshold,
        });
    }

    // Intention epochs
    let mut anomalous_count = 0;
    for (i, (epoch_idx, direction, _)) in intention_epochs.iter().enumerate() {
        let dist = mahalanobis_distance(&intention_features[i], &mean, &inv_cov);
        let is_anomalous = dist > threshold;
        if is_anomalous {
            anomalous_count += 1;
        }
        epoch_features_vec.push(EpochFeatures {
            epoch_index: *epoch_idx,
            direction: direction.clone(),
            features: feature_names
                .iter()
                .zip(&intention_features[i])
                .map(|(n, &v)| (n.clone(), v))
                .collect(),
            mahalanobis_distance: Some(dist),
            is_anomalous,
        });
    }

    let baseline_distances: Vec<f64> = epoch_features_vec
        .iter()
        .filter(|e| e.direction == "BASELINE")
        .filter_map(|e| e.mahalanobis_distance)
        .collect();
    let intention_distances: Vec<f64> = epoch_features_vec
        .iter()
        .filter(|e| e.direction != "BASELINE")
        .filter_map(|e| e.mahalanobis_distance)
        .collect();

    let baseline_mean_distance = if baseline_distances.is_empty() {
        0.0
    } else {
        baseline_distances.iter().sum::<f64>() / baseline_distances.len() as f64
    };
    let intention_mean_distance = if intention_distances.is_empty() {
        0.0
    } else {
        intention_distances.iter().sum::<f64>() / intention_distances.len() as f64
    };

    let total_intention = intention_epochs.len();
    let interpretation = if total_intention == 0 {
        "No intention epochs to analyze".to_string()
    } else if anomalous_count == 0 {
        "No anomalous intention epochs detected — intention data is within baseline distribution"
            .to_string()
    } else {
        let pct = anomalous_count as f64 / total_intention as f64 * 100.0;
        format!(
            "{anomalous_count}/{total_intention} intention epochs anomalous ({pct:.0}%) — \
             intention data deviates from baseline in feature space"
        )
    };

    AnomalyResult {
        feature_names,
        epoch_features: epoch_features_vec,
        baseline_mean: mean,
        anomalous_count,
        total_intention_epochs: total_intention,
        threshold,
        baseline_mean_distance,
        intention_mean_distance,
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
    fn extract_features_empty() {
        let f = extract_features(&[]);
        assert_eq!(f.len(), FEATURE_NAMES.len());
        assert!(f.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn extract_features_basic() {
        let data: Vec<u8> = (0..100).collect();
        let f = extract_features(&data);
        assert_eq!(f.len(), FEATURE_NAMES.len());
        // Mean of 0..99 = 49.5
        assert!((f[0] - 49.5).abs() < 0.01, "mean = {}", f[0]);
        // Variance should be nonzero
        assert!(f[1] > 0.0, "variance = {}", f[1]);
    }

    #[test]
    fn max_run_length_empty() {
        assert_eq!(max_run_length(&[]), 0);
    }

    #[test]
    fn max_run_length_all_ones() {
        assert_eq!(max_run_length(&[0xFF, 0xFF]), 16);
    }

    #[test]
    fn max_run_length_alternating() {
        assert_eq!(max_run_length(&[0xAA]), 1); // 10101010
    }

    #[test]
    fn estimate_distribution_basic() {
        let samples = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
            vec![5.0, 6.0],
        ];
        let (mean, cov) = estimate_distribution(&samples);
        assert!((mean[0] - 3.0).abs() < 1e-10);
        assert!((mean[1] - 4.0).abs() < 1e-10);
        assert!(cov[0][0] > 0.0);
    }

    #[test]
    fn estimate_distribution_empty() {
        let (mean, cov) = estimate_distribution(&[]);
        assert!(mean.is_empty());
        assert!(cov.is_empty());
    }

    #[test]
    fn invert_matrix_identity() {
        let id = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let inv = invert_matrix(&id).unwrap();
        assert!((inv[0][0] - 1.0).abs() < 1e-10);
        assert!((inv[1][1] - 1.0).abs() < 1e-10);
        assert!(inv[0][1].abs() < 1e-10);
    }

    #[test]
    fn invert_matrix_2x2() {
        let m = vec![vec![4.0, 7.0], vec![2.0, 6.0]];
        let inv = invert_matrix(&m).unwrap();
        // Verify M * M^{-1} ≈ I
        let prod00 = m[0][0] * inv[0][0] + m[0][1] * inv[1][0];
        let prod01 = m[0][0] * inv[0][1] + m[0][1] * inv[1][1];
        assert!((prod00 - 1.0).abs() < 1e-8, "prod00 = {prod00}");
        assert!(prod01.abs() < 1e-8, "prod01 = {prod01}");
    }

    #[test]
    fn invert_matrix_singular() {
        let m = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        assert!(invert_matrix(&m).is_none());
    }

    #[test]
    fn mahalanobis_at_mean_is_zero() {
        let mean = vec![1.0, 2.0];
        let inv_cov = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let d = mahalanobis_distance(&mean, &mean, &inv_cov);
        assert!(d < 1e-10, "d = {d}");
    }

    #[test]
    fn mahalanobis_positive() {
        let mean = vec![0.0, 0.0];
        let inv_cov = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let x = vec![3.0, 4.0];
        let d = mahalanobis_distance(&x, &mean, &inv_cov);
        // With identity covariance, Mahalanobis = Euclidean = sqrt(9+16) = 5
        assert!((d - 5.0).abs() < 1e-10, "d = {d}");
    }

    #[test]
    fn anomaly_detection_empty() {
        let result = compute_anomaly(&[], &[]);
        assert_eq!(result.anomalous_count, 0);
        assert_eq!(result.total_intention_epochs, 0);
    }

    #[test]
    fn anomaly_detection_identical_epochs() {
        let data: Vec<u8> = (0..200).map(|i| (i % 256) as u8).collect();
        let baseline = vec![(0, data.clone()), (1, data.clone()), (2, data.clone())];
        let intention = vec![(3, "HIGH".to_string(), data.clone())];
        let result = compute_anomaly(&baseline, &intention);
        // Identical data should not be flagged as anomalous
        assert_eq!(result.anomalous_count, 0);
    }

    #[test]
    fn chi_squared_critical_sanity() {
        let c = chi_squared_critical(10, 0.05);
        // chi2(10, 0.05) ≈ 18.307
        assert!(c > 10.0 && c < 30.0, "critical = {c}");
    }
}
