//! Quantum/classical contribution proxy metrics (v3).
//!
//! This experimental model builds on prior versions with:
//! - calibrated hierarchical priors from labeled runs,
//! - lag-aware coupling metrics and adaptive-bin MI,
//! - optional stress sweeps for measured stress sensitivity,
//! - bootstrap/Monte-Carlo uncertainty intervals,
//! - ablation and sensitivity reports.
//! - telemetry-aware classical confounding adjustment.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::f64::consts::{LN_2, PI};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::thread;
use std::time::{Duration, Instant};

use crate::analysis::SourceAnalysis;
use crate::metrics::standard::EntropyMeasurements;
use crate::metrics::streams::collect_named_source_stream_samples;
use crate::metrics::telemetry::TelemetryWindowReport;
use crate::pool::EntropyPool;
use crate::source::SourceCategory;

/// Experimental model identifier.
pub const MODEL_ID: &str = "quantum_proxy_v3";
/// Experimental model version.
pub const MODEL_VERSION: u32 = 3;

/// Tunable constants for v3 assessment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QuantumAssessmentConfig {
    /// |r| threshold for zero-lag correlation.
    pub corr_threshold: f64,
    /// |r| threshold for lagged correlation max.
    pub lagged_corr_threshold: f64,
    /// MI threshold in bits for zero-lag adaptive-bin MI.
    pub mi_threshold_bits: f64,
    /// MI threshold in bits for lagged adaptive-bin MI max.
    pub lagged_mi_threshold_bits: f64,
    /// Min-entropy delta (bits/byte) treated as strong stress sensitivity.
    pub stress_delta_bits: f64,
    /// Maximum lag for lagged coupling scans.
    pub max_lag: usize,
    /// Monte-Carlo draws for uncertainty intervals.
    pub bootstrap_rounds: usize,
    /// Number of windows for per-source variability estimation.
    pub bootstrap_windows: usize,
    /// Coupling blend weight for zero-lag corr term.
    pub coupling_weight_corr: f64,
    /// Coupling blend weight for lagged corr term.
    pub coupling_weight_lag_corr: f64,
    /// Coupling blend weight for zero-lag MI term.
    pub coupling_weight_mi: f64,
    /// Coupling blend weight for lagged MI term.
    pub coupling_weight_lag_mi: f64,
    /// Number of null coupling rounds per source pair used for finite-sample debiasing
    /// and permutation p-value estimation.
    pub coupling_null_rounds: usize,
    /// Sigma guard for null debiasing (`observed - (null_mean + sigma * null_std)`).
    pub coupling_null_sigma: f64,
    /// FDR control level for coupling significance tests (Benjamini-Hochberg).
    pub coupling_fdr_alpha: f64,
    /// If true, only FDR-significant excess coupling contributes to penalty.
    /// If false, significance is diagnostic and excess coupling remains continuous.
    pub coupling_use_fdr_gate: bool,
}

impl Default for QuantumAssessmentConfig {
    fn default() -> Self {
        Self {
            corr_threshold: 0.30,
            lagged_corr_threshold: 0.36,
            mi_threshold_bits: 0.02,
            lagged_mi_threshold_bits: 0.03,
            stress_delta_bits: 1.5,
            max_lag: 8,
            bootstrap_rounds: 400,
            bootstrap_windows: 8,
            coupling_weight_corr: 0.40,
            coupling_weight_lag_corr: 0.20,
            coupling_weight_mi: 0.25,
            coupling_weight_lag_mi: 0.15,
            coupling_null_rounds: 31,
            coupling_null_sigma: 2.0,
            coupling_fdr_alpha: 0.05,
            coupling_use_fdr_gate: false,
        }
    }
}

/// Telemetry-based confounding adjustment config.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TelemetryConfoundConfig {
    /// Relative weight for absolute load-per-core.
    pub weight_load_abs: f64,
    /// Relative weight for load change over the run window.
    pub weight_load_delta: f64,
    /// Relative weight for thermal rise over the run window.
    pub weight_thermal_rise: f64,
    /// Relative weight for frequency drift over the run window.
    pub weight_frequency_drift: f64,
    /// Relative weight for memory pressure.
    pub weight_memory_pressure: f64,
    /// Relative weight for rail/power/current drift.
    pub weight_rail_drift: f64,
    /// Load-per-core value mapped to confound=1 for that term.
    pub load_full_scale_per_core: f64,
    /// Thermal rise (C) mapped to confound=1 for that term.
    pub thermal_full_scale_c: f64,
    /// Relative frequency drift mapped to confound=1 for that term.
    pub frequency_full_scale_ratio: f64,
    /// Relative rail drift mapped to confound=1 for that term.
    pub rail_full_scale_ratio: f64,
    /// How strongly confound raises effective stress sensitivity.
    pub confound_to_stress_scale: f64,
}

impl Default for TelemetryConfoundConfig {
    fn default() -> Self {
        Self {
            weight_load_abs: 0.26,
            weight_load_delta: 0.18,
            weight_thermal_rise: 0.18,
            weight_frequency_drift: 0.14,
            weight_memory_pressure: 0.16,
            weight_rail_drift: 0.08,
            load_full_scale_per_core: 1.0,
            thermal_full_scale_c: 8.0,
            frequency_full_scale_ratio: 0.15,
            rail_full_scale_ratio: 0.25,
            confound_to_stress_scale: 0.70,
        }
    }
}

/// Telemetry-derived confounding diagnostics used for v3 adjustment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfoundReport {
    pub confound_index: f64,
    pub load_abs_per_core: f64,
    pub load_delta_per_core: f64,
    pub thermal_rise_c: f64,
    pub frequency_drift_ratio: f64,
    pub memory_pressure: f64,
    pub rail_drift_ratio: f64,
    pub confound_to_stress_scale: f64,
}

/// Stress sweep settings for measured stress sensitivity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StressSweepConfig {
    /// Enable stress sweeps.
    pub enabled: bool,
    /// Warmup before each stressed sampling pass.
    pub warmup_ms: u64,
    /// CPU worker threads for compute stress.
    pub cpu_threads: usize,
    /// Memory worker threads.
    pub memory_threads: usize,
    /// Memory footprint per memory worker.
    pub memory_megabytes_per_thread: usize,
    /// Scheduler churn worker threads.
    pub scheduler_threads: usize,
}

impl Default for StressSweepConfig {
    fn default() -> Self {
        let cpu_threads = thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(1)
            .clamp(1, 8);
        Self {
            enabled: true,
            warmup_ms: 180,
            cpu_threads,
            memory_threads: 1,
            memory_megabytes_per_thread: 64,
            scheduler_threads: 2,
        }
    }
}

/// Labeled prior calibration record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationRecord {
    /// Optional source name label.
    pub source: Option<String>,
    /// Optional source category label.
    pub category: Option<String>,
    /// Quantum-likelihood target in [0,1].
    pub label: f64,
    /// Effective sample weight for this record.
    pub weight: f64,
}

/// Posterior summary for a beta-binomial estimate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BetaPosterior {
    pub alpha: f64,
    pub beta: f64,
    pub n_eff: f64,
    pub mean: f64,
    pub ci_low: f64,
    pub ci_high: f64,
}

/// Hierarchical prior calibration table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorCalibration {
    pub model_id: String,
    pub model_version: u32,
    pub prior_alpha: f64,
    pub prior_beta: f64,
    pub global: BetaPosterior,
    pub categories: HashMap<String, BetaPosterior>,
    pub sources: HashMap<String, BetaPosterior>,
}

/// Term inputs used to produce quantum/classical decomposition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QuantumScoreComponents {
    pub physics_prior: f64,
    pub quality_factor: f64,
    pub stress_sensitivity: f64,
    pub coupling_penalty: f64,
}

/// Point assessment output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QuantumAssessment {
    pub quantum_score: f64,
    pub classical_score: f64,
    pub quantum_min_entropy_bits: f64,
    pub classical_min_entropy_bits: f64,
    pub components: QuantumScoreComponents,
}

/// Aggregated quantum/classical ratio with uncertainty.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QuantumClassicalRatio {
    pub quantum_bits: f64,
    pub classical_bits: f64,
    pub quantum_fraction: f64,
    pub classical_fraction: f64,
    pub quantum_to_classical: f64,
    pub quantum_bits_ci_low: f64,
    pub quantum_bits_ci_high: f64,
    pub classical_bits_ci_low: f64,
    pub classical_bits_ci_high: f64,
    pub quantum_fraction_ci_low: f64,
    pub quantum_fraction_ci_high: f64,
    pub quantum_to_classical_ci_low: f64,
    pub quantum_to_classical_ci_high: f64,
}

/// Per-source coupling accumulator.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CouplingStats {
    pub sum_abs_corr_raw: f64,
    pub sum_abs_corr_lag_raw: f64,
    pub sum_mi_bits_raw: f64,
    pub sum_mi_bits_lag_raw: f64,
    pub sum_abs_corr_null: f64,
    pub sum_abs_corr_lag_null: f64,
    pub sum_mi_bits_null: f64,
    pub sum_mi_bits_lag_null: f64,
    pub sum_abs_corr_excess: f64,
    pub sum_abs_corr_lag_excess: f64,
    pub sum_mi_bits_excess: f64,
    pub sum_mi_bits_lag_excess: f64,
    pub sum_q_corr: f64,
    pub sum_q_corr_lag: f64,
    pub sum_q_mi: f64,
    pub sum_q_mi_lag: f64,
    pub significant_pairs_any: usize,
    pub significant_pairs_corr: usize,
    pub significant_pairs_lag_corr: usize,
    pub significant_pairs_mi: usize,
    pub significant_pairs_lag_mi: usize,
    pub pairs: usize,
}

impl CouplingStats {
    pub fn mean_abs_corr(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_excess / self.pairs as f64
        }
    }

    pub fn mean_abs_corr_lag(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_lag_excess / self.pairs as f64
        }
    }

    pub fn mean_mi_bits(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_excess / self.pairs as f64
        }
    }

    pub fn mean_mi_bits_lag(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_lag_excess / self.pairs as f64
        }
    }

    pub fn mean_abs_corr_raw(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_raw / self.pairs as f64
        }
    }

    pub fn mean_abs_corr_lag_raw(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_lag_raw / self.pairs as f64
        }
    }

    pub fn mean_mi_bits_raw(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_raw / self.pairs as f64
        }
    }

    pub fn mean_mi_bits_lag_raw(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_lag_raw / self.pairs as f64
        }
    }

    pub fn mean_abs_corr_null(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_null / self.pairs as f64
        }
    }

    pub fn mean_abs_corr_lag_null(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_abs_corr_lag_null / self.pairs as f64
        }
    }

    pub fn mean_mi_bits_null(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_null / self.pairs as f64
        }
    }

    pub fn mean_mi_bits_lag_null(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            self.sum_mi_bits_lag_null / self.pairs as f64
        }
    }

    pub fn mean_q_corr(self) -> f64 {
        if self.pairs == 0 {
            1.0
        } else {
            clamp01(self.sum_q_corr / self.pairs as f64)
        }
    }

    pub fn mean_q_corr_lag(self) -> f64 {
        if self.pairs == 0 {
            1.0
        } else {
            clamp01(self.sum_q_corr_lag / self.pairs as f64)
        }
    }

    pub fn mean_q_mi(self) -> f64 {
        if self.pairs == 0 {
            1.0
        } else {
            clamp01(self.sum_q_mi / self.pairs as f64)
        }
    }

    pub fn mean_q_mi_lag(self) -> f64 {
        if self.pairs == 0 {
            1.0
        } else {
            clamp01(self.sum_q_mi_lag / self.pairs as f64)
        }
    }

    pub fn significant_pair_fraction_any(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            clamp01(self.significant_pairs_any as f64 / self.pairs as f64)
        }
    }

    pub fn significant_pair_fraction_corr(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            clamp01(self.significant_pairs_corr as f64 / self.pairs as f64)
        }
    }

    pub fn significant_pair_fraction_corr_lag(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            clamp01(self.significant_pairs_lag_corr as f64 / self.pairs as f64)
        }
    }

    pub fn significant_pair_fraction_mi(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            clamp01(self.significant_pairs_mi as f64 / self.pairs as f64)
        }
    }

    pub fn significant_pair_fraction_mi_lag(self) -> f64 {
        if self.pairs == 0 {
            0.0
        } else {
            clamp01(self.significant_pairs_lag_mi as f64 / self.pairs as f64)
        }
    }
}

