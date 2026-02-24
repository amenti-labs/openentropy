//! ML-lite classification for consciousness experiment anomaly detection.
//!
//! Implements a simple nearest-centroid / LDA-inspired classifier that
//! can distinguish between baseline and intention epochs based on the
//! 10-feature anomaly vector. Pure Rust, no external dependencies.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Feature extraction (reused from anomaly detection)
// ---------------------------------------------------------------------------

/// Feature names for the 10-dimensional anomaly vector.
pub const FEATURE_NAMES: &[&str] = &[
    "mean",
    "variance",
    "skewness",
    "kurtosis",
    "bit_bias",
    "approx_entropy",
    "lz76_complexity",
    "spectral_flatness",
    "max_run_length",
    "mean_abs_change",
];

/// Extract 10-dimensional feature vector from byte data.
pub fn extract_features(data: &[u8]) -> Vec<f64> {
    if data.is_empty() {
        return vec![0.0; 10];
    }

    let n = data.len() as f64;
    let values: Vec<f64> = data.iter().map(|&b| b as f64).collect();

    // Mean
    let mean = values.iter().sum::<f64>() / n;

    // Variance
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let sd = variance.sqrt().max(1e-10);

    // Skewness
    let skewness = values
        .iter()
        .map(|x| ((x - mean) / sd).powi(3))
        .sum::<f64>()
        / n;

    // Kurtosis (excess)
    let kurtosis = values
        .iter()
        .map(|x| ((x - mean) / sd).powi(4))
        .sum::<f64>()
        / n
        - 3.0;

    // Bit bias
    let total_ones: u32 = data.iter().map(|b| b.count_ones()).sum();
    let total_bits = data.len() * 8;
    let bit_bias = (total_ones as f64 / total_bits as f64 - 0.5).abs();

    // Approximate entropy (simplified)
    let approx_entropy = crate::consciousness_stats::approximate_entropy(data, 2, 0.2 * sd);

    // LZ76 complexity
    let lz76 = crate::consciousness_stats::lz76_complexity(data);

    // Spectral flatness
    let spectral_flatness = crate::consciousness_stats::spectral_flatness(data);

    // Max run length
    let max_run = max_run_length(data);

    // Mean absolute change
    let mean_abs_change = if values.len() > 1 {
        values
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .sum::<f64>()
            / (values.len() - 1) as f64
    } else {
        0.0
    };

    vec![
        mean,
        variance,
        skewness,
        kurtosis,
        bit_bias,
        approx_entropy,
        lz76,
        spectral_flatness,
        max_run as f64,
        mean_abs_change,
    ]
}

fn max_run_length(data: &[u8]) -> usize {
    let mut max_run = 0;
    let mut current_run = 1;
    for window in data.windows(2) {
        if window[0] == window[1] {
            current_run += 1;
        } else {
            max_run = max_run.max(current_run);
            current_run = 1;
        }
    }
    max_run.max(current_run)
}

// ---------------------------------------------------------------------------
// Nearest Centroid Classifier
// ---------------------------------------------------------------------------

/// A trained nearest-centroid classifier for baseline vs intention epochs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearestCentroidClassifier {
    /// Centroid of baseline class (mean feature vector).
    pub baseline_centroid: Vec<f64>,
    /// Centroid of intention class (mean feature vector).
    pub intention_centroid: Vec<f64>,
    /// Per-feature standard deviations for normalization.
    pub feature_sds: Vec<f64>,
    /// Number of training samples per class.
    pub baseline_n: usize,
    pub intention_n: usize,
    /// Feature names.
    pub feature_names: Vec<String>,
}

/// Classification result for a single epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Predicted class: "baseline" or "intention".
    pub predicted_class: String,
    /// Distance to baseline centroid (normalized).
    pub baseline_distance: f64,
    /// Distance to intention centroid (normalized).
    pub intention_distance: f64,
    /// Confidence: ratio of distances (higher = more confident).
    pub confidence: f64,
    /// Feature vector used for classification.
    pub features: Vec<f64>,
}

/// Full classification report across multiple epochs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationReport {
    /// Per-epoch classification results.
    pub classifications: Vec<ClassificationResult>,
    /// Overall accuracy (if ground truth is known).
    pub accuracy: f64,
    /// True positive rate (intention correctly classified).
    pub true_positive_rate: f64,
    /// True negative rate (baseline correctly classified).
    pub true_negative_rate: f64,
    /// Cross-validation accuracy (leave-one-out).
    pub cv_accuracy: f64,
    /// Feature importance ranking.
    pub feature_importance: Vec<(String, f64)>,
}

