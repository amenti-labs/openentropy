//! Verdict functions for entropy analysis metrics.
//!
//! Each function evaluates a metric value against domain-specific thresholds
//! and returns a [`Verdict`] (PASS / WARN / FAIL / NA). This module
//! centralises threshold logic so that both the CLI and the Python SDK
//! produce identical verdicts without duplicating constants.
//!
//! # Adding a new verdict
//!
//! 1. Add a `pub fn verdict_<metric>(...) -> Verdict` here.
//! 2. Call it from the CLI formatting layer.
//! 3. Expose it via Python bindings if needed.

use serde::Serialize;

use crate::analysis::RunsResult;

// ---------------------------------------------------------------------------
// Verdict enum
// ---------------------------------------------------------------------------

/// Result of evaluating a metric against quality thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Verdict {
    /// Metric is within expected range for genuine randomness.
    Pass,
    /// Metric is borderline — not a definitive failure.
    Warn,
    /// Metric is outside the expected range.
    Fail,
    /// Metric could not be computed (NaN, insufficient data, etc.).
    #[serde(rename = "N/A")]
    Na,
}

impl Verdict {
    /// Short display string for CLI tables.
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Warn => "WARN",
            Verdict::Fail => "FAIL",
            Verdict::Na => "N/A",
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Forensic analysis verdicts
// ---------------------------------------------------------------------------

/// Autocorrelation: max |r| across all lags.
pub fn verdict_autocorr(max_abs: f64) -> Verdict {
    if max_abs > 0.15 {
        Verdict::Fail
    } else if max_abs > 0.05 {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Spectral flatness (1.0 = white noise).
pub fn verdict_spectral(flatness: f64) -> Verdict {
    if flatness < 0.5 {
        Verdict::Fail
    } else if flatness < 0.75 {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Bit bias: overall deviation from 0.5 and per-bit significance.
pub fn verdict_bias(overall: f64, has_significant: bool) -> Verdict {
    if overall > 0.02 {
        Verdict::Fail
    } else if has_significant {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Distribution uniformity via KS p-value.
pub fn verdict_distribution(ks_p: f64) -> Verdict {
    if ks_p < 0.001 {
        Verdict::Fail
    } else if ks_p < 0.01 {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Stationarity: ANOVA F-statistic across sliding windows.
pub fn verdict_stationarity(f_stat: f64, is_stationary: bool) -> Verdict {
    if f_stat > 3.0 {
        Verdict::Fail
    } else if !is_stationary {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Runs analysis: longest run ratio and total-runs deviation.
pub fn verdict_runs(ru: &RunsResult, _sample_size: usize) -> Verdict {
    let longest_ratio = if ru.expected_longest_run > 0.0 {
        ru.longest_run as f64 / ru.expected_longest_run
    } else {
        1.0
    };
    let runs_dev = if ru.expected_runs > 0.0 {
        (ru.total_runs as f64 - ru.expected_runs).abs() / ru.expected_runs
    } else {
        0.0
    };
    if longest_ratio > 3.0 || runs_dev > 0.4 {
        Verdict::Fail
    } else if longest_ratio > 2.0 || runs_dev > 0.2 {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

// ---------------------------------------------------------------------------
// Chaos theory verdicts
// ---------------------------------------------------------------------------

/// Hurst exponent: H ≈ 0.5 indicates random walk (no long-range dependence).
pub fn verdict_hurst(h: f64) -> Verdict {
    if !h.is_finite() {
        return Verdict::Na;
    }
    if (0.4..=0.6).contains(&h) {
        Verdict::Pass
    } else if (0.3..=0.7).contains(&h) {
        Verdict::Warn
    } else {
        Verdict::Fail
    }
}

/// Lyapunov exponent: λ ≈ 0 indicates no deterministic chaos.
pub fn verdict_lyapunov(l: f64) -> Verdict {
    if !l.is_finite() {
        return Verdict::Na;
    }
    if l.abs() < 0.1 {
        Verdict::Pass
    } else if l.abs() < 0.2 {
        Verdict::Warn
    } else {
        Verdict::Fail
    }
}

/// Correlation dimension: high D₂ indicates high-dimensional (random) attractor.
pub fn verdict_corrdim(d: f64) -> Verdict {
    if !d.is_finite() {
        return Verdict::Na;
    }
    if d > 3.0 {
        Verdict::Pass
    } else if d > 2.0 {
        Verdict::Warn
    } else {
        Verdict::Fail
    }
}

/// BiEntropy: high values indicate maximal binary entropy.
pub fn verdict_bientropy(b: f64) -> Verdict {
    if !b.is_finite() {
        return Verdict::Na;
    }
    if b > 0.95 {
        Verdict::Pass
    } else if b > 0.90 {
        Verdict::Warn
    } else {
        Verdict::Fail
    }
}

/// Epiplexity (compression ratio): ratio ≈ 1.0 means incompressible (random).
pub fn verdict_compression(c: f64) -> Verdict {
    if !c.is_finite() {
        return Verdict::Na;
    }
    if c > 0.99 {
        Verdict::Pass
    } else if c > 0.95 {
        Verdict::Warn
    } else {
        Verdict::Fail
    }
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

/// Format a metric value for display, showing "N/A" if invalid or non-finite.
pub fn metric_or_na(value: f64, is_valid: bool) -> String {
    if is_valid && value.is_finite() {
        format!("{value:.4}")
    } else {
        "N/A".to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_enum_display() {
        assert_eq!(Verdict::Pass.as_str(), "PASS");
        assert_eq!(Verdict::Warn.as_str(), "WARN");
        assert_eq!(Verdict::Fail.as_str(), "FAIL");
        assert_eq!(Verdict::Na.as_str(), "N/A");
        assert_eq!(format!("{}", Verdict::Pass), "PASS");
    }

    #[test]
    fn autocorr_thresholds() {
        assert_eq!(verdict_autocorr(0.01), Verdict::Pass);
        assert_eq!(verdict_autocorr(0.10), Verdict::Warn);
        assert_eq!(verdict_autocorr(0.20), Verdict::Fail);
    }

    #[test]
    fn spectral_thresholds() {
        assert_eq!(verdict_spectral(0.90), Verdict::Pass);
        assert_eq!(verdict_spectral(0.60), Verdict::Warn);
        assert_eq!(verdict_spectral(0.30), Verdict::Fail);
    }

    #[test]
    fn hurst_thresholds() {
        assert_eq!(verdict_hurst(0.50), Verdict::Pass);
        assert_eq!(verdict_hurst(0.35), Verdict::Warn);
        assert_eq!(verdict_hurst(0.10), Verdict::Fail);
        assert_eq!(verdict_hurst(f64::NAN), Verdict::Na);
    }

    #[test]
    fn lyapunov_thresholds() {
        assert_eq!(verdict_lyapunov(0.05), Verdict::Pass);
        assert_eq!(verdict_lyapunov(0.15), Verdict::Warn);
        assert_eq!(verdict_lyapunov(0.50), Verdict::Fail);
        assert_eq!(verdict_lyapunov(f64::NAN), Verdict::Na);
    }

    #[test]
    fn corrdim_thresholds() {
        assert_eq!(verdict_corrdim(5.0), Verdict::Pass);
        assert_eq!(verdict_corrdim(2.5), Verdict::Warn);
        assert_eq!(verdict_corrdim(1.5), Verdict::Fail);
        assert_eq!(verdict_corrdim(f64::NAN), Verdict::Na);
    }

    #[test]
    fn bientropy_thresholds() {
        assert_eq!(verdict_bientropy(0.98), Verdict::Pass);
        assert_eq!(verdict_bientropy(0.92), Verdict::Warn);
        assert_eq!(verdict_bientropy(0.80), Verdict::Fail);
        assert_eq!(verdict_bientropy(f64::NAN), Verdict::Na);
    }

    #[test]
    fn compression_thresholds() {
        assert_eq!(verdict_compression(1.00), Verdict::Pass);
        assert_eq!(verdict_compression(0.97), Verdict::Warn);
        assert_eq!(verdict_compression(0.90), Verdict::Fail);
        assert_eq!(verdict_compression(f64::NAN), Verdict::Na);
    }

    #[test]
    fn metric_display() {
        assert_eq!(metric_or_na(0.1234, true), "0.1234");
        assert_eq!(metric_or_na(0.1234, false), "N/A");
        assert_eq!(metric_or_na(f64::NAN, true), "N/A");
    }
}
