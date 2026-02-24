//! Temporal analysis for consciousness-RNG experiments.
//!
//! Examines *when* during a trial block the consciousness effect appears,
//! how quickly it onsets, and how it decays. PEAR Lab found effects were
//! strongest in the first few seconds of intention.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Autocorrelation of Z-score time series within a phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZSeriesAutocorrelation {
    /// Lag and corresponding autocorrelation coefficient.
    pub lags: Vec<(usize, f64)>,
    /// Maximum absolute autocorrelation and its lag.
    pub max_abs_corr: f64,
    pub max_abs_lag: usize,
    /// Significance threshold (2/sqrt(n)).
    pub threshold: f64,
}

/// Sliding-window peak effect detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeakEffectWindow {
    /// Starting trial index of the peak window.
    pub start_index: usize,
    /// Window size (number of trials).
    pub window_size: usize,
    /// Mean Z in the peak window.
    pub mean_z: f64,
    /// Stouffer Z for the peak window.
    pub stouffer_z: f64,
    /// Fraction of phase elapsed at peak start (0.0 = beginning).
    pub phase_fraction: f64,
}

/// CUSUM change-point detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnsetDetection {
    /// Detected change-point index (None if no clear onset).
    pub change_point: Option<usize>,
    /// CUSUM statistic at change point.
    pub cusum_value: f64,
    /// Threshold used for detection.
    pub threshold: f64,
    /// Mean Z before change point.
    pub pre_onset_mean_z: f64,
    /// Mean Z after change point.
    pub post_onset_mean_z: f64,
}

/// Exponential decay fit to absolute Z-score series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayAnalysis {
    /// Decay rate (lambda). Positive = effect decays over time.
    pub decay_rate: f64,
    /// Half-life in trial units. NaN if no decay detected.
    pub half_life_trials: f64,
    /// R-squared of the exponential fit.
    pub r_squared: f64,
    /// Interpretation string.
    pub interpretation: String,
}

/// Complete temporal analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalResult {
    /// Per-phase Z-score time series.
    pub phase_z_series: Vec<PhaseZSeries>,
    /// Per-phase autocorrelation analysis.
    pub autocorrelations: Vec<ZSeriesAutocorrelation>,
    /// Peak effect windows per phase.
    pub peak_windows: Vec<PeakEffectWindow>,
    /// Onset detection per intention phase.
    pub onset_detections: Vec<OnsetDetection>,
    /// Decay analysis per intention phase.
    pub decay_analyses: Vec<DecayAnalysis>,
    /// Overall temporal signature interpretation.
    pub interpretation: String,
}

/// Z-score series for one phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseZSeries {
    pub direction: String,
    pub z_scores: Vec<f64>,
    pub cumulative_z: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Computation functions
// ---------------------------------------------------------------------------

/// Compute autocorrelation of a Z-score time series.
pub fn autocorrelation_z_series(z_scores: &[f64], max_lag: usize) -> ZSeriesAutocorrelation {
    let n = z_scores.len();
    if n < 4 {
        return ZSeriesAutocorrelation {
            lags: vec![],
            max_abs_corr: 0.0,
            max_abs_lag: 0,
            threshold: 1.0,
        };
    }

    let mean = z_scores.iter().sum::<f64>() / n as f64;
    let var: f64 = z_scores.iter().map(|&z| (z - mean).powi(2)).sum::<f64>() / n as f64;
    if var < 1e-20 {
        return ZSeriesAutocorrelation {
            lags: vec![],
            max_abs_corr: 0.0,
            max_abs_lag: 0,
            threshold: 2.0 / (n as f64).sqrt(),
        };
    }

    let effective_max_lag = max_lag.min(n / 4);
    let mut lags = Vec::new();
    let mut max_abs_corr = 0.0;
    let mut max_abs_lag = 0;

    for lag in 1..=effective_max_lag {
        let mut cov = 0.0;
        for i in 0..(n - lag) {
            cov += (z_scores[i] - mean) * (z_scores[i + lag] - mean);
        }
        let r = cov / (n as f64 * var);
        lags.push((lag, r));
        if r.abs() > max_abs_corr {
            max_abs_corr = r.abs();
            max_abs_lag = lag;
        }
    }

    ZSeriesAutocorrelation {
        lags,
        max_abs_corr,
        max_abs_lag,
        threshold: 2.0 / (n as f64).sqrt(),
    }
}