impl NearestCentroidClassifier {
    /// Train a classifier from labeled feature vectors.
    pub fn train(
        baseline_features: &[Vec<f64>],
        intention_features: &[Vec<f64>],
    ) -> Option<Self> {
        if baseline_features.is_empty() || intention_features.is_empty() {
            return None;
        }

        let n_features = baseline_features[0].len();

        // Compute centroids
        let baseline_centroid = compute_centroid(baseline_features, n_features);
        let intention_centroid = compute_centroid(intention_features, n_features);

        // Compute per-feature SDs (pooled across both classes)
        let mut all_features: Vec<&Vec<f64>> = baseline_features.iter().collect();
        all_features.extend(intention_features.iter());

        let feature_sds: Vec<f64> = (0..n_features)
            .map(|j| {
                let mean = all_features.iter().map(|f| f[j]).sum::<f64>() / all_features.len() as f64;
                let var = all_features
                    .iter()
                    .map(|f| (f[j] - mean).powi(2))
                    .sum::<f64>()
                    / all_features.len() as f64;
                var.sqrt().max(1e-10)
            })
            .collect();

        Some(Self {
            baseline_centroid,
            intention_centroid,
            feature_sds,
            baseline_n: baseline_features.len(),
            intention_n: intention_features.len(),
            feature_names: FEATURE_NAMES.iter().map(|s| s.to_string()).collect(),
        })
    }

    /// Classify a single feature vector.
    pub fn classify(&self, features: &[f64]) -> ClassificationResult {
        let baseline_dist = normalized_distance(features, &self.baseline_centroid, &self.feature_sds);
        let intention_dist = normalized_distance(features, &self.intention_centroid, &self.feature_sds);

        let predicted_class = if baseline_dist <= intention_dist {
            "baseline"
        } else {
            "intention"
        };

        let confidence = if baseline_dist + intention_dist > 0.0 {
            (baseline_dist - intention_dist).abs() / (baseline_dist + intention_dist)
        } else {
            0.0
        };

        ClassificationResult {
            predicted_class: predicted_class.to_string(),
            baseline_distance: baseline_dist,
            intention_distance: intention_dist,
            confidence,
            features: features.to_vec(),
        }
    }

    /// Compute feature importance (Fisher's discriminant ratio).
    pub fn feature_importance(&self) -> Vec<(String, f64)> {
        let mut importance: Vec<(String, f64)> = self
            .baseline_centroid
            .iter()
            .zip(self.intention_centroid.iter())
            .zip(self.feature_sds.iter())
            .enumerate()
            .map(|(i, ((bc, ic), sd))| {
                let name = self.feature_names.get(i).cloned().unwrap_or_else(|| format!("f{i}"));
                let fisher_ratio = ((bc - ic) / sd).powi(2);
                (name, fisher_ratio)
            })
            .collect();

        importance.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        importance
    }

    /// Leave-one-out cross-validation accuracy.
    pub fn loocv_accuracy(
        baseline_features: &[Vec<f64>],
        intention_features: &[Vec<f64>],
    ) -> f64 {
        let total = baseline_features.len() + intention_features.len();
        if total < 4 {
            return 0.5; // Not enough data for CV
        }

        let mut correct = 0;

        // LOO for baseline samples
        for i in 0..baseline_features.len() {
            let train_baseline: Vec<Vec<f64>> = baseline_features
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, f)| f.clone())
                .collect();

            if let Some(clf) = Self::train(&train_baseline, intention_features) {
                let result = clf.classify(&baseline_features[i]);
                if result.predicted_class == "baseline" {
                    correct += 1;
                }
            }
        }

        // LOO for intention samples
        for i in 0..intention_features.len() {
            let train_intention: Vec<Vec<f64>> = intention_features
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, f)| f.clone())
                .collect();

            if let Some(clf) = Self::train(baseline_features, &train_intention) {
                let result = clf.classify(&intention_features[i]);
                if result.predicted_class == "intention" {
                    correct += 1;
                }
            }
        }

        correct as f64 / total as f64
    }
}

fn compute_centroid(vectors: &[Vec<f64>], n_features: usize) -> Vec<f64> {
    let n = vectors.len() as f64;
    (0..n_features)
        .map(|j| vectors.iter().map(|v| v[j]).sum::<f64>() / n)
        .collect()
}