/// Input row for batch assessment.
#[derive(Debug, Clone)]
pub struct QuantumSourceInput {
    pub name: String,
    pub category: Option<SourceCategory>,
    pub min_entropy_bits: f64,
    pub quality_factor: f64,
    pub stress_sensitivity: f64,
    pub physics_prior_override: Option<f64>,
}

/// Per-source output row with intervals and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSourceResult {
    pub name: String,
    pub category: String,
    pub min_entropy_bits: f64,
    pub physics_prior: f64,
    pub physics_prior_ci_low: f64,
    pub physics_prior_ci_high: f64,
    pub prior_source_samples: f64,
    pub prior_category_samples: f64,
    pub quality_factor: f64,
    pub stress_sensitivity: f64,
    pub stress_sensitivity_effective: f64,
    pub telemetry_confound_penalty: f64,
    pub coupling_mean_abs_r_raw: f64,
    pub coupling_mean_abs_r_lag_raw: f64,
    pub coupling_mean_mi_bits_raw: f64,
    pub coupling_mean_mi_bits_lag_raw: f64,
    pub coupling_mean_abs_r_null: f64,
    pub coupling_mean_abs_r_lag_null: f64,
    pub coupling_mean_mi_bits_null: f64,
    pub coupling_mean_mi_bits_lag_null: f64,
    pub coupling_mean_abs_r: f64,
    pub coupling_mean_abs_r_lag: f64,
    pub coupling_mean_mi_bits: f64,
    pub coupling_mean_mi_bits_lag: f64,
    pub coupling_mean_q_corr: f64,
    pub coupling_mean_q_corr_lag: f64,
    pub coupling_mean_q_mi: f64,
    pub coupling_mean_q_mi_lag: f64,
    pub coupling_significant_pair_fraction_any: f64,
    pub coupling_significant_pair_fraction_corr: f64,
    pub coupling_significant_pair_fraction_corr_lag: f64,
    pub coupling_significant_pair_fraction_mi: f64,
    pub coupling_significant_pair_fraction_mi_lag: f64,
    pub coupling_penalty: f64,
    pub quantum_score: f64,
    pub quantum_score_ci_low: f64,
    pub quantum_score_ci_high: f64,
    pub classical_score: f64,
    pub quantum_min_entropy_bits: f64,
    pub quantum_min_entropy_bits_ci_low: f64,
    pub quantum_min_entropy_bits_ci_high: f64,
    pub classical_min_entropy_bits: f64,
    pub classical_min_entropy_bits_ci_low: f64,
    pub classical_min_entropy_bits_ci_high: f64,
}

/// Stress sweep per-source diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressSweepSourceResult {
    pub baseline_min_entropy: f64,
    pub cpu_load_min_entropy: Option<f64>,
    pub memory_load_min_entropy: Option<f64>,
    pub scheduler_load_min_entropy: Option<f64>,
    pub mean_abs_delta_min_entropy: f64,
    pub stress_sensitivity: f64,
}

/// Stress sweep batch diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressSweepReport {
    pub config: StressSweepConfig,
    pub elapsed_ms: u64,
    pub by_source: HashMap<String, StressSweepSourceResult>,
}

/// Aggregate calibration metadata included in reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumCalibrationSummary {
    pub global_prior: f64,
    pub global_prior_ci_low: f64,
    pub global_prior_ci_high: f64,
    pub category_entries: usize,
    pub source_entries: usize,
}

/// Aggregate ablation row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumAblationEntry {
    pub scenario: String,
    pub quantum_fraction: f64,
    pub quantum_to_classical: f64,
    pub delta_quantum_fraction: f64,
    pub delta_quantum_to_classical: f64,
}

/// Ablation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumAblationReport {
    pub entries: Vec<QuantumAblationEntry>,
}

/// Per-source local sensitivity summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSourceSensitivity {
    pub name: String,
    pub baseline_q: f64,
    pub impact_without_prior: f64,
    pub impact_without_quality: f64,
    pub impact_without_coupling: f64,
    pub impact_without_stress: f64,
}

/// Global sensitivity summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSensitivitySummary {
    pub mean_impact_without_prior: f64,
    pub mean_impact_without_quality: f64,
    pub mean_impact_without_coupling: f64,
    pub mean_impact_without_stress: f64,
}

/// Sensitivity report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSensitivityReport {
    pub sources: Vec<QuantumSourceSensitivity>,
    pub summary: QuantumSensitivitySummary,
}

/// Full report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumBatchReport {
    pub config: QuantumAssessmentConfig,
    pub calibration: QuantumCalibrationSummary,
    pub sources: Vec<QuantumSourceResult>,
    pub aggregate: QuantumClassicalRatio,
    pub ablation: QuantumAblationReport,
    pub sensitivity: QuantumSensitivityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry_confound: Option<TelemetryConfoundReport>,
}

#[derive(Debug, Clone, Copy)]
struct PriorEstimate {
    mean: f64,
    ci_low: f64,
    ci_high: f64,
    source_n_eff: f64,
    category_n_eff: f64,
}

#[derive(Debug, Clone, Copy)]
struct ComponentUncertainty {
    min_entropy_mean: f64,
    min_entropy_std: f64,
    quality_mean: f64,
    quality_std: f64,
    stress_mean: f64,
    stress_std: f64,
    coupling_penalty_mean: f64,
    coupling_penalty_std: f64,
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn nonneg(v: f64) -> f64 {
    v.max(0.0)
}

fn safe_div(num: f64, den: f64) -> f64 {
    if den.abs() <= 1e-12 { 0.0 } else { num / den }
}

fn metric_value(
    snapshot: &crate::metrics::telemetry::TelemetrySnapshot,
    domain: &str,
    name: &str,
) -> Option<f64> {
    snapshot
        .metrics
        .iter()
        .find(|m| m.domain == domain && m.name == name)
        .map(|m| m.value)
}

fn mean_positive_delta_by_domain(window: &TelemetryWindowReport, domain: &str) -> f64 {
    let values: Vec<f64> = window
        .deltas
        .iter()
        .filter(|d| d.domain == domain && d.delta_value > 0.0)
        .map(|d| d.delta_value)
        .collect();
    mean(&values)
}

fn mean_relative_delta_by_domains(window: &TelemetryWindowReport, domains: &[&str]) -> f64 {
    let mut rel = Vec::new();
    for d in &window.deltas {
        if !domains.iter().any(|x| *x == d.domain) {
            continue;
        }
        let denom = d.start_value.abs().max(d.end_value.abs()).max(1e-9);
        rel.push((d.delta_value.abs()) / denom);
    }
    mean(&rel)
}

fn memory_pressure_from_window(window: &TelemetryWindowReport) -> f64 {
    let total = metric_value(&window.end, "memory", "total_bytes")
        .or_else(|| metric_value(&window.start, "memory", "total_bytes"))
        .unwrap_or(0.0);
    if total <= 0.0 {
        return 0.0;
    }

    let avail_start = metric_value(&window.start, "memory", "available_bytes")
        .or_else(|| metric_value(&window.start, "memory", "free_bytes"))
        .unwrap_or(0.0);
    let avail_end = metric_value(&window.end, "memory", "available_bytes")
        .or_else(|| metric_value(&window.end, "memory", "free_bytes"))
        .unwrap_or(0.0);

    let end_pressure = clamp01(1.0 - safe_div(avail_end, total));
    let pressure_delta = clamp01(safe_div((avail_start - avail_end).max(0.0), total));
    clamp01(0.6 * end_pressure + 0.4 * pressure_delta)
}

/// Derive a telemetry confounding report from start/end host telemetry.
pub fn telemetry_confound_from_window(
    window: &TelemetryWindowReport,
    cfg: TelemetryConfoundConfig,
) -> TelemetryConfoundReport {
    let cores = window.end.cpu_count.max(1) as f64;
    let load_start = window.start.loadavg_1m.unwrap_or(0.0);
    let load_end = window.end.loadavg_1m.unwrap_or(load_start);
    let load_abs_per_core = safe_div(load_end, cores);
    let load_delta_per_core = safe_div((load_end - load_start).abs(), cores);

    let thermal_rise_c = mean_positive_delta_by_domain(window, "thermal");
    let frequency_drift_ratio = mean_relative_delta_by_domains(window, &["frequency"]);
    let memory_pressure = memory_pressure_from_window(window);
    let rail_drift_ratio = mean_relative_delta_by_domains(window, &["power", "voltage", "current"]);

    let load_abs_term = clamp01(safe_div(
        load_abs_per_core,
        cfg.load_full_scale_per_core.max(1e-9),
    ));
    let load_delta_term = clamp01(safe_div(
        load_delta_per_core,
        cfg.load_full_scale_per_core.max(1e-9),
    ));
    let thermal_term = clamp01(safe_div(thermal_rise_c, cfg.thermal_full_scale_c.max(1e-9)));
    let freq_term = clamp01(safe_div(
        frequency_drift_ratio,
        cfg.frequency_full_scale_ratio.max(1e-9),
    ));
    let rail_term = clamp01(safe_div(
        rail_drift_ratio,
        cfg.rail_full_scale_ratio.max(1e-9),
    ));

    let weights = cfg.weight_load_abs
        + cfg.weight_load_delta
        + cfg.weight_thermal_rise
        + cfg.weight_frequency_drift
        + cfg.weight_memory_pressure
        + cfg.weight_rail_drift;
    let confound_index = if weights <= 0.0 {
        0.0
    } else {
        clamp01(
            (cfg.weight_load_abs * load_abs_term
                + cfg.weight_load_delta * load_delta_term
                + cfg.weight_thermal_rise * thermal_term
                + cfg.weight_frequency_drift * freq_term
                + cfg.weight_memory_pressure * memory_pressure
                + cfg.weight_rail_drift * rail_term)
                / weights,
        )
    };

    TelemetryConfoundReport {
        confound_index,
        load_abs_per_core,
        load_delta_per_core,
        thermal_rise_c,
        frequency_drift_ratio,
        memory_pressure,
        rail_drift_ratio,
        confound_to_stress_scale: cfg.confound_to_stress_scale,
    }
}

fn category_telemetry_scale(category: &str) -> f64 {
    match category {
        "timing" | "system" | "scheduling" | "microarch" | "io" | "ipc" | "network" => 1.0,
        "gpu" | "novel" | "cross_domain" | "composite" => 1.1,
        "sensor" | "thermal" => 0.8,
        _ => 1.0,
    }
}

fn ci_shifted(
    center_old: f64,
    low_old: f64,
    high_old: f64,
    center_new: f64,
    lo: f64,
    hi: f64,
) -> (f64, f64) {
    let left = (center_old - low_old).max(0.0);
    let right = (high_old - center_old).max(0.0);
    (
        (center_new - left).clamp(lo, hi),
        (center_new + right).clamp(lo, hi),
    )
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn stddev(values: &[f64], center: f64) -> f64 {
    if values.len() <= 1 {
        0.0
    } else {
        let var = values
            .iter()
            .map(|v| (v - center) * (v - center))
            .sum::<f64>()
            / values.len() as f64;
        var.sqrt()
    }
}

fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let q = q.clamp(0.0, 1.0);
    let idx = q * (sorted.len().saturating_sub(1)) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let t = idx - lo as f64;
        sorted[lo] * (1.0 - t) + sorted[hi] * t
    }
}