/// Find the sliding window with the strongest effect.
pub fn find_peak_effect_window(z_scores: &[f64], window_size: usize) -> PeakEffectWindow {
    let n = z_scores.len();
    let ws = window_size.min(n).max(1);

    if n == 0 {
        return PeakEffectWindow {
            start_index: 0,
            window_size: 0,
            mean_z: 0.0,
            stouffer_z: 0.0,
            phase_fraction: 0.0,
        };
    }

    let mut best_start = 0;
    let mut best_abs_mean = 0.0f64;

    for start in 0..=(n - ws) {
        let window = &z_scores[start..start + ws];
        let mean = window.iter().sum::<f64>() / ws as f64;
        if mean.abs() > best_abs_mean {
            best_abs_mean = mean.abs();
            best_start = start;
        }
    }

    let window = &z_scores[best_start..best_start + ws];
    let mean_z = window.iter().sum::<f64>() / ws as f64;
    let stouffer = crate::consciousness::stouffer_z(window);
    let phase_fraction = if n > 1 {
        best_start as f64 / (n - 1) as f64
    } else {
        0.0
    };

    PeakEffectWindow {
        start_index: best_start,
        window_size: ws,
        mean_z,
        stouffer_z: stouffer,
        phase_fraction,
    }
}

/// CUSUM change-point detection for onset of consciousness effect.
///
/// Detects the trial at which the Z-score series shifts away from zero.
/// Uses a one-sided CUSUM targeting positive shift (for High phases) or
/// negative shift (for Low phases).
pub fn detect_onset(z_scores: &[f64], expected_shift: f64) -> OnsetDetection {
    let n = z_scores.len();
    if n < 4 {
        return OnsetDetection {
            change_point: None,
            cusum_value: 0.0,
            threshold: 0.0,
            pre_onset_mean_z: 0.0,
            post_onset_mean_z: 0.0,
        };
    }

    // CUSUM: accumulate deviations from 0, with allowance k = |expected_shift|/2
    let k = expected_shift.abs() / 2.0;
    let threshold = 4.0; // standard CUSUM threshold

    let mut cusum = 0.0f64;
    let mut max_cusum = 0.0f64;
    let mut change_point: Option<usize> = None;

    for (i, &z) in z_scores.iter().enumerate() {
        let deviation = if expected_shift >= 0.0 {
            z - k
        } else {
            -z - k
        };
        cusum = (cusum + deviation).max(0.0);
        if cusum > max_cusum {
            max_cusum = cusum;
        }
        if cusum > threshold && change_point.is_none() {
            change_point = Some(i);
        }
    }

    let (pre_mean, post_mean) = if let Some(cp) = change_point {
        let pre = if cp > 0 {
            z_scores[..cp].iter().sum::<f64>() / cp as f64
        } else {
            0.0
        };
        let post = z_scores[cp..].iter().sum::<f64>() / (n - cp) as f64;
        (pre, post)
    } else {
        let mean = z_scores.iter().sum::<f64>() / n as f64;
        (mean, mean)
    };

    OnsetDetection {
        change_point,
        cusum_value: max_cusum,
        threshold,
        pre_onset_mean_z: pre_mean,
        post_onset_mean_z: post_mean,
    }
}

