//! # openentropy-core
//!
//! **Your computer is a hardware noise observatory.**
//!
//! `openentropy-core` is the core entropy harvesting library that extracts randomness
//! from 47 unconventional hardware sources — clock jitter, DRAM row buffer timing,
//! CPU speculative execution, Bluetooth RSSI, NVMe latency, and more.
//!
//! ## Quick Start
//!
//! ```no_run
//! use openentropy_core::EntropyPool;
//!
//! // Auto-detect all available sources and create a pool
//! let pool = EntropyPool::auto();
//!
//! // Get conditioned random bytes
//! let random_bytes = pool.get_random_bytes(256);
//! assert_eq!(random_bytes.len(), 256);
//!
//! // Check pool health
//! let health = pool.health_report();
//! println!("{}/{} sources healthy", health.healthy, health.total);
//! ```
//!
//! ## Architecture
//!
//! Sources → Pool (concatenate) → Conditioning → Output
//!
//! Three output modes:
//! - **Sha256** (default): SHA-256 conditioning mixes all source bytes with state,
//!   counter, timestamp, and OS entropy. Cryptographically strong output.
//! - **VonNeumann**: debiases raw bytes without destroying noise structure.
//! - **Raw** (`get_raw_bytes`): source bytes pass through unchanged — no hashing,
//!   no whitening, no mixing between sources.
//!
//! Raw mode preserves the actual hardware noise signal for researchers studying
//! device entropy characteristics. Most QRNG APIs (ANU, Outshift) run DRBG
//! post-processing that destroys the raw hardware signal. We don't.
//!
//! Every source implements the [`EntropySource`] trait. The [`EntropyPool`]
//! collects from all registered sources and concatenates their byte streams.

pub mod analysis;
pub mod conditioning;
pub mod measurement;
pub mod metrics;
pub mod platform;
pub mod pool;
pub mod quantum;
pub mod session;
pub mod source;
pub mod sources;
pub mod telemetry;

/// Explicit experimental namespace for non-standardized models.
///
/// Prefer these paths in new code:
/// - `openentropy_core::experimental::quantum_proxy_v3`
/// - `openentropy_core::experimental::telemetry_v1`
pub mod experimental {
    pub use crate::metrics::experimental::quantum_proxy_v3;
    pub use crate::metrics::telemetry as telemetry_v1;
}

pub use conditioning::{
    ConditioningMode, MinEntropyReport, QualityReport, condition, grade_min_entropy,
    min_entropy_estimate, quick_min_entropy, quick_quality, quick_shannon,
};
// Experimental re-exports are kept for compatibility. Prefer explicit paths
// under `openentropy_core::experimental` for clearer boundary signaling.
pub use metrics::experimental::quantum_proxy_v3::{
    CalibrationRecord, CouplingStats, MODEL_ID as QUANTUM_PROXY_MODEL_ID,
    MODEL_VERSION as QUANTUM_PROXY_MODEL_VERSION, PriorCalibration, QuantumAblationEntry,
    QuantumAblationReport, QuantumAssessment, QuantumAssessmentConfig, QuantumBatchReport,
    QuantumCalibrationSummary, QuantumClassicalRatio, QuantumScoreComponents,
    QuantumSensitivityReport, QuantumSensitivitySummary, QuantumSourceInput, QuantumSourceResult,
    QuantumSourceSensitivity, StressSweepConfig, StressSweepReport, StressSweepSourceResult,
    TelemetryConfoundConfig, TelemetryConfoundReport, aggregate_ratio, apply_telemetry_confound,
    assess_batch, assess_batch_from_streams, assess_batch_from_streams_with_calibration,
    assess_batch_from_streams_with_calibration_and_telemetry,
    assess_batch_from_streams_with_telemetry, assess_from_components, calibrate_priors,
    collect_stress_sweep, coupling_penalty, default_calibration,
    estimate_stress_sensitivity_from_streams, load_calibration_from_path,
    pairwise_coupling_by_source, pairwise_coupling_by_source_with_config, parse_source_category,
    quality_factor_from_analysis, stress_sensitivity, telemetry_confound_from_window,
};
pub use metrics::standard::{EntropyMeasurements, SourceMeasurementRecord, compression_ratio};
pub use metrics::streams::{
    DEFAULT_SAMPLE_TIMEOUT_SECS, SourceRawStreamSample, collect_named_source_stream_samples,
    collect_source_stream_samples, collect_source_stream_samples_with_timeout,
    to_measurement_records, to_named_streams,
};
pub use metrics::telemetry::{
    MODEL_ID as TELEMETRY_MODEL_ID, MODEL_VERSION as TELEMETRY_MODEL_VERSION, TelemetryMetric,
    TelemetryMetricDelta, TelemetrySnapshot, TelemetryWindowReport, build_telemetry_window,
    collect_telemetry_snapshot, collect_telemetry_window,
};
pub use platform::{detect_available_sources, platform_info};
pub use pool::{EntropyPool, HealthReport, SourceHealth, SourceInfoSnapshot};
pub use session::{
    MachineInfo, SessionConfig, SessionMeta, SessionSourceAnalysis, SessionWriter,
    detect_machine_info,
};
pub use source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

/// Library version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