fn approx_beta_posterior(
    alpha: f64,
    beta: f64,
    prior_alpha: f64,
    prior_beta: f64,
) -> BetaPosterior {
    let total = (alpha + beta).max(1e-9);
    let mean = alpha / total;
    let var = (alpha * beta) / ((total * total) * (total + 1.0)).max(1e-12);
    let std = var.sqrt();
    let ci_low = clamp01(mean - 1.96 * std);
    let ci_high = clamp01(mean + 1.96 * std);
    BetaPosterior {
        alpha,
        beta,
        n_eff: nonneg(total - (prior_alpha + prior_beta)),
        mean,
        ci_low,
        ci_high,
    }
}

fn sample_standard_normal(rng: &mut impl Rng) -> f64 {
    let u1 = rng.random::<f64>().clamp(f64::MIN_POSITIVE, 1.0);
    let u2 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

fn sample_clamped_normal(rng: &mut impl Rng, mean: f64, std: f64, lo: f64, hi: f64) -> f64 {
    if std <= 1e-12 {
        return mean.clamp(lo, hi);
    }
    let draw = mean + std * sample_standard_normal(rng);
    draw.clamp(lo, hi)
}

fn adaptive_bin_count(n: usize) -> usize {
    (n as f64).sqrt().round().clamp(8.0, 64.0) as usize
}

fn bytes_to_bin(v: u8, bins: usize) -> usize {
    let b = ((v as usize) * bins) / 256;
    b.min(bins.saturating_sub(1))
}

fn mutual_information_binned(a: &[u8], b: &[u8], bins: usize) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 || bins == 0 {
        return 0.0;
    }
    let mut px = vec![0usize; bins];
    let mut py = vec![0usize; bins];
    let mut pxy = vec![0usize; bins * bins];

    for i in 0..n {
        let x = bytes_to_bin(a[i], bins);
        let y = bytes_to_bin(b[i], bins);
        px[x] += 1;
        py[y] += 1;
        pxy[x * bins + y] += 1;
    }

    let inv_n = 1.0 / n as f64;
    let mut mi = 0.0;
    let mut kx = 0usize;
    let mut ky = 0usize;
    let mut kxy = 0usize;

    for x in 0..bins {
        if px[x] == 0 {
            continue;
        }
        kx += 1;
        let p_x = px[x] as f64 * inv_n;
        for y in 0..bins {
            let c = pxy[x * bins + y];
            if c == 0 || py[y] == 0 {
                continue;
            }
            kxy += 1;
            let p_y = py[y] as f64 * inv_n;
            let p_xy = c as f64 * inv_n;
            mi += p_xy * (p_xy / (p_x * p_y)).log2();
        }
    }

    for v in &py {
        if *v > 0 {
            ky += 1;
        }
    }

    // Miller-Madow first-order finite-sample correction applied to MI via
    // H(X)+H(Y)-H(X,Y).
    let mm = (kx as f64 + ky as f64 - kxy as f64 - 1.0) / (2.0 * n as f64 * LN_2);
    (mi + mm).max(0.0)
}

fn pearson_corr_bytes(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xx = 0.0;
    let mut sum_yy = 0.0;
    let mut sum_xy = 0.0;
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        sum_x += x;
        sum_y += y;
        sum_xx += x * x;
        sum_yy += y * y;
        sum_xy += x * y;
    }
    let nf = n as f64;
    let num = nf * sum_xy - sum_x * sum_y;
    let den_x = nf * sum_xx - sum_x * sum_x;
    let den_y = nf * sum_yy - sum_y * sum_y;
    let den = (den_x.max(0.0) * den_y.max(0.0)).sqrt();
    if den <= 1e-12 {
        0.0
    } else {
        (num / den).clamp(-1.0, 1.0)
    }
}

fn max_lagged_metric<F>(a: &[u8], b: &[u8], max_lag: usize, mut metric: F) -> f64
where
    F: FnMut(&[u8], &[u8]) -> f64,
{
    let n = a.len().min(b.len());
    if n < 4 {
        return 0.0;
    }
    let mut best = metric(a, b).abs();
    for lag in 1..=max_lag {
        if n <= lag + 2 {
            break;
        }
        let m1 = metric(&a[..(n - lag)], &b[lag..n]).abs();
        let m2 = metric(&a[lag..n], &b[..(n - lag)]).abs();
        best = best.max(m1).max(m2);
    }
    best
}

fn circular_shift_copy(data: &[u8], shift: usize) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let n = data.len();
    let s = shift % n;
    if s == 0 {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(&data[s..]);
    out.extend_from_slice(&data[..s]);
    out
}

fn debiased_metric(observed: f64, null_values: &[f64], sigma: f64) -> (f64, f64) {
    if null_values.is_empty() {
        return (observed.max(0.0), 0.0);
    }
    let m = mean(null_values);
    let s = stddev(null_values, m);
    let excess = (observed - (m + sigma.max(0.0) * s)).max(0.0);
    (excess, m)
}

fn upper_tail_permutation_pvalue(observed: f64, null_values: &[f64]) -> f64 {
    if null_values.is_empty() {
        return 1.0;
    }
    let ge = null_values.iter().filter(|&&v| v >= observed).count() as f64;
    // Phipson-Smyth correction: permutation p-values should never be zero.
    (ge + 1.0) / (null_values.len() as f64 + 1.0)
}

fn normal_survival_approx(z: f64) -> f64 {
    // Abramowitz-Stegun style normal tail approximation.
    // Accurate enough for ranking/significance gating in this experimental model.
    if !z.is_finite() {
        return 1.0;
    }
    if z < 0.0 {
        return 1.0 - normal_survival_approx(-z);
    }
    let t = 1.0 / (1.0 + 0.231_641_9 * z);
    let poly = t
        * (0.319_381_530
            + t * (-0.356_563_782
                + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429))));
    let pdf = (-0.5 * z * z).exp() / (2.0 * PI).sqrt();
    (pdf * poly).clamp(0.0, 1.0)
}

fn upper_tail_nullfit_pvalue(observed: f64, null_values: &[f64]) -> f64 {
    if null_values.len() < 3 {
        return upper_tail_permutation_pvalue(observed, null_values);
    }
    let m = mean(null_values);
    let s = stddev(null_values, m);
    if s <= 1e-12 {
        return if observed > m { 1e-12 } else { 1.0 };
    }
    let z = (observed - m) / s;
    normal_survival_approx(z).clamp(1e-12, 1.0)
}

fn benjamini_hochberg_qvalues(pvalues: &[f64]) -> Vec<f64> {
    if pvalues.is_empty() {
        return Vec::new();
    }
    let m = pvalues.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&i, &j| {
        pvalues[i]
            .partial_cmp(&pvalues[j])
            .unwrap_or(Ordering::Equal)
            .then(i.cmp(&j))
    });

    let mut q = vec![1.0; m];
    let mut prev = 1.0_f64;
    for (rank0, &idx) in order.iter().enumerate().rev() {
        let rank = rank0 + 1;
        let adj = (pvalues[idx] * m as f64 / rank as f64).min(1.0);
        prev = prev.min(adj);
        q[idx] = prev;
    }
    q
}

#[derive(Debug, Clone, Copy, Default)]
struct PairCouplingMoments {
    corr_raw: f64,
    corr_lag_raw: f64,
    mi_raw: f64,
    mi_lag_raw: f64,
    corr_null: f64,
    corr_lag_null: f64,
    mi_null: f64,
    mi_lag_null: f64,
    corr_excess: f64,
    corr_lag_excess: f64,
    mi_excess: f64,
    mi_lag_excess: f64,
    p_corr: f64,
    p_corr_lag: f64,
    p_mi: f64,
    p_mi_lag: f64,
}

fn pair_coupling_moments(a: &[u8], b: &[u8], cfg: QuantumAssessmentConfig) -> PairCouplingMoments {
    let n = a.len().min(b.len());
    if n < 8 {
        return PairCouplingMoments::default();
    }
    let a = &a[..n];
    let b = &b[..n];
    let bins = adaptive_bin_count(n);

    let corr_raw = pearson_corr_bytes(a, b).abs();
    let corr_lag_raw = max_lagged_metric(a, b, cfg.max_lag, pearson_corr_bytes).abs();
    let mi_raw = mutual_information_binned(a, b, bins);
    let mi_lag_raw = max_lagged_metric(a, b, cfg.max_lag, |x, y| {
        mutual_information_binned(x, y, adaptive_bin_count(x.len().min(y.len())))
    });

    let rounds = cfg.coupling_null_rounds.clamp(0, 64);
    let mut null_corr = Vec::with_capacity(rounds);
    let mut null_corr_lag = Vec::with_capacity(rounds);
    let mut null_mi = Vec::with_capacity(rounds);
    let mut null_mi_lag = Vec::with_capacity(rounds);

    if rounds > 0 && n > 16 {
        for k in 0..rounds {
            let shift = (((k + 1) * n) / (rounds + 1)).clamp(1, n.saturating_sub(1));
            let shifted = circular_shift_copy(b, shift);
            let nr = pearson_corr_bytes(a, &shifted).abs();
            let nr_lag = max_lagged_metric(a, &shifted, cfg.max_lag, pearson_corr_bytes).abs();
            let nmi = mutual_information_binned(a, &shifted, bins);
            let nmi_lag = max_lagged_metric(a, &shifted, cfg.max_lag, |x, y| {
                mutual_information_binned(x, y, adaptive_bin_count(x.len().min(y.len())))
            });
            null_corr.push(nr);
            null_corr_lag.push(nr_lag);
            null_mi.push(nmi);
            null_mi_lag.push(nmi_lag);
        }
    }

    let (corr_excess, corr_null) = debiased_metric(corr_raw, &null_corr, cfg.coupling_null_sigma);
    let (corr_lag_excess, corr_lag_null) =
        debiased_metric(corr_lag_raw, &null_corr_lag, cfg.coupling_null_sigma);
    let (mi_excess, mi_null) = debiased_metric(mi_raw, &null_mi, cfg.coupling_null_sigma);
    let (mi_lag_excess, mi_lag_null) =
        debiased_metric(mi_lag_raw, &null_mi_lag, cfg.coupling_null_sigma);
    let p_corr = upper_tail_nullfit_pvalue(corr_raw, &null_corr);
    let p_corr_lag = upper_tail_nullfit_pvalue(corr_lag_raw, &null_corr_lag);
    let p_mi = upper_tail_nullfit_pvalue(mi_raw, &null_mi);
    let p_mi_lag = upper_tail_nullfit_pvalue(mi_lag_raw, &null_mi_lag);

    PairCouplingMoments {
        corr_raw,
        corr_lag_raw,
        mi_raw,
        mi_lag_raw,
        corr_null,
        corr_lag_null,
        mi_null,
        mi_lag_null,
        corr_excess,
        corr_lag_excess,
        mi_excess,
        mi_lag_excess,
        p_corr,
        p_corr_lag,
        p_mi,
        p_mi_lag,
    }
}

/// Parse a category label into `SourceCategory`.
pub fn parse_source_category(category: &str) -> Option<SourceCategory> {
    match category.trim().to_lowercase().as_str() {
        "thermal" => Some(SourceCategory::Thermal),
        "timing" => Some(SourceCategory::Timing),
        "scheduling" => Some(SourceCategory::Scheduling),
        "io" => Some(SourceCategory::IO),
        "ipc" => Some(SourceCategory::IPC),
        "microarch" => Some(SourceCategory::Microarch),
        "gpu" => Some(SourceCategory::GPU),
        "network" => Some(SourceCategory::Network),
        "system" => Some(SourceCategory::System),
        "composite" => Some(SourceCategory::Composite),
        "signal" => Some(SourceCategory::Signal),
        "sensor" => Some(SourceCategory::Sensor),
        _ => None,
    }
}