/// Exponential decay analysis of absolute Z-scores.
///
/// Fits |Z_i| ~ A * exp(-lambda * i) using log-linear regression.
/// Positive lambda means the effect decays over time.
pub fn analyze_decay(z_scores: &[f64]) -> DecayAnalysis {
    let n = z_scores.len();
    if n < 4 {
        return DecayAnalysis {
            decay_rate: 0.0,
            half_life_trials: f64::NAN,
            r_squared: 0.0,
            interpretation: "insufficient data".to_string(),
        };
    }

    // Use |Z| values, skip zeros
    let log_data: Vec<(f64, f64)> = z_scores
        .iter()
        .enumerate()
        .filter(|(_, z)| z.abs() > 0.01)
        .map(|(i, z)| (i as f64, z.abs().ln()))
        .collect();

    if log_data.len() < 3 {
        return DecayAnalysis {
            decay_rate: 0.0,
            half_life_trials: f64::NAN,
            r_squared: 0.0,
            interpretation: "insufficient non-zero values".to_string(),
        };
    }

    // Linear regression: ln|Z| = ln(A) - lambda * i
    let n_pts = log_data.len() as f64;
    let sum_x: f64 = log_data.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = log_data.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = log_data.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = log_data.iter().map(|(x, _)| x * x).sum();

    let denom = n_pts * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-20 {
        return DecayAnalysis {
            decay_rate: 0.0,
            half_life_trials: f64::NAN,
            r_squared: 0.0,
            interpretation: "degenerate regression".to_string(),
        };
    }

    let slope = (n_pts * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_pts;
    let decay_rate = -slope; // lambda = -slope

    // R-squared
    let mean_y = sum_y / n_pts;
    let ss_tot: f64 = log_data.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();
    let ss_res: f64 = log_data
        .iter()
        .map(|(x, y)| (y - (intercept + slope * x)).powi(2))
        .sum();
    let r_squared = if ss_tot > 1e-20 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    let half_life = if decay_rate > 0.001 {
        (2.0_f64.ln()) / decay_rate
    } else {
        f64::NAN
    };

    let interpretation = if decay_rate > 0.05 && r_squared > 0.3 {
        format!(
            "effect decays — half-life {:.1} trials (R²={:.2})",
            half_life, r_squared
        )
    } else if decay_rate < -0.05 && r_squared > 0.3 {
        format!(
            "effect strengthens over time (R²={:.2})",
            r_squared
        )
    } else {
        format!("no clear temporal trend (R²={:.2})", r_squared)
    };

    DecayAnalysis {
        decay_rate,
        half_life_trials: half_life,
        r_squared,
        interpretation,
    }
}

/// Build cumulative Z-score series from per-trial Z-scores.
pub fn cumulative_z_series(z_scores: &[f64]) -> Vec<f64> {
    let mut cumulative = Vec::with_capacity(z_scores.len());
    for i in 1..=z_scores.len() {
        cumulative.push(crate::consciousness::stouffer_z(&z_scores[..i]));
    }
    cumulative
}

/// Compute full temporal analysis from phase results.
pub fn compute_temporal(
    phases: &[crate::consciousness::PhaseResult],
) -> TemporalResult {
    let mut phase_z_series_vec = Vec::new();
    let mut autocorrelations = Vec::new();
    let mut peak_windows = Vec::new();
    let mut onset_detections = Vec::new();
    let mut decay_analyses = Vec::new();

    for phase in phases {
        let z_scores: Vec<f64> = phase.trials.iter().map(|t| t.pooled_z).collect();
        let cumulative = cumulative_z_series(&z_scores);

        phase_z_series_vec.push(PhaseZSeries {
            direction: phase.direction.to_string(),
            z_scores: z_scores.clone(),
            cumulative_z: cumulative,
        });

        // Autocorrelation
        let acf = autocorrelation_z_series(&z_scores, 20);
        autocorrelations.push(acf);

        // Peak window (20% of trials)
        let window_size = (z_scores.len() / 5).max(3);
        let peak = find_peak_effect_window(&z_scores, window_size);
        peak_windows.push(peak);

        // Onset and decay (only for intention phases)
        if phase.direction != crate::consciousness::IntentionDirection::Baseline {
            let expected_shift = match phase.direction {
                crate::consciousness::IntentionDirection::High => 0.5,
                crate::consciousness::IntentionDirection::Low => -0.5,
                _ => 0.0,
            };
            let onset = detect_onset(&z_scores, expected_shift);
            onset_detections.push(onset);

            let decay = analyze_decay(&z_scores);
            decay_analyses.push(decay);
        }
    }

    // Overall interpretation
    let early_bias = peak_windows
        .iter()
        .filter(|p| p.phase_fraction < 0.3 && p.stouffer_z.abs() > 1.0)
        .count();
    let has_decay = decay_analyses.iter().any(|d| d.decay_rate > 0.05 && d.r_squared > 0.3);
    let has_onset = onset_detections.iter().any(|o| o.change_point.is_some());

    let interpretation = if early_bias > 0 && has_decay {
        "Classic PEAR pattern: strong early effect with temporal decay".to_string()
    } else if has_onset {
        "Clear onset detected — effect appears after initial stabilization".to_string()
    } else if early_bias > 0 {
        "Peak effect concentrated in early trials".to_string()
    } else {
        "No strong temporal pattern detected".to_string()
    };

    TemporalResult {
        phase_z_series: phase_z_series_vec,
        autocorrelations,
        peak_windows,
        onset_detections,
        decay_analyses,
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
    fn autocorrelation_empty() {
        let acf = autocorrelation_z_series(&[], 10);
        assert!(acf.lags.is_empty());
    }

    #[test]
    fn autocorrelation_constant() {
        let z = vec![1.0; 20];
        let acf = autocorrelation_z_series(&z, 5);
        // Constant series has zero variance → empty lags
        assert!(acf.lags.is_empty() || acf.max_abs_corr < 0.01);
    }

    #[test]
    fn autocorrelation_alternating() {
        let z: Vec<f64> = (0..40).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let acf = autocorrelation_z_series(&z, 5);
        assert!(!acf.lags.is_empty());
        // Lag 1 should be strongly negative
        if let Some(&(_, r)) = acf.lags.first() {
            assert!(r < -0.5, "lag-1 autocorrelation = {r}");
        }
    }

    #[test]
    fn peak_window_empty() {
        let peak = find_peak_effect_window(&[], 5);
        assert_eq!(peak.window_size, 0);
    }

    #[test]
    fn peak_window_all_positive() {
        let z = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        let peak = find_peak_effect_window(&z, 3);
        // Peak should be at the end (highest values)
        assert!(peak.start_index >= 5, "peak start = {}", peak.start_index);
        assert!(peak.mean_z > 0.0);
    }

    #[test]
    fn peak_window_early_spike() {
        let mut z = vec![0.0; 20];
        z[0] = 3.0;
        z[1] = 2.5;
        z[2] = 2.0;
        let peak = find_peak_effect_window(&z, 3);
        assert_eq!(peak.start_index, 0);
        assert!(peak.phase_fraction < 0.1);
    }

    #[test]
    fn onset_detection_no_signal() {
        let z = vec![0.1, -0.1, 0.05, -0.05, 0.02, -0.02, 0.01, -0.01];
        let onset = detect_onset(&z, 0.5);
        assert!(onset.change_point.is_none());
    }

    #[test]
    fn onset_detection_clear_shift() {
        // First 10 trials near zero, then 10 trials at +2.0
        let mut z = vec![0.1; 10];
        z.extend(vec![2.0; 10]);
        let onset = detect_onset(&z, 1.0);
        // Should detect change around index 10-12
        if let Some(cp) = onset.change_point {
            assert!(cp >= 8 && cp <= 15, "change point = {cp}");
            assert!(onset.post_onset_mean_z > onset.pre_onset_mean_z);
        }
    }

    #[test]
    fn onset_detection_small_data() {
        let z = vec![1.0, 2.0];
        let onset = detect_onset(&z, 0.5);
        assert!(onset.change_point.is_none());
    }

    #[test]
    fn decay_analysis_decaying_signal() {
        // Z-scores that decay: 2.0, 1.5, 1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1
        let z = vec![2.0, 1.5, 1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.15, 0.1];
        let decay = analyze_decay(&z);
        assert!(decay.decay_rate > 0.0, "decay_rate = {}", decay.decay_rate);
    }

    #[test]
    fn decay_analysis_flat_signal() {
        let z = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let decay = analyze_decay(&z);
        assert!(decay.decay_rate.abs() < 0.1, "decay_rate = {}", decay.decay_rate);
    }

    #[test]
    fn decay_analysis_empty() {
        let decay = analyze_decay(&[]);
        assert_eq!(decay.decay_rate, 0.0);
    }

    #[test]
    fn cumulative_z_series_basic() {
        let z = vec![1.0, 1.0, 1.0, 1.0];
        let cumz = cumulative_z_series(&z);
        assert_eq!(cumz.len(), 4);
        // First value: stouffer([1.0]) = 1.0
        assert!((cumz[0] - 1.0).abs() < 1e-10);
        // Last value: stouffer([1,1,1,1]) = 4/sqrt(4) = 2.0
        assert!((cumz[3] - 2.0).abs() < 1e-10);
    }
}
