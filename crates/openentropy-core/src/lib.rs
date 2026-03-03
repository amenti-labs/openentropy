//! # openentropy-core
//!
//! **Your computer is a hardware noise observatory.**
//!
//! `openentropy-core` is the core entropy harvesting library that extracts randomness
//! from 60+ unconventional hardware sources — clock jitter, DRAM row buffer timing,
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
pub mod benchmark;
pub mod chaos;
pub mod comparison;
pub mod conditioning;
pub mod dispatcher;
pub(crate) mod math;
pub mod platform;
pub mod pool;
pub mod session;
pub mod source;
pub mod sources;
pub mod telemetry;
pub mod trials;
pub mod verdict;

pub use analysis::{
    AutocorrResult, BitBiasResult, CrossCorrMatrix, CrossCorrPair, DistributionResult, RunsResult,
    SourceAnalysis, SpectralResult, StationarityResult, autocorrelation_profile, bit_bias,
    cross_correlation_matrix, distribution_stats, full_analysis, pearson_correlation,
    runs_analysis, spectral_analysis, stationarity_test,
};
pub use benchmark::{
    BenchConfig, BenchReport, BenchSourceReport, PoolQualityReport, RankBy, benchmark_sources,
};
pub use chaos::{
    BiEntropyResult, ChaosAnalysis, CorrelationDimResult, EpiplexityResult, HurstResult,
    LyapunovResult, bientropy, chaos_analysis, correlation_dimension, epiplexity, hurst_exponent,
    lyapunov_exponent,
};
pub use comparison::{
    AggregateDelta, ComparisonResult, DigramAnalysis, MarkovAnalysis, MultiLagAnalysis,
    RunLengthComparison, TemporalAnalysis, TwoSampleTests, WindowAnomaly, aggregate_delta,
    cliffs_delta, compare, compare_with_analysis, digram_analysis, markov_analysis,
    multi_lag_analysis, run_length_comparison, temporal_analysis, two_sample_tests,
};
pub use conditioning::{
    ConditioningMode, MinEntropyReport, QualityReport, condition, grade_min_entropy,
    min_entropy_estimate, quick_autocorrelation_lag1, quick_min_entropy, quick_quality,
    quick_shannon,
};
pub use dispatcher::{
    AnalysisConfig, AnalysisProfile, AnalysisReport, SourceReport, VerdictSummary, analyze,
};
pub use platform::{detect_available_sources, platform_info};
pub use pool::{EntropyPool, HealthReport, SourceHealth, SourceInfoSnapshot};
pub use session::{
    MachineInfo, SessionConfig, SessionMeta, SessionSourceAnalysis, SessionWriter,
    detect_machine_info, list_sessions, load_session_raw_data,
};
pub use source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
pub use telemetry::{
    MODEL_ID as TELEMETRY_MODEL_ID, MODEL_VERSION as TELEMETRY_MODEL_VERSION, TelemetryMetric,
    TelemetryMetricDelta, TelemetrySnapshot, TelemetryWindowReport, build_telemetry_window,
    collect_telemetry_snapshot, collect_telemetry_window,
};
pub use trials::{
    CalibrationResult, StoufferResult, TrialAnalysis, TrialConfig, calibration_check,
    stouffer_combine, trial_analysis,
};
pub use verdict::{
    Verdict, metric_or_na, verdict_autocorr, verdict_bias, verdict_bientropy, verdict_compression,
    verdict_corrdim, verdict_distribution, verdict_hurst, verdict_lyapunov, verdict_runs,
    verdict_spectral, verdict_stationarity,
};
/// Library version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