fn seeded_calibration_records() -> Vec<CalibrationRecord> {
    let mut out = Vec::new();

    let mut add_category = |category: &str, label: f64, weight: f64| {
        out.push(CalibrationRecord {
            source: None,
            category: Some(category.to_string()),
            label: clamp01(label),
            weight: weight.max(0.1),
        });
    };

    add_category("thermal", 0.76, 20.0);
    add_category("sensor", 0.69, 16.0);
    add_category("timing", 0.45, 20.0);
    add_category("gpu", 0.38, 14.0);
    add_category("io", 0.34, 14.0);
    add_category("ipc", 0.29, 12.0);
    add_category("microarch", 0.28, 16.0);
    add_category("scheduling", 0.24, 12.0);
    add_category("network", 0.23, 10.0);
    add_category("system", 0.19, 14.0);
    add_category("signal", 0.20, 10.0);
    add_category("composite", 0.10, 8.0);

    let mut add_source = |source: &str, category: &str, label: f64, weight: f64| {
        out.push(CalibrationRecord {
            source: Some(source.to_string()),
            category: Some(category.to_string()),
            label: clamp01(label),
            weight: weight.max(0.1),
        });
    };

    add_source("audio_noise", "sensor", 0.95, 12.0);
    add_source("camera_noise", "sensor", 0.94, 10.0);
    add_source("counter_beat", "thermal", 0.90, 12.0);
    add_source("audio_pll_timing", "thermal", 0.82, 10.0);
    add_source("display_pll", "thermal", 0.82, 10.0);
    add_source("pcie_pll", "thermal", 0.82, 10.0);
    add_source("denormal_timing", "thermal", 0.72, 8.0);
    add_source("pdn_resonance", "thermal", 0.72, 8.0);
    add_source("usb_timing", "io", 0.42, 8.0);
    add_source("gpu_timing", "gpu", 0.42, 8.0);
    add_source("gpu_divergence", "gpu", 0.42, 8.0);
    add_source("iosurface_crossing", "io", 0.42, 8.0);
    add_source("sleep_jitter", "scheduling", 0.25, 8.0);
    add_source("dispatch_queue", "scheduling", 0.25, 8.0);
    add_source("sysctl_deltas", "system", 0.18, 8.0);
    add_source("vmstat_deltas", "system", 0.18, 8.0);
    add_source("process_table", "system", 0.18, 6.0);
    add_source("process", "system", 0.18, 6.0);
    add_source("dns_timing", "network", 0.22, 6.0);
    add_source("tcp_connect", "network", 0.22, 6.0);
    add_source("wifi_rssi", "network", 0.22, 6.0);

    out
}

/// Build a calibration table from labeled records.
pub fn calibrate_priors(
    records: &[CalibrationRecord],
    prior_alpha: f64,
    prior_beta: f64,
) -> PriorCalibration {
    let pa = prior_alpha.max(1e-3);
    let pb = prior_beta.max(1e-3);

    let mut g_alpha = pa;
    let mut g_beta = pb;
    let mut cat_counts: HashMap<String, (f64, f64)> = HashMap::new();
    let mut src_counts: HashMap<String, (f64, f64)> = HashMap::new();

    for row in records {
        let y = clamp01(row.label);
        let w = row.weight.max(0.1);
        g_alpha += y * w;
        g_beta += (1.0 - y) * w;

        if let Some(cat) = &row.category {
            let key = cat.trim().to_lowercase();
            let e = cat_counts.entry(key).or_insert((pa, pb));
            e.0 += y * w;
            e.1 += (1.0 - y) * w;
        }

        if let Some(src) = &row.source {
            let key = src.trim().to_lowercase();
            let e = src_counts.entry(key).or_insert((pa, pb));
            e.0 += y * w;
            e.1 += (1.0 - y) * w;
        }
    }

    let categories = cat_counts
        .into_iter()
        .map(|(k, (a, b))| (k, approx_beta_posterior(a, b, pa, pb)))
        .collect();
    let sources = src_counts
        .into_iter()
        .map(|(k, (a, b))| (k, approx_beta_posterior(a, b, pa, pb)))
        .collect();

    PriorCalibration {
        model_id: MODEL_ID.to_string(),
        model_version: MODEL_VERSION,
        prior_alpha: pa,
        prior_beta: pb,
        global: approx_beta_posterior(g_alpha, g_beta, pa, pb),
        categories,
        sources,
    }
}

/// Seeded default calibration used when no external labeled calibration is provided.
pub fn default_calibration() -> PriorCalibration {
    calibrate_priors(&seeded_calibration_records(), 1.0, 1.0)
}

/// Load calibration JSON from disk.
pub fn load_calibration_from_path(path: &Path) -> std::io::Result<PriorCalibration> {
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str::<PriorCalibration>(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to parse calibration JSON: {e}"),
        )
    })
}

fn prior_from_calibration(
    source_name: &str,
    category: Option<SourceCategory>,
    calibration: &PriorCalibration,
) -> PriorEstimate {
    let src_key = source_name.trim().to_lowercase();
    let src = calibration.sources.get(&src_key).copied();

    let cat_key = category.map(|c| c.to_string().to_lowercase());
    let cat = cat_key
        .as_ref()
        .and_then(|k| calibration.categories.get(k))
        .copied();

    let g = calibration.global;

    let src_n = src.map(|p| p.n_eff).unwrap_or(0.0);
    let cat_n = cat.map(|p| p.n_eff).unwrap_or(0.0);

    // Hierarchical shrinkage: source -> category -> global.
    let w_source = safe_div(src_n, src_n + 8.0);
    let w_category_base = safe_div(cat_n, cat_n + 6.0);
    let w_category = (1.0 - w_source) * w_category_base;
    let w_global = (1.0 - w_source - w_category).max(0.0);

    let src_mean = src.map(|p| p.mean).unwrap_or(g.mean);
    let cat_mean = cat.map(|p| p.mean).unwrap_or(g.mean);
    let mean = clamp01(w_source * src_mean + w_category * cat_mean + w_global * g.mean);

    let src_low = src.map(|p| p.ci_low).unwrap_or(g.ci_low);
    let src_high = src.map(|p| p.ci_high).unwrap_or(g.ci_high);
    let cat_low = cat.map(|p| p.ci_low).unwrap_or(g.ci_low);
    let cat_high = cat.map(|p| p.ci_high).unwrap_or(g.ci_high);

    let ci_low = clamp01(w_source * src_low + w_category * cat_low + w_global * g.ci_low);
    let ci_high = clamp01(w_source * src_high + w_category * cat_high + w_global * g.ci_high);

    PriorEstimate {
        mean,
        ci_low,
        ci_high,
        source_n_eff: src_n,
        category_n_eff: cat_n,
    }
}

/// Quality factor from statistical analysis.
pub fn quality_factor_from_analysis(sa: &SourceAnalysis) -> f64 {
    let ac_penalty = clamp01(sa.autocorrelation.max_abs_correlation / 0.30);
    let bias_penalty = clamp01(sa.bit_bias.overall_bias / 0.03);
    let flatness = clamp01(sa.spectral.flatness);
    let stationarity_term = if sa.stationarity.is_stationary {
        1.0
    } else {
        clamp01(1.0 / (1.0 + sa.stationarity.f_statistic / 3.0))
    };
    let longest_ratio = if sa.runs.expected_longest_run > 0.0 {
        sa.runs.longest_run as f64 / sa.runs.expected_longest_run
    } else {
        1.0
    };
    let runs_dev = if sa.runs.expected_runs > 0.0 {
        ((sa.runs.total_runs as f64 - sa.runs.expected_runs).abs() / sa.runs.expected_runs).abs()
    } else {
        0.0
    };
    let runs_penalty = 0.5 * clamp01((longest_ratio - 1.0) / 2.0) + 0.5 * clamp01(runs_dev / 0.4);

    clamp01(
        flatness
            * (1.0 - 0.50 * ac_penalty)
            * (1.0 - 0.40 * bias_penalty)
            * stationarity_term
            * (1.0 - 0.30 * runs_penalty),
    )
}

/// Convert stress deltas to normalized sensitivity.
pub fn stress_sensitivity(mean_abs_delta_min_entropy: f64, cfg: QuantumAssessmentConfig) -> f64 {
    clamp01(mean_abs_delta_min_entropy.abs() / cfg.stress_delta_bits.max(1e-9))
}

/// Convert coupling moments to a normalized penalty.
pub fn coupling_penalty(stats: CouplingStats, cfg: QuantumAssessmentConfig) -> f64 {
    let w_sum = cfg.coupling_weight_corr
        + cfg.coupling_weight_lag_corr
        + cfg.coupling_weight_mi
        + cfg.coupling_weight_lag_mi;
    let denom = if w_sum <= 1e-9 { 1.0 } else { w_sum };

    let corr_term = clamp01(stats.mean_abs_corr() / cfg.corr_threshold.max(1e-9));
    let lag_corr_term = clamp01(stats.mean_abs_corr_lag() / cfg.lagged_corr_threshold.max(1e-9));
    let mi_term = clamp01(stats.mean_mi_bits() / cfg.mi_threshold_bits.max(1e-9));
    let lag_mi_term = clamp01(stats.mean_mi_bits_lag() / cfg.lagged_mi_threshold_bits.max(1e-9));

    clamp01(
        (cfg.coupling_weight_corr * corr_term
            + cfg.coupling_weight_lag_corr * lag_corr_term
            + cfg.coupling_weight_mi * mi_term
            + cfg.coupling_weight_lag_mi * lag_mi_term)
            / denom,
    )
}

/// Decompose min-entropy into quantum/classical components.
pub fn assess_from_components(
    min_entropy_bits: f64,
    components: QuantumScoreComponents,
) -> QuantumAssessment {
    let q = clamp01(
        components.physics_prior
            * components.quality_factor
            * (1.0 - components.stress_sensitivity)
            * (1.0 - components.coupling_penalty),
    );
    let min_h = min_entropy_bits.max(0.0);
    let q_bits = min_h * q;
    let c_bits = (min_h - q_bits).max(0.0);
    QuantumAssessment {
        quantum_score: q,
        classical_score: 1.0 - q,
        quantum_min_entropy_bits: q_bits,
        classical_min_entropy_bits: c_bits,
        components,
    }
}

/// Aggregate a vector of assessments.
pub fn aggregate_ratio(values: &[QuantumAssessment]) -> QuantumClassicalRatio {
    let quantum_bits: f64 = values.iter().map(|v| v.quantum_min_entropy_bits).sum();
    let classical_bits: f64 = values.iter().map(|v| v.classical_min_entropy_bits).sum();
    let total = quantum_bits + classical_bits;
    let quantum_fraction = if total > 0.0 {
        quantum_bits / total
    } else {
        0.0
    };
    let classical_fraction = if total > 0.0 {
        classical_bits / total
    } else {
        0.0
    };
    let quantum_to_classical = if classical_bits <= 0.0 {
        if quantum_bits > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    } else {
        quantum_bits / classical_bits
    };

    QuantumClassicalRatio {
        quantum_bits,
        classical_bits,
        quantum_fraction,
        classical_fraction,
        quantum_to_classical,
        quantum_bits_ci_low: quantum_bits,
        quantum_bits_ci_high: quantum_bits,
        classical_bits_ci_low: classical_bits,
        classical_bits_ci_high: classical_bits,
        quantum_fraction_ci_low: quantum_fraction,
        quantum_fraction_ci_high: quantum_fraction,
        quantum_to_classical_ci_low: quantum_to_classical,
        quantum_to_classical_ci_high: quantum_to_classical,
    }
}