fn normalized_distance(a: &[f64], b: &[f64], sds: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .zip(sds.iter())
        .map(|((ai, bi), sd)| ((ai - bi) / sd).powi(2))
        .sum::<f64>()
        .sqrt()
}

// ---------------------------------------------------------------------------
// Full classification pipeline
// ---------------------------------------------------------------------------

/// Run full classification pipeline on consciousness experiment epochs.
pub fn classify_epochs(
    baseline_epochs: &[Vec<u8>],
    intention_epochs: &[Vec<u8>],
) -> Option<ClassificationReport> {
    if baseline_epochs.len() < 2 || intention_epochs.len() < 2 {
        return None;
    }

    let baseline_features: Vec<Vec<f64>> = baseline_epochs
        .iter()
        .map(|data| extract_features(data))
        .collect();

    let intention_features: Vec<Vec<f64>> = intention_epochs
        .iter()
        .map(|data| extract_features(data))
        .collect();

    let classifier = NearestCentroidClassifier::train(&baseline_features, &intention_features)?;

    // Classify all epochs
    let mut classifications = Vec::new();
    let mut correct = 0;
    let mut true_pos = 0;
    let mut true_neg = 0;

    for features in &baseline_features {
        let result = classifier.classify(features);
        if result.predicted_class == "baseline" {
            correct += 1;
            true_neg += 1;
        }
        classifications.push(result);
    }

    for features in &intention_features {
        let result = classifier.classify(features);
        if result.predicted_class == "intention" {
            correct += 1;
            true_pos += 1;
        }
        classifications.push(result);
    }

    let total = baseline_features.len() + intention_features.len();
    let accuracy = correct as f64 / total as f64;
    let tpr = true_pos as f64 / intention_features.len() as f64;
    let tnr = true_neg as f64 / baseline_features.len() as f64;

    let cv_accuracy =
        NearestCentroidClassifier::loocv_accuracy(&baseline_features, &intention_features);

    let feature_importance = classifier.feature_importance();

    Some(ClassificationReport {
        classifications,
        accuracy,
        true_positive_rate: tpr,
        true_negative_rate: tnr,
        cv_accuracy,
        feature_importance,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_features_empty() {
        let features = extract_features(&[]);
        assert_eq!(features.len(), 10);
    }

    #[test]
    fn test_extract_features_basic() {
        let data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        let features = extract_features(&data);
        assert_eq!(features.len(), 10);
        // Mean should be ~127.5
        assert!((features[0] - 127.5).abs() < 1.0);
    }

    #[test]
    fn test_max_run_length() {
        assert_eq!(max_run_length(&[1, 1, 1, 2, 2]), 3);
        assert_eq!(max_run_length(&[1, 2, 3, 4, 5]), 1);
        assert_eq!(max_run_length(&[7, 7, 7, 7]), 4);
    }

    #[test]
    fn test_nearest_centroid_basic() {
        let baseline = vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
        let intention = vec![vec![5.0, 5.0], vec![6.0, 5.0], vec![5.0, 6.0]];

        let clf = NearestCentroidClassifier::train(&baseline, &intention).unwrap();

        let result = clf.classify(&[0.5, 0.5]);
        assert_eq!(result.predicted_class, "baseline");

        let result = clf.classify(&[5.5, 5.5]);
        assert_eq!(result.predicted_class, "intention");
    }

    #[test]
    fn test_feature_importance() {
        let baseline = vec![vec![0.0, 5.0], vec![1.0, 5.0], vec![0.0, 5.0]];
        let intention = vec![vec![10.0, 5.0], vec![11.0, 5.0], vec![10.0, 5.0]];

        let clf = NearestCentroidClassifier::train(&baseline, &intention).unwrap();
        let importance = clf.feature_importance();

        // First feature should be more important (big difference)
        assert!(importance[0].1 > importance[1].1);
    }

    #[test]
    fn test_loocv_separable() {
        let baseline = vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 1.0],
        ];
        let intention = vec![
            vec![10.0, 10.0],
            vec![11.0, 10.0],
            vec![10.0, 11.0],
            vec![11.0, 11.0],
        ];

        let acc = NearestCentroidClassifier::loocv_accuracy(&baseline, &intention);
        assert_eq!(acc, 1.0); // Perfectly separable
    }

    #[test]
    fn test_classify_epochs_too_few() {
        let baseline = vec![vec![1u8; 100]];
        let intention = vec![vec![2u8; 100]];
        assert!(classify_epochs(&baseline, &intention).is_none());
    }
}