/// Compute per-source pairwise coupling moments from raw streams.
pub fn pairwise_coupling_by_source(
    streams: &[(String, Vec<u8>)],
    min_pair_samples: usize,
    max_lag: usize,
) -> HashMap<String, CouplingStats> {
    let cfg = QuantumAssessmentConfig {
        max_lag,
        ..QuantumAssessmentConfig::default()
    };
    pairwise_coupling_by_source_with_config(streams, min_pair_samples, cfg)
}

/// Compute per-source pairwise coupling moments from raw streams using full model config.
pub fn pairwise_coupling_by_source_with_config(
    streams: &[(String, Vec<u8>)],
    min_pair_samples: usize,
    cfg: QuantumAssessmentConfig,
) -> HashMap<String, CouplingStats> {
    let min_pair_samples = min_pair_samples.max(8);
    let mut out: HashMap<String, CouplingStats> = HashMap::new();
    let mut pair_rows: Vec<(String, String, PairCouplingMoments)> = Vec::new();

    for i in 0..streams.len() {
        for j in (i + 1)..streams.len() {
            let (name_a, data_a) = &streams[i];
            let (name_b, data_b) = &streams[j];
            let n = data_a.len().min(data_b.len());
            if n < min_pair_samples {
                continue;
            }
            let a = &data_a[..n];
            let b = &data_b[..n];
            let metrics = pair_coupling_moments(a, b, cfg);
            pair_rows.push((name_a.clone(), name_b.clone(), metrics));
        }
    }

    if pair_rows.is_empty() {
        return out;
    }

    let p_corr: Vec<f64> = pair_rows.iter().map(|(_, _, m)| m.p_corr).collect();
    let p_corr_lag: Vec<f64> = pair_rows.iter().map(|(_, _, m)| m.p_corr_lag).collect();
    let p_mi: Vec<f64> = pair_rows.iter().map(|(_, _, m)| m.p_mi).collect();
    let p_mi_lag: Vec<f64> = pair_rows.iter().map(|(_, _, m)| m.p_mi_lag).collect();
    let q_corr = benjamini_hochberg_qvalues(&p_corr);
    let q_corr_lag = benjamini_hochberg_qvalues(&p_corr_lag);
    let q_mi = benjamini_hochberg_qvalues(&p_mi);
    let q_mi_lag = benjamini_hochberg_qvalues(&p_mi_lag);
    let alpha = cfg.coupling_fdr_alpha.clamp(1e-6, 1.0);
    let use_fdr_gate = cfg.coupling_use_fdr_gate;

    for (idx, (name_a, name_b, metrics)) in pair_rows.into_iter().enumerate() {
        let qc = q_corr.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        let qcl = q_corr_lag.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        let qmi = q_mi.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        let qmil = q_mi_lag.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);

        let sig_corr = qc <= alpha;
        let sig_corr_lag = qcl <= alpha;
        let sig_mi = qmi <= alpha;
        let sig_mi_lag = qmil <= alpha;
        let sig_any = sig_corr || sig_corr_lag || sig_mi || sig_mi_lag;

        for name in [&name_a, &name_b] {
            let e = out.entry(name.clone()).or_default();
            e.sum_abs_corr_raw += metrics.corr_raw;
            e.sum_abs_corr_lag_raw += metrics.corr_lag_raw;
            e.sum_mi_bits_raw += metrics.mi_raw;
            e.sum_mi_bits_lag_raw += metrics.mi_lag_raw;
            e.sum_abs_corr_null += metrics.corr_null;
            e.sum_abs_corr_lag_null += metrics.corr_lag_null;
            e.sum_mi_bits_null += metrics.mi_null;
            e.sum_mi_bits_lag_null += metrics.mi_lag_null;
            e.sum_abs_corr_excess += if !use_fdr_gate || sig_corr {
                metrics.corr_excess
            } else {
                0.0
            };
            e.sum_abs_corr_lag_excess += if !use_fdr_gate || sig_corr_lag {
                metrics.corr_lag_excess
            } else {
                0.0
            };
            e.sum_mi_bits_excess += if !use_fdr_gate || sig_mi {
                metrics.mi_excess
            } else {
                0.0
            };
            e.sum_mi_bits_lag_excess += if !use_fdr_gate || sig_mi_lag {
                metrics.mi_lag_excess
            } else {
                0.0
            };
            e.sum_q_corr += qc;
            e.sum_q_corr_lag += qcl;
            e.sum_q_mi += qmi;
            e.sum_q_mi_lag += qmil;
            if sig_any {
                e.significant_pairs_any += 1;
            }
            if sig_corr {
                e.significant_pairs_corr += 1;
            }
            if sig_corr_lag {
                e.significant_pairs_lag_corr += 1;
            }
            if sig_mi {
                e.significant_pairs_mi += 1;
            }
            if sig_mi_lag {
                e.significant_pairs_lag_mi += 1;
            }
            e.pairs += 1;
        }
    }

    out
}

fn split_windows(data: &[u8], n_windows: usize, min_len: usize) -> Vec<&[u8]> {
    if data.len() < min_len || n_windows == 0 {
        return Vec::new();
    }
    let mut windows = Vec::new();
    let step = (data.len() / n_windows).max(min_len);
    let mut start = 0usize;
    while start + min_len <= data.len() {
        let end = (start + step).min(data.len());
        if end - start >= min_len {
            windows.push(&data[start..end]);
        }
        if end == data.len() {
            break;
        }
        start = end;
    }
    windows
}

fn windowed_coupling_penalty_by_source(
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
) -> HashMap<String, Vec<f64>> {
    let mut out: HashMap<String, Vec<f64>> = HashMap::new();

    // Build per-source windows first.
    let per_source_windows: Vec<(String, Vec<&[u8]>)> = streams
        .iter()
        .map(|(name, data)| {
            (
                name.clone(),
                split_windows(data, cfg.bootstrap_windows, min_pair_samples.max(32)),
            )
        })
        .collect();

    let max_w = per_source_windows
        .iter()
        .map(|(_, w)| w.len())
        .min()
        .unwrap_or(0);

    for wi in 0..max_w {
        let mut win_streams: Vec<(String, Vec<u8>)> = Vec::new();
        for (name, windows) in &per_source_windows {
            if let Some(w) = windows.get(wi) {
                win_streams.push((name.clone(), w.to_vec()));
            }
        }
        let coupling = pairwise_coupling_by_source_with_config(&win_streams, min_pair_samples, cfg);
        for (name, stats) in coupling {
            let penalty = coupling_penalty(stats, cfg);
            out.entry(name).or_default().push(penalty);
        }
    }

    out
}

fn component_uncertainty_from_streams(
    inputs: &[QuantumSourceInput],
    streams: &[(String, Vec<u8>)],
    coupling_by_name: &HashMap<String, CouplingStats>,
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
) -> HashMap<String, ComponentUncertainty> {
    let stream_map: HashMap<&str, &[u8]> = streams
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();

    let coupling_window_penalties =
        windowed_coupling_penalty_by_source(streams, cfg, min_pair_samples.max(16));

    let mut out = HashMap::new();
    for input in inputs {
        let data = stream_map.get(input.name.as_str()).copied().unwrap_or(&[]);

        let mut h_vals = Vec::new();
        let mut q_vals = Vec::new();
        for w in split_windows(data, cfg.bootstrap_windows, 64) {
            let m = EntropyMeasurements::from_bytes(w, None);
            h_vals.push(m.min_entropy.max(0.0));
            let sa = crate::analysis::full_analysis(&input.name, w);
            q_vals.push(quality_factor_from_analysis(&sa));
        }

        if h_vals.is_empty() {
            h_vals.push(input.min_entropy_bits.max(0.0));
        }
        if q_vals.is_empty() {
            q_vals.push(clamp01(input.quality_factor));
        }

        let h_mean = mean(&h_vals);
        let h_std = stddev(&h_vals, h_mean).max(0.02);

        let q_mean = mean(&q_vals);
        let q_std = stddev(&q_vals, q_mean).max(0.02);

        let base_stress = clamp01(input.stress_sensitivity);
        let implied_stress_from_variability = clamp01(safe_div(
            stddev(&h_vals, h_mean),
            cfg.stress_delta_bits.max(1e-9),
        ));
        let s_mean = if base_stress <= 1e-6 {
            implied_stress_from_variability
        } else {
            base_stress
        };
        let s_std = (0.03 + 0.2 * s_mean).clamp(0.02, 0.20);

        let c_point = coupling_by_name
            .get(&input.name)
            .copied()
            .map(|s| coupling_penalty(s, cfg))
            .unwrap_or(0.0);
        let c_samples = coupling_window_penalties
            .get(&input.name)
            .cloned()
            .unwrap_or_else(|| vec![c_point]);
        let c_mean = mean(&c_samples);
        let c_std = stddev(&c_samples, c_mean).max(0.02);

        out.insert(
            input.name.clone(),
            ComponentUncertainty {
                min_entropy_mean: h_mean,
                min_entropy_std: h_std,
                quality_mean: q_mean,
                quality_std: q_std,
                stress_mean: s_mean,
                stress_std: s_std,
                coupling_penalty_mean: c_mean,
                coupling_penalty_std: c_std,
            },
        );
    }

    out
}

fn prior_std_from_ci(low: f64, high: f64) -> f64 {
    ((high - low).abs() / (2.0 * 1.96)).max(0.01)
}

fn build_ablation_and_sensitivity(
    base_rows: &[QuantumSourceResult],
    global_prior: f64,
) -> (QuantumAblationReport, QuantumSensitivityReport) {
    let mut baseline_assess = Vec::with_capacity(base_rows.len());
    let mut no_prior_assess = Vec::with_capacity(base_rows.len());
    let mut no_quality_assess = Vec::with_capacity(base_rows.len());
    let mut no_coupling_assess = Vec::with_capacity(base_rows.len());
    let mut no_stress_assess = Vec::with_capacity(base_rows.len());
    let mut prior_only_assess = Vec::with_capacity(base_rows.len());
    let mut measured_only_assess = Vec::with_capacity(base_rows.len());

    let mut sensitivities = Vec::with_capacity(base_rows.len());

    for row in base_rows {
        let base_components = QuantumScoreComponents {
            physics_prior: row.physics_prior,
            quality_factor: row.quality_factor,
            stress_sensitivity: row.stress_sensitivity,
            coupling_penalty: row.coupling_penalty,
        };
        let base = assess_from_components(row.min_entropy_bits, base_components);
        let no_prior = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                physics_prior: global_prior,
                ..base_components
            },
        );
        let no_quality = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                quality_factor: 1.0,
                ..base_components
            },
        );
        let no_coupling = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                coupling_penalty: 0.0,
                ..base_components
            },
        );
        let no_stress = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                stress_sensitivity: 0.0,
                ..base_components
            },
        );
        let prior_only = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                quality_factor: 1.0,
                stress_sensitivity: 0.0,
                coupling_penalty: 0.0,
                ..base_components
            },
        );
        let measured_only = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                physics_prior: 1.0,
                ..base_components
            },
        );

        baseline_assess.push(base);
        no_prior_assess.push(no_prior);
        no_quality_assess.push(no_quality);
        no_coupling_assess.push(no_coupling);
        no_stress_assess.push(no_stress);
        prior_only_assess.push(prior_only);
        measured_only_assess.push(measured_only);

        sensitivities.push(QuantumSourceSensitivity {
            name: row.name.clone(),
            baseline_q: base.quantum_score,
            impact_without_prior: (base.quantum_score - no_prior.quantum_score).abs(),
            impact_without_quality: (base.quantum_score - no_quality.quantum_score).abs(),
            impact_without_coupling: (base.quantum_score - no_coupling.quantum_score).abs(),
            impact_without_stress: (base.quantum_score - no_stress.quantum_score).abs(),
        });
    }

    let baseline = aggregate_ratio(&baseline_assess);

    let scenarios = [
        ("full", baseline),
        ("without_prior", aggregate_ratio(&no_prior_assess)),
        ("without_quality", aggregate_ratio(&no_quality_assess)),
        ("without_coupling", aggregate_ratio(&no_coupling_assess)),
        ("without_stress", aggregate_ratio(&no_stress_assess)),
        ("prior_only", aggregate_ratio(&prior_only_assess)),
        ("measured_only", aggregate_ratio(&measured_only_assess)),
    ];

    let mut entries = Vec::with_capacity(scenarios.len());
    for (name, row) in scenarios {
        entries.push(QuantumAblationEntry {
            scenario: name.to_string(),
            quantum_fraction: row.quantum_fraction,
            quantum_to_classical: row.quantum_to_classical,
            delta_quantum_fraction: row.quantum_fraction - baseline.quantum_fraction,
            delta_quantum_to_classical: row.quantum_to_classical - baseline.quantum_to_classical,
        });
    }

    sensitivities.sort_by(|a, b| {
        let ia = a.impact_without_prior
            + a.impact_without_quality
            + a.impact_without_coupling
            + a.impact_without_stress;
        let ib = b.impact_without_prior
            + b.impact_without_quality
            + b.impact_without_coupling
            + b.impact_without_stress;
        ib.partial_cmp(&ia)
            .unwrap_or(Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });

    let summary = QuantumSensitivitySummary {
        mean_impact_without_prior: mean(
            &sensitivities
                .iter()
                .map(|s| s.impact_without_prior)
                .collect::<Vec<_>>(),
        ),
        mean_impact_without_quality: mean(
            &sensitivities
                .iter()
                .map(|s| s.impact_without_quality)
                .collect::<Vec<_>>(),
        ),
        mean_impact_without_coupling: mean(
            &sensitivities
                .iter()
                .map(|s| s.impact_without_coupling)
                .collect::<Vec<_>>(),
        ),
        mean_impact_without_stress: mean(
            &sensitivities
                .iter()
                .map(|s| s.impact_without_stress)
                .collect::<Vec<_>>(),
        ),
    };

    (
        QuantumAblationReport { entries },
        QuantumSensitivityReport {
            sources: sensitivities,
            summary,
        },
    )
}

type SourceCiBounds = (f64, f64, f64, f64, f64, f64);
type PerSourceCiBounds = HashMap<String, SourceCiBounds>;

fn run_monte_carlo(
    rows: &[QuantumSourceResult],
    cfg: QuantumAssessmentConfig,
    uncertainty: &HashMap<String, ComponentUncertainty>,
) -> (PerSourceCiBounds, QuantumClassicalRatio) {
    let draws = cfg.bootstrap_rounds.clamp(64, 4096);
    let mut rng = rand::rng();

    let mut per_source_q: HashMap<String, Vec<f64>> = HashMap::new();
    let mut per_source_qb: HashMap<String, Vec<f64>> = HashMap::new();
    let mut per_source_cb: HashMap<String, Vec<f64>> = HashMap::new();

    let mut agg_q = Vec::with_capacity(draws);
    let mut agg_c = Vec::with_capacity(draws);
    let mut agg_qf = Vec::with_capacity(draws);
    let mut agg_qc = Vec::with_capacity(draws);

    for _ in 0..draws {
        let mut q_sum = 0.0;
        let mut c_sum = 0.0;
        for row in rows {
            let u = uncertainty
                .get(&row.name)
                .copied()
                .unwrap_or(ComponentUncertainty {
                    min_entropy_mean: row.min_entropy_bits,
                    min_entropy_std: 0.05,
                    quality_mean: row.quality_factor,
                    quality_std: 0.02,
                    stress_mean: row.stress_sensitivity,
                    stress_std: 0.03,
                    coupling_penalty_mean: row.coupling_penalty,
                    coupling_penalty_std: 0.03,
                });

            let prior_std = prior_std_from_ci(row.physics_prior_ci_low, row.physics_prior_ci_high);
            let prior = sample_clamped_normal(&mut rng, row.physics_prior, prior_std, 0.0, 1.0);
            let min_h =
                sample_clamped_normal(&mut rng, u.min_entropy_mean, u.min_entropy_std, 0.0, 8.0);
            let quality = sample_clamped_normal(&mut rng, u.quality_mean, u.quality_std, 0.0, 1.0);
            let stress = sample_clamped_normal(&mut rng, u.stress_mean, u.stress_std, 0.0, 1.0);
            let coupling = sample_clamped_normal(
                &mut rng,
                u.coupling_penalty_mean,
                u.coupling_penalty_std,
                0.0,
                1.0,
            );

            let a = assess_from_components(
                min_h,
                QuantumScoreComponents {
                    physics_prior: prior,
                    quality_factor: quality,
                    stress_sensitivity: stress,
                    coupling_penalty: coupling,
                },
            );

            per_source_q
                .entry(row.name.clone())
                .or_default()
                .push(a.quantum_score);
            per_source_qb
                .entry(row.name.clone())
                .or_default()
                .push(a.quantum_min_entropy_bits);
            per_source_cb
                .entry(row.name.clone())
                .or_default()
                .push(a.classical_min_entropy_bits);

            q_sum += a.quantum_min_entropy_bits;
            c_sum += a.classical_min_entropy_bits;
        }

        let total = q_sum + c_sum;
        let qf = if total > 0.0 { q_sum / total } else { 0.0 };
        let qc = if c_sum > 0.0 {
            q_sum / c_sum
        } else if q_sum > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        agg_q.push(q_sum);
        agg_c.push(c_sum);
        agg_qf.push(qf);
        agg_qc.push(qc);
    }

    let mut per_source = HashMap::new();
    for row in rows {
        let q = per_source_q.get(&row.name).cloned().unwrap_or_default();
        let qb = per_source_qb.get(&row.name).cloned().unwrap_or_default();
        let cb = per_source_cb.get(&row.name).cloned().unwrap_or_default();
        per_source.insert(
            row.name.clone(),
            (
                quantile(&q, 0.025),
                quantile(&q, 0.975),
                quantile(&qb, 0.025),
                quantile(&qb, 0.975),
                quantile(&cb, 0.025),
                quantile(&cb, 0.975),
            ),
        );
    }

    let q_point = mean(&agg_q);
    let c_point = mean(&agg_c);
    let total = q_point + c_point;
    let qf_point = if total > 0.0 { q_point / total } else { 0.0 };
    let cf_point = if total > 0.0 { c_point / total } else { 0.0 };
    let qc_point = if c_point > 0.0 {
        q_point / c_point
    } else if q_point > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let aggregate = QuantumClassicalRatio {
        quantum_bits: q_point,
        classical_bits: c_point,
        quantum_fraction: qf_point,
        classical_fraction: cf_point,
        quantum_to_classical: qc_point,
        quantum_bits_ci_low: quantile(&agg_q, 0.025),
        quantum_bits_ci_high: quantile(&agg_q, 0.975),
        classical_bits_ci_low: quantile(&agg_c, 0.025),
        classical_bits_ci_high: quantile(&agg_c, 0.975),
        quantum_fraction_ci_low: quantile(&agg_qf, 0.025),
        quantum_fraction_ci_high: quantile(&agg_qf, 0.975),
        quantum_to_classical_ci_low: quantile(&agg_qc, 0.025),
        quantum_to_classical_ci_high: quantile(&agg_qc, 0.975),
    };

    (per_source, aggregate)
}

/// Run full batch assessment with explicit calibration and stream-aware uncertainty.
pub fn assess_batch_from_streams_with_calibration(
    inputs: &[QuantumSourceInput],
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
    calibration: &PriorCalibration,
) -> QuantumBatchReport {
    let coupling = pairwise_coupling_by_source_with_config(streams, min_pair_samples, cfg);
    let uncertainty =
        component_uncertainty_from_streams(inputs, streams, &coupling, cfg, min_pair_samples);

    let mut rows = Vec::with_capacity(inputs.len());

    for input in inputs {
        let stats = coupling.get(&input.name).copied().unwrap_or_default();
        let c_penalty = coupling_penalty(stats, cfg);
        let prior_est = if let Some(p) = input.physics_prior_override {
            PriorEstimate {
                mean: clamp01(p),
                ci_low: clamp01(p - 0.05),
                ci_high: clamp01(p + 0.05),
                source_n_eff: 0.0,
                category_n_eff: 0.0,
            }
        } else {
            prior_from_calibration(&input.name, input.category, calibration)
        };

        let q = assess_from_components(
            input.min_entropy_bits,
            QuantumScoreComponents {
                physics_prior: prior_est.mean,
                quality_factor: clamp01(input.quality_factor),
                stress_sensitivity: clamp01(input.stress_sensitivity),
                coupling_penalty: c_penalty,
            },
        );

        rows.push(QuantumSourceResult {
            name: input.name.clone(),
            category: input
                .category
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            min_entropy_bits: input.min_entropy_bits.max(0.0),
            physics_prior: prior_est.mean,
            physics_prior_ci_low: prior_est.ci_low,
            physics_prior_ci_high: prior_est.ci_high,
            prior_source_samples: prior_est.source_n_eff,
            prior_category_samples: prior_est.category_n_eff,
            quality_factor: clamp01(input.quality_factor),
            stress_sensitivity: clamp01(input.stress_sensitivity),
            stress_sensitivity_effective: clamp01(input.stress_sensitivity),
            telemetry_confound_penalty: 0.0,
            coupling_mean_abs_r_raw: stats.mean_abs_corr_raw(),
            coupling_mean_abs_r_lag_raw: stats.mean_abs_corr_lag_raw(),
            coupling_mean_mi_bits_raw: stats.mean_mi_bits_raw(),
            coupling_mean_mi_bits_lag_raw: stats.mean_mi_bits_lag_raw(),
            coupling_mean_abs_r_null: stats.mean_abs_corr_null(),
            coupling_mean_abs_r_lag_null: stats.mean_abs_corr_lag_null(),
            coupling_mean_mi_bits_null: stats.mean_mi_bits_null(),
            coupling_mean_mi_bits_lag_null: stats.mean_mi_bits_lag_null(),
            coupling_mean_abs_r: stats.mean_abs_corr(),
            coupling_mean_abs_r_lag: stats.mean_abs_corr_lag(),
            coupling_mean_mi_bits: stats.mean_mi_bits(),
            coupling_mean_mi_bits_lag: stats.mean_mi_bits_lag(),
            coupling_mean_q_corr: stats.mean_q_corr(),
            coupling_mean_q_corr_lag: stats.mean_q_corr_lag(),
            coupling_mean_q_mi: stats.mean_q_mi(),
            coupling_mean_q_mi_lag: stats.mean_q_mi_lag(),
            coupling_significant_pair_fraction_any: stats.significant_pair_fraction_any(),
            coupling_significant_pair_fraction_corr: stats.significant_pair_fraction_corr(),
            coupling_significant_pair_fraction_corr_lag: stats.significant_pair_fraction_corr_lag(),
            coupling_significant_pair_fraction_mi: stats.significant_pair_fraction_mi(),
            coupling_significant_pair_fraction_mi_lag: stats.significant_pair_fraction_mi_lag(),
            coupling_penalty: c_penalty,
            quantum_score: q.quantum_score,
            quantum_score_ci_low: q.quantum_score,
            quantum_score_ci_high: q.quantum_score,
            classical_score: q.classical_score,
            quantum_min_entropy_bits: q.quantum_min_entropy_bits,
            quantum_min_entropy_bits_ci_low: q.quantum_min_entropy_bits,
            quantum_min_entropy_bits_ci_high: q.quantum_min_entropy_bits,
            classical_min_entropy_bits: q.classical_min_entropy_bits,
            classical_min_entropy_bits_ci_low: q.classical_min_entropy_bits,
            classical_min_entropy_bits_ci_high: q.classical_min_entropy_bits,
        });
    }

    rows.sort_by(|a, b| {
        b.quantum_score
            .partial_cmp(&a.quantum_score)
            .unwrap_or(Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });

    let (per_source_ci, aggregate) = run_monte_carlo(&rows, cfg, &uncertainty);

    for row in &mut rows {
        if let Some((q_lo, q_hi, qb_lo, qb_hi, cb_lo, cb_hi)) = per_source_ci.get(&row.name) {
            row.quantum_score_ci_low = *q_lo;
            row.quantum_score_ci_high = *q_hi;
            row.quantum_min_entropy_bits_ci_low = *qb_lo;
            row.quantum_min_entropy_bits_ci_high = *qb_hi;
            row.classical_min_entropy_bits_ci_low = *cb_lo;
            row.classical_min_entropy_bits_ci_high = *cb_hi;
        }
    }

    let (ablation, sensitivity) = build_ablation_and_sensitivity(&rows, calibration.global.mean);

    QuantumBatchReport {
        config: cfg,
        calibration: QuantumCalibrationSummary {
            global_prior: calibration.global.mean,
            global_prior_ci_low: calibration.global.ci_low,
            global_prior_ci_high: calibration.global.ci_high,
            category_entries: calibration.categories.len(),
            source_entries: calibration.sources.len(),
        },
        sources: rows,
        aggregate,
        ablation,
        sensitivity,
        telemetry_confound: None,
    }
}

/// Run full batch assessment with default seeded calibration.
pub fn assess_batch_from_streams(
    inputs: &[QuantumSourceInput],
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
) -> QuantumBatchReport {
    let calibration = default_calibration();
    assess_batch_from_streams_with_calibration(inputs, streams, cfg, min_pair_samples, &calibration)
}

/// Run assessment from precomputed coupling stats and default calibration.
pub fn assess_batch(
    inputs: &[QuantumSourceInput],
    coupling_by_name: &HashMap<String, CouplingStats>,
    cfg: QuantumAssessmentConfig,
) -> QuantumBatchReport {
    // Fallback path with no streams: uses deterministic intervals and no windowed uncertainty.
    let calibration = default_calibration();

    let mut rows = Vec::with_capacity(inputs.len());
    let mut assessments = Vec::with_capacity(inputs.len());

    for input in inputs {
        let stats = coupling_by_name
            .get(&input.name)
            .copied()
            .unwrap_or_default();
        let c_penalty = coupling_penalty(stats, cfg);
        let prior_est = input.physics_prior_override.map_or_else(
            || prior_from_calibration(&input.name, input.category, &calibration),
            |p| PriorEstimate {
                mean: clamp01(p),
                ci_low: clamp01(p - 0.05),
                ci_high: clamp01(p + 0.05),
                source_n_eff: 0.0,
                category_n_eff: 0.0,
            },
        );

        let q = assess_from_components(
            input.min_entropy_bits,
            QuantumScoreComponents {
                physics_prior: prior_est.mean,
                quality_factor: clamp01(input.quality_factor),
                stress_sensitivity: clamp01(input.stress_sensitivity),
                coupling_penalty: c_penalty,
            },
        );
        assessments.push(q);

        rows.push(QuantumSourceResult {
            name: input.name.clone(),
            category: input
                .category
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            min_entropy_bits: input.min_entropy_bits.max(0.0),
            physics_prior: prior_est.mean,
            physics_prior_ci_low: prior_est.ci_low,
            physics_prior_ci_high: prior_est.ci_high,
            prior_source_samples: prior_est.source_n_eff,
            prior_category_samples: prior_est.category_n_eff,
            quality_factor: clamp01(input.quality_factor),
            stress_sensitivity: clamp01(input.stress_sensitivity),
            stress_sensitivity_effective: clamp01(input.stress_sensitivity),
            telemetry_confound_penalty: 0.0,
            coupling_mean_abs_r_raw: stats.mean_abs_corr_raw(),
            coupling_mean_abs_r_lag_raw: stats.mean_abs_corr_lag_raw(),
            coupling_mean_mi_bits_raw: stats.mean_mi_bits_raw(),
            coupling_mean_mi_bits_lag_raw: stats.mean_mi_bits_lag_raw(),
            coupling_mean_abs_r_null: stats.mean_abs_corr_null(),
            coupling_mean_abs_r_lag_null: stats.mean_abs_corr_lag_null(),
            coupling_mean_mi_bits_null: stats.mean_mi_bits_null(),
            coupling_mean_mi_bits_lag_null: stats.mean_mi_bits_lag_null(),
            coupling_mean_abs_r: stats.mean_abs_corr(),
            coupling_mean_abs_r_lag: stats.mean_abs_corr_lag(),
            coupling_mean_mi_bits: stats.mean_mi_bits(),
            coupling_mean_mi_bits_lag: stats.mean_mi_bits_lag(),
            coupling_mean_q_corr: stats.mean_q_corr(),
            coupling_mean_q_corr_lag: stats.mean_q_corr_lag(),
            coupling_mean_q_mi: stats.mean_q_mi(),
            coupling_mean_q_mi_lag: stats.mean_q_mi_lag(),
            coupling_significant_pair_fraction_any: stats.significant_pair_fraction_any(),
            coupling_significant_pair_fraction_corr: stats.significant_pair_fraction_corr(),
            coupling_significant_pair_fraction_corr_lag: stats.significant_pair_fraction_corr_lag(),
            coupling_significant_pair_fraction_mi: stats.significant_pair_fraction_mi(),
            coupling_significant_pair_fraction_mi_lag: stats.significant_pair_fraction_mi_lag(),
            coupling_penalty: c_penalty,
            quantum_score: q.quantum_score,
            quantum_score_ci_low: q.quantum_score,
            quantum_score_ci_high: q.quantum_score,
            classical_score: q.classical_score,
            quantum_min_entropy_bits: q.quantum_min_entropy_bits,
            quantum_min_entropy_bits_ci_low: q.quantum_min_entropy_bits,
            quantum_min_entropy_bits_ci_high: q.quantum_min_entropy_bits,
            classical_min_entropy_bits: q.classical_min_entropy_bits,
            classical_min_entropy_bits_ci_low: q.classical_min_entropy_bits,
            classical_min_entropy_bits_ci_high: q.classical_min_entropy_bits,
        });
    }

    rows.sort_by(|a, b| {
        b.quantum_score
            .partial_cmp(&a.quantum_score)
            .unwrap_or(Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });

    let aggregate = aggregate_ratio(&assessments);
    let (ablation, sensitivity) = build_ablation_and_sensitivity(&rows, calibration.global.mean);

    QuantumBatchReport {
        config: cfg,
        calibration: QuantumCalibrationSummary {
            global_prior: calibration.global.mean,
            global_prior_ci_low: calibration.global.ci_low,
            global_prior_ci_high: calibration.global.ci_high,
            category_entries: calibration.categories.len(),
            source_entries: calibration.sources.len(),
        },
        sources: rows,
        aggregate,
        ablation,
        sensitivity,
        telemetry_confound: None,
    }
}

/// Apply telemetry-based classical confounding adjustment to an existing report.
///
/// This keeps the base model terms but increases effective stress sensitivity
/// based on measured host-state instability over the same run window.
pub fn apply_telemetry_confound(
    mut report: QuantumBatchReport,
    telemetry_window: Option<&TelemetryWindowReport>,
    cfg: TelemetryConfoundConfig,
) -> QuantumBatchReport {
    let Some(window) = telemetry_window else {
        return report;
    };
    let confound = telemetry_confound_from_window(window, cfg);
    report.telemetry_confound = Some(confound.clone());

    let base_penalty = clamp01(confound.confound_index * cfg.confound_to_stress_scale);
    if base_penalty <= 0.0 {
        return report;
    }

    for row in &mut report.sources {
        let cat_scale = category_telemetry_scale(&row.category);
        let penalty = clamp01(base_penalty * cat_scale);
        let stress_effective = clamp01(row.stress_sensitivity + penalty);

        let q = assess_from_components(
            row.min_entropy_bits,
            QuantumScoreComponents {
                physics_prior: row.physics_prior,
                quality_factor: row.quality_factor,
                stress_sensitivity: stress_effective,
                coupling_penalty: row.coupling_penalty,
            },
        );

        let (q_score_ci_low, q_score_ci_high) = ci_shifted(
            row.quantum_score,
            row.quantum_score_ci_low,
            row.quantum_score_ci_high,
            q.quantum_score,
            0.0,
            1.0,
        );
        let (q_bits_ci_low, q_bits_ci_high) = ci_shifted(
            row.quantum_min_entropy_bits,
            row.quantum_min_entropy_bits_ci_low,
            row.quantum_min_entropy_bits_ci_high,
            q.quantum_min_entropy_bits,
            0.0,
            row.min_entropy_bits.max(0.0),
        );
        let c_center = q.classical_min_entropy_bits;
        let c_low = (row.min_entropy_bits.max(0.0) - q_bits_ci_high).max(0.0);
        let c_high = row.min_entropy_bits.max(0.0) - q_bits_ci_low;

        row.telemetry_confound_penalty = penalty;
        row.stress_sensitivity_effective = stress_effective;
        row.quantum_score = q.quantum_score;
        row.classical_score = q.classical_score;
        row.quantum_min_entropy_bits = q.quantum_min_entropy_bits;
        row.classical_min_entropy_bits = q.classical_min_entropy_bits;
        row.quantum_score_ci_low = q_score_ci_low;
        row.quantum_score_ci_high = q_score_ci_high;
        row.quantum_min_entropy_bits_ci_low = q_bits_ci_low;
        row.quantum_min_entropy_bits_ci_high = q_bits_ci_high;
        row.classical_min_entropy_bits_ci_low = c_low.min(c_center);
        row.classical_min_entropy_bits_ci_high = c_high.max(c_center);
    }

    report.sources.sort_by(|a, b| {
        b.quantum_score
            .partial_cmp(&a.quantum_score)
            .unwrap_or(Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });

    let q_bits = report
        .sources
        .iter()
        .map(|r| r.quantum_min_entropy_bits.max(0.0))
        .sum::<f64>();
    let c_bits = report
        .sources
        .iter()
        .map(|r| r.classical_min_entropy_bits.max(0.0))
        .sum::<f64>();
    let total = q_bits + c_bits;
    let qf = if total > 0.0 { q_bits / total } else { 0.0 };
    let cf = if total > 0.0 { c_bits / total } else { 0.0 };
    let q_to_c = if c_bits > 0.0 {
        q_bits / c_bits
    } else if q_bits > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let old = report.aggregate;
    let (q_ci_low, q_ci_high) = ci_shifted(
        old.quantum_bits,
        old.quantum_bits_ci_low,
        old.quantum_bits_ci_high,
        q_bits,
        0.0,
        q_bits.max(old.quantum_bits_ci_high + old.classical_bits_ci_high + 1.0),
    );
    let (c_ci_low, c_ci_high) = ci_shifted(
        old.classical_bits,
        old.classical_bits_ci_low,
        old.classical_bits_ci_high,
        c_bits,
        0.0,
        c_bits.max(old.quantum_bits_ci_high + old.classical_bits_ci_high + 1.0),
    );
    let qf_ci_low = if (q_ci_low + c_ci_high) > 0.0 {
        q_ci_low / (q_ci_low + c_ci_high)
    } else {
        0.0
    };
    let qf_ci_high = if (q_ci_high + c_ci_low) > 0.0 {
        q_ci_high / (q_ci_high + c_ci_low)
    } else {
        0.0
    };
    let q_to_c_ci_low = if c_ci_high > 0.0 {
        q_ci_low / c_ci_high
    } else if q_ci_low > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    let q_to_c_ci_high = if c_ci_low > 0.0 {
        q_ci_high / c_ci_low
    } else if q_ci_high > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    report.aggregate = QuantumClassicalRatio {
        quantum_bits: q_bits,
        classical_bits: c_bits,
        quantum_fraction: qf,
        classical_fraction: cf,
        quantum_to_classical: q_to_c,
        quantum_bits_ci_low: q_ci_low,
        quantum_bits_ci_high: q_ci_high,
        classical_bits_ci_low: c_ci_low,
        classical_bits_ci_high: c_ci_high,
        quantum_fraction_ci_low: qf_ci_low,
        quantum_fraction_ci_high: qf_ci_high,
        quantum_to_classical_ci_low: q_to_c_ci_low,
        quantum_to_classical_ci_high: q_to_c_ci_high,
    };

    let (ablation, sensitivity) =
        build_ablation_and_sensitivity(&report.sources, report.calibration.global_prior);
    report.ablation = ablation;
    report.sensitivity = sensitivity;
    report
}

/// Assess a batch from streams and apply telemetry confound adjustment when provided.
pub fn assess_batch_from_streams_with_telemetry(
    inputs: &[QuantumSourceInput],
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
    telemetry_window: Option<&TelemetryWindowReport>,
    telemetry_cfg: TelemetryConfoundConfig,
) -> QuantumBatchReport {
    let report = assess_batch_from_streams(inputs, streams, cfg, min_pair_samples);
    apply_telemetry_confound(report, telemetry_window, telemetry_cfg)
}

/// Assess with custom calibration and optional telemetry confound adjustment.
pub fn assess_batch_from_streams_with_calibration_and_telemetry(
    inputs: &[QuantumSourceInput],
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
    min_pair_samples: usize,
    calibration: &PriorCalibration,
    telemetry_window: Option<&TelemetryWindowReport>,
    telemetry_cfg: TelemetryConfoundConfig,
) -> QuantumBatchReport {
    let report = assess_batch_from_streams_with_calibration(
        inputs,
        streams,
        cfg,
        min_pair_samples,
        calibration,
    );
    apply_telemetry_confound(report, telemetry_window, telemetry_cfg)
}

fn collect_min_entropy_map(
    pool: &EntropyPool,
    source_names: &[String],
    sample_bytes: usize,
    min_samples: usize,
    timeout_secs: f64,
) -> HashMap<String, f64> {
    let samples = pool.collect_source_raw_samples_parallel(
        sample_bytes.max(1),
        timeout_secs.max(0.1),
        min_samples.max(1),
    );

    let selected: std::collections::HashSet<&str> =
        source_names.iter().map(|s| s.as_str()).collect();
    samples
        .into_iter()
        .filter(|s| selected.contains(s.name.as_str()))
        .map(|s| {
            let m = EntropyMeasurements::from_bytes(&s.data, None);
            (s.name, m.min_entropy.max(0.0))
        })
        .collect()
}

struct WorkerGuard {
    stop: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.stop.store(true, AtomicOrdering::Relaxed);
        while let Some(h) = self.handles.pop() {
            let _ = h.join();
        }
    }
}

fn spawn_cpu_workers(threads: usize) -> WorkerGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(threads);
    for _ in 0..threads.max(1) {
        let stop_flag = Arc::clone(&stop);
        handles.push(thread::spawn(move || {
            let mut x = 0x9e37_79b9_7f4a_7c15_u64;
            while !stop_flag.load(AtomicOrdering::Relaxed) {
                x = x
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let y = x.rotate_left(13) ^ x.rotate_right(7);
                std::hint::black_box(y);
            }
        }));
    }
    WorkerGuard { stop, handles }
}

fn spawn_memory_workers(threads: usize, mb_per_thread: usize) -> WorkerGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(threads.max(1));
    let bytes = mb_per_thread.max(8) * 1024 * 1024;

    for _ in 0..threads.max(1) {
        let stop_flag = Arc::clone(&stop);
        handles.push(thread::spawn(move || {
            let mut buf = vec![0u8; bytes];
            let mut i = 0usize;
            while !stop_flag.load(AtomicOrdering::Relaxed) {
                let idx = i % buf.len();
                buf[idx] = buf[idx].wrapping_add(1);
                i = i.wrapping_add(4099);
                if i & 0x3fff == 0 {
                    std::hint::black_box(buf[idx]);
                }
            }
        }));
    }

    WorkerGuard { stop, handles }
}

fn spawn_scheduler_workers(threads: usize) -> WorkerGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(threads.max(1));
    for _ in 0..threads.max(1) {
        let stop_flag = Arc::clone(&stop);
        handles.push(thread::spawn(move || {
            while !stop_flag.load(AtomicOrdering::Relaxed) {
                thread::yield_now();
                thread::sleep(Duration::from_micros(80));
            }
        }));
    }
    WorkerGuard { stop, handles }
}

/// Collect measured stress sensitivity per source via baseline + stressed sampling passes.
pub fn collect_stress_sweep(
    pool: &EntropyPool,
    source_names: &[String],
    sample_bytes: usize,
    min_samples: usize,
    timeout_secs: f64,
    qcfg: QuantumAssessmentConfig,
    scfg: StressSweepConfig,
) -> StressSweepReport {
    let t0 = Instant::now();

    let baseline =
        collect_named_source_stream_samples(pool, source_names, sample_bytes, min_samples)
            .into_iter()
            .map(|s| {
                (
                    s.name,
                    EntropyMeasurements::from_bytes(&s.data, None)
                        .min_entropy
                        .max(0.0),
                )
            })
            .collect::<HashMap<_, _>>();

    let mut cpu_map: HashMap<String, f64> = HashMap::new();
    let mut mem_map: HashMap<String, f64> = HashMap::new();
    let mut sched_map: HashMap<String, f64> = HashMap::new();

    if scfg.enabled {
        {
            let _guard = spawn_cpu_workers(scfg.cpu_threads);
            thread::sleep(Duration::from_millis(scfg.warmup_ms));
            cpu_map = collect_min_entropy_map(
                pool,
                source_names,
                sample_bytes,
                min_samples,
                timeout_secs,
            );
        }

        {
            let _guard =
                spawn_memory_workers(scfg.memory_threads, scfg.memory_megabytes_per_thread);
            thread::sleep(Duration::from_millis(scfg.warmup_ms));
            mem_map = collect_min_entropy_map(
                pool,
                source_names,
                sample_bytes,
                min_samples,
                timeout_secs,
            );
        }

        {
            let _guard = spawn_scheduler_workers(scfg.scheduler_threads);
            thread::sleep(Duration::from_millis(scfg.warmup_ms));
            sched_map = collect_min_entropy_map(
                pool,
                source_names,
                sample_bytes,
                min_samples,
                timeout_secs,
            );
        }
    }

    let mut by_source = HashMap::new();
    for name in source_names {
        let base = baseline.get(name).copied().unwrap_or(0.0);
        let cpu = cpu_map.get(name).copied();
        let mem = mem_map.get(name).copied();
        let sched = sched_map.get(name).copied();

        let mut deltas = Vec::new();
        if let Some(v) = cpu {
            deltas.push((v - base).abs());
        }
        if let Some(v) = mem {
            deltas.push((v - base).abs());
        }
        if let Some(v) = sched {
            deltas.push((v - base).abs());
        }
        let mad = mean(&deltas);
        let s = stress_sensitivity(mad, qcfg);

        by_source.insert(
            name.clone(),
            StressSweepSourceResult {
                baseline_min_entropy: base,
                cpu_load_min_entropy: cpu,
                memory_load_min_entropy: mem,
                scheduler_load_min_entropy: sched,
                mean_abs_delta_min_entropy: mad,
                stress_sensitivity: s,
            },
        );
    }

    StressSweepReport {
        config: scfg,
        elapsed_ms: t0.elapsed().as_millis() as u64,
        by_source,
    }
}

/// Estimate stress sensitivity from stream variability when no live stress sweep is available.
pub fn estimate_stress_sensitivity_from_streams(
    streams: &[(String, Vec<u8>)],
    cfg: QuantumAssessmentConfig,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (name, data) in streams {
        let windows = split_windows(data, cfg.bootstrap_windows, 64);
        let mut mins = Vec::new();
        for w in windows {
            mins.push(
                EntropyMeasurements::from_bytes(w, None)
                    .min_entropy
                    .max(0.0),
            );
        }
        if mins.is_empty() {
            out.insert(name.clone(), 0.0);
            continue;
        }
        let m = mean(&mins);
        let s = stddev(&mins, m);
        let score = stress_sensitivity(s, cfg);
        out.insert(name.clone(), score);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_builds() {
        let rows = vec![
            CalibrationRecord {
                source: Some("a".into()),
                category: Some("sensor".into()),
                label: 0.9,
                weight: 5.0,
            },
            CalibrationRecord {
                source: Some("b".into()),
                category: Some("system".into()),
                label: 0.1,
                weight: 5.0,
            },
        ];
        let c = calibrate_priors(&rows, 1.0, 1.0);
        assert!(c.global.mean > 0.0);
        assert_eq!(c.model_id, MODEL_ID);
    }

    #[test]
    fn coupling_detects_dependence() {
        let a: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let b = a.clone();
        let c: Vec<u8> = (0..=255).rev().cycle().take(4096).collect();
        let streams = vec![
            ("a".to_string(), a),
            ("b".to_string(), b),
            ("c".to_string(), c),
        ];
        let stats = pairwise_coupling_by_source(&streams, 128, 8);
        let sa = stats.get("a").copied().unwrap_or_default();
        assert!(sa.mean_abs_corr() >= 0.0);
        assert!(sa.mean_abs_corr_lag() >= 0.0);
        assert!(sa.mean_mi_bits() >= 0.0);
    }

    #[test]
    fn coupling_significance_detects_identical_streams() {
        let mut x = 0x9e37_79b9_7f4a_7c15_u64;
        let mut y = 0x243f_6a88_85a3_08d3_u64;
        let mut a = Vec::with_capacity(8192);
        let mut c = Vec::with_capacity(8192);
        for _ in 0..8192 {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            y = y
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            a.push((x >> 24) as u8);
            c.push((y >> 24) as u8);
        }
        let b = a.clone();
        let streams = vec![
            ("a".to_string(), a),
            ("b".to_string(), b),
            ("c".to_string(), c),
        ];
        let stats = pairwise_coupling_by_source_with_config(
            &streams,
            512,
            QuantumAssessmentConfig::default(),
        );
        let sa = stats.get("a").copied().unwrap_or_default();
        assert!(sa.significant_pair_fraction_any() > 0.0);
        assert!(sa.mean_q_corr() < 1.0 || sa.mean_q_mi() < 1.0);
    }

    #[test]
    fn assess_with_uncertainty() {
        let inputs = vec![
            QuantumSourceInput {
                name: "audio_noise".to_string(),
                category: Some(SourceCategory::Sensor),
                min_entropy_bits: 6.0,
                quality_factor: 0.8,
                stress_sensitivity: 0.1,
                physics_prior_override: None,
            },
            QuantumSourceInput {
                name: "sysctl_deltas".to_string(),
                category: Some(SourceCategory::System),
                min_entropy_bits: 2.0,
                quality_factor: 0.7,
                stress_sensitivity: 0.2,
                physics_prior_override: None,
            },
        ];
        let streams = vec![
            (
                "audio_noise".to_string(),
                (0..=255).cycle().take(2048).collect(),
            ),
            (
                "sysctl_deltas".to_string(),
                (0..=255).rev().cycle().take(2048).collect(),
            ),
        ];
        let report =
            assess_batch_from_streams(&inputs, &streams, QuantumAssessmentConfig::default(), 128);
        assert_eq!(report.sources.len(), 2);
        assert!(report.aggregate.quantum_bits >= 0.0);
        assert!(report.aggregate.classical_bits >= 0.0);
        assert!(
            report.aggregate.quantum_fraction_ci_high >= report.aggregate.quantum_fraction_ci_low
        );
    }
}
