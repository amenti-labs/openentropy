---
title: 'Rust API Reference'
description: 'Complete Rust API reference for openentropy-core and related crates'
---

Accurate reference for the current Rust workspace API.

For Python bindings, see [Python SDK](/openentropy/python-sdk/).

For day-to-day usage, start with:

- [Rust Quick Reference](/openentropy/rust-sdk/quick-reference/)
- [Rust Analysis Workflows](/openentropy/rust-sdk/analysis/)

Use this page for complete type and function coverage.

## openentropy-core

Crate: `openentropy-core`  
Path: `crates/openentropy-core/`

### Public re-exports (`openentropy_core`)

```rust
pub use conditioning::{
    ConditioningMode, MinEntropyReport, QualityReport, condition, grade_min_entropy,
    min_entropy_estimate, quick_autocorrelation_lag1, quick_min_entropy, quick_quality, quick_shannon,
};
pub use platform::{detect_available_sources, platform_info};
pub use pool::{EntropyPool, HealthReport, SourceHealth, SourceInfoSnapshot};
pub use comparison::{
    AggregateDelta, ComparisonResult, DigramAnalysis, MarkovAnalysis, MultiLagAnalysis,
    RunLengthComparison, TemporalAnalysis, TwoSampleTests, WindowAnomaly, aggregate_delta,
    cliffs_delta, compare, compare_with_analysis, digram_analysis, markov_analysis,
    multi_lag_analysis, run_length_comparison, temporal_analysis, two_sample_tests,
};
pub use trials::{
    CalibrationResult, StoufferResult, TrialAnalysis, TrialConfig, calibration_check,
    stouffer_combine, trial_analysis,
};
pub use chaos::{
    BiEntropyResult, ChaosAnalysis, CorrelationDimResult, EpiplexityResult, HurstResult,
    LyapunovResult, bientropy, chaos_analysis, correlation_dimension, epiplexity,
    hurst_exponent, lyapunov_exponent,
};
pub use dispatcher::{
    AnalysisConfig, AnalysisProfile, AnalysisReport, SourceReport, VerdictSummary, analyze,
};
pub use session::{
    MachineInfo, SessionConfig, SessionMeta, SessionSourceAnalysis, SessionWriter,
    detect_machine_info,
};
pub use source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

### `EntropyPool` (`openentropy_core::pool`)

```rust
pub fn new(seed: Option<&[u8]>) -> Self
pub fn auto() -> Self
pub fn add_source(&mut self, source: Box<dyn EntropySource>)
pub fn source_count(&self) -> usize

pub fn collect_all(&self) -> usize
pub fn collect_all_parallel(&self, timeout_secs: f64) -> usize
pub fn collect_enabled(&self, enabled_names: &[String]) -> usize
pub fn collect_enabled_n(&self, enabled_names: &[String], n_samples: usize) -> usize

pub fn get_raw_bytes(&self, n_bytes: usize) -> Vec<u8>
pub fn get_random_bytes(&self, n_bytes: usize) -> Vec<u8>
pub fn get_bytes(&self, n_bytes: usize, mode: ConditioningMode) -> Vec<u8>
pub fn get_source_bytes(
    &self,
    source_name: &str,
    n_bytes: usize,
    mode: ConditioningMode,
) -> Option<Vec<u8>>
pub fn get_source_raw_bytes(&self, source_name: &str, n_samples: usize) -> Option<Vec<u8>>

pub fn health_report(&self) -> HealthReport
pub fn print_health(&self)
pub fn source_names(&self) -> Vec<String>
pub fn source_infos(&self) -> Vec<SourceInfoSnapshot>
```

### Pool report types

```rust
pub struct HealthReport {
    pub healthy: usize,
    pub total: usize,
    pub raw_bytes: u64,
    pub output_bytes: u64,
    pub buffer_size: usize,
    pub sources: Vec<SourceHealth>,
}

pub struct SourceHealth {
    pub name: String,
    pub healthy: bool,
    pub bytes: u64,
    pub entropy: f64,
    pub min_entropy: f64,
    pub autocorrelation: f64,
    pub time: f64,
    pub failures: u64,
}

pub struct SourceInfoSnapshot {
    pub name: String,
    pub description: String,
    pub physics: String,
    pub category: String,
    pub platform: String,
    pub requirements: Vec<String>,
    pub entropy_rate_estimate: f64,
    pub composite: bool,
    pub config: Vec<(&'static str, String)>,
}
```

### `EntropySource` and metadata (`openentropy_core::source`)

```rust
pub trait EntropySource: Send + Sync {
    fn info(&self) -> &SourceInfo;
    fn is_available(&self) -> bool;
    fn collect(&self, n_samples: usize) -> Vec<u8>;
    fn name(&self) -> &'static str { self.info().name }
}
```

```rust
pub struct SourceInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub physics: &'static str,
    pub category: SourceCategory,
    pub platform: Platform,
    pub requirements: &'static [Requirement],
    pub entropy_rate_estimate: f64,
    pub composite: bool,
    pub is_fast: bool,
}
```

```rust
pub enum Platform { Any, MacOS, Linux }
```

```rust
pub enum Requirement {
    Metal,
    AudioUnit,
    Wifi,
    Usb,
    Camera,
    AppleSilicon,
    Bluetooth,
    IOKit,
    IOSurface,
    SecurityFramework,
    RawBlockDevice,
}
```

```rust
pub enum SourceCategory {
    Thermal,
    Timing,
    Scheduling,
    IO,
    IPC,
    Microarch,
    GPU,
    Network,
    System,
    Quantum,
    Signal,
    Sensor,
}
```

### Source discovery and registry

```rust
pub fn detect_available_sources() -> Vec<Box<dyn EntropySource>>
pub fn platform_info() -> PlatformInfo
```

```rust
pub fn all_sources() -> Vec<Box<dyn EntropySource>> // currently 63 sources
```

## openentropy-tests

Crate: `openentropy-tests`  
Path: `crates/openentropy-tests/`

```rust
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub p_value: Option<f64>,
    pub statistic: f64,
    pub details: String,
    pub grade: char,
}

pub fn run_all_tests(data: &[u8]) -> Vec<TestResult>
pub fn calculate_quality_score(results: &[TestResult]) -> f64
```

## openentropy-server

Crate: `openentropy-server`  
Path: `crates/openentropy-server/`

```rust
pub async fn run_server(pool: EntropyPool, host: &str, port: u16, allow_raw: bool) -> std::io::Result<()>
```

HTTP endpoints:

- `GET /api/v1/random?length=N&type=T[&raw=true|&conditioning=...]`
- `GET /health`
- `GET /sources`
- `GET /pool/status`

## openentropy-cli

Crate: `openentropy-cli`  
Binary: `openentropy`  
Path: `crates/openentropy-cli/`

Subcommands:

- `scan`
- `bench`
- `analyze`
- `stream`
- `server`
- `monitor`
- `record`
- `sessions`

## Benchmark Module (`openentropy_core::benchmark`)

### `benchmark_sources(pool: &EntropyPool, config: &BenchConfig) -> Result<BenchReport, BenchError>`

Run a multi-round benchmark across all sources in a pool.

```rust
use openentropy_core::{EntropyPool, benchmark::{benchmark_sources, BenchConfig}};

let pool = EntropyPool::auto();
let config = BenchConfig::default();
let report = benchmark_sources(&pool, &config)?;
for src in &report.sources {
    println!("{}: grade={} score={:.3}", src.name, src.grade, src.score);
}
```

**`BenchConfig` fields** (all public):
- `samples_per_round: usize` — default 2048
- `rounds: usize` — default 3
- `warmup_rounds: usize` — default 1
- `timeout_sec: f64` — default 2.0
- `rank_by: RankBy` — `Balanced` | `MinEntropy` | `Throughput`, default `Balanced`
- `include_pool_quality: bool` — default true
- `pool_quality_bytes: usize` — default 65536
- `conditioning: ConditioningMode` — default `Sha256`

**`BenchReport` fields**: `generated_unix`, `config`, `sources: Vec<BenchSourceReport>`, `pool: Option<PoolQualityReport>`

**`BenchSourceReport` fields**: `name`, `composite`, `healthy`, `success_rounds`, `failures`, `avg_shannon`, `avg_min_entropy`, `avg_throughput_bps`, `avg_autocorrelation`, `p99_latency_ms`, `stability`, `grade: char`, `score: f64`

## Session Utilities (`openentropy_core::session`)

### `list_sessions(dir: &Path) -> Result<Vec<(PathBuf, SessionMeta)>, std::io::Error>`

List all recorded sessions in a directory, sorted newest-first. Returns empty Vec for nonexistent directory.

### `load_session_raw_data(session_dir: &Path) -> Result<HashMap<String, Vec<u8>>, std::io::Error>`

Load raw entropy data from a session directory. Returns a map of source name → raw bytes.

```rust
use openentropy_core::{list_sessions, load_session_raw_data, full_analysis};
use std::path::Path;

let sessions = list_sessions(Path::new("sessions"))?;
for (path, meta) in &sessions {
    println!("{}: {} samples", meta.id, meta.total_samples);
    let raw = load_session_raw_data(path)?;
    for (source, data) in &raw {
        let analysis = full_analysis(source, data);
        println!("  {}: H∞={:.4}", source, analysis.min_entropy);
    }
}
```

## Forensic Analysis (`openentropy_core::analysis`)

Core statistical analysis battery for evaluating entropy quality. Six tests
that assess the fundamental properties expected of random data. See
[Analysis System](/openentropy/concepts/analysis/) for interpretation guides
and verdict thresholds.

### `full_analysis(source_name: &str, data: &[u8]) -> SourceAnalysis`

Run all six forensic tests in one call. Returns a `SourceAnalysis` containing
autocorrelation, spectral, bit bias, distribution, stationarity, and runs
results, plus Shannon and min-entropy estimates.

```rust
use openentropy_core::full_analysis;

let analysis = full_analysis("clock_jitter", &data);
println!("Shannon: {:.4} bits/byte", analysis.shannon_entropy);
println!("Min-entropy: {:.4}", analysis.min_entropy);
println!("Spectral flatness: {:.4}", analysis.spectral.flatness);
println!("Stationary: {}", analysis.stationarity.is_stationary);
```

### Individual functions

| Function | Returns | Description |
|----------|---------|-------------|
| `autocorrelation_profile(data, max_lag)` | `AutocorrResult` | Serial dependence at multiple lags |
| `spectral_analysis(data)` | `SpectralResult` | FFT-based power spectral density |
| `bit_bias(data)` | `BitBiasResult` | Per-bit deviation from 50/50 |
| `distribution_stats(data)` | `DistributionResult` | Byte-value distribution vs uniform |
| `stationarity_test(data)` | `StationarityResult` | ANOVA stability over 10 windows |
| `runs_analysis(data)` | `RunsResult` | Consecutive identical value patterns |
| `cross_correlation_matrix(sources)` | `CrossCorrMatrix` | Pairwise Pearson correlation, flags \|r\| > 0.3 |
| `pearson_correlation(a, b)` | `f64` | Pearson correlation coefficient |

## Chaos Theory Analysis (`openentropy_core::chaos`)

Distinguish true randomness from deterministic chaos using five independent metrics.
See [Chaos Theory Analysis](/openentropy/concepts/analysis-chaos/)
for interpretation guides and verdict thresholds.

### `chaos_analysis(data: &[u8]) -> ChaosAnalysis`

Run the full chaos analysis battery on a byte stream. Returns a `ChaosAnalysis` containing Hurst exponent, Lyapunov exponent, correlation dimension, BiEntropy, and epiplexity.

```rust
use openentropy_core::chaos::chaos_analysis;

let result = chaos_analysis(&data);
println!("Hurst H={:.4} (valid={})", result.hurst.hurst_exponent, result.hurst.is_valid);
println!("Lyapunov λ={:.4}", result.lyapunov.lyapunov_exponent);
println!("Correlation dim D₂={:.4}", result.correlation_dimension.dimension);
println!("BiEntropy={:.4}, TBiEn={:.4}", result.bientropy.bien, result.bientropy.tbien);
println!("Compression ratio={:.4}", result.epiplexity.compression_ratio);
```

### Individual functions

| Function | Returns | Description |
|----------|---------|-------------|
| `hurst_exponent(data)` | `HurstResult` | Rescaled range (R/S) analysis — H≈0.5 = random |
| `lyapunov_exponent(data)` | `LyapunovResult` | Largest Lyapunov exponent — λ>0 = chaotic |
| `correlation_dimension(data)` | `CorrelationDimensionResult` | Grassberger–Procaccia D₂ estimate |
| `bientropy(data)` | `BiEntropyResult` | Binary entropy derivative (BiEn, TBiEn) |
| `epiplexity(data)` | `EpiplexityResult` | Compression-ratio complexity metric |

### Interpreting results

For **true random** data, expect: Hurst H ≈ 0.5, Lyapunov λ > 0 (sensitive dependence), high correlation dimension, BiEntropy near maximum, compression ratio near 1.0. See the [Verdict System](/openentropy/concepts/analysis-verdicts/) for automated pass/fail classification of each metric.

## Unified Analysis Dispatcher (`openentropy_core::dispatcher`)

Run multiple analysis modules through a single entry point with configurable profiles.

### `analyze(sources: &[(&str, &[u8])], config: &AnalysisConfig) -> AnalysisReport`

Dispatch analysis across one or more labeled byte streams. The config controls which modules run.

```rust
use openentropy_core::dispatcher::{analyze, AnalysisConfig, AnalysisProfile};

// Use a preset profile
let config = AnalysisProfile::Deep.to_config();
let report = analyze(&[("clock_jitter", &data)], &config);

for source in &report.sources {
    println!("{}: forensic={} chaos={} trials={}",
        source.label,
        source.forensic.is_some(),
        source.chaos.is_some(),
        source.trials.is_some(),
    );
    println!(
        "  Verdicts: bias={:?} hurst={:?}",
        source.verdicts.bias,
        source.verdicts.hurst
    );
}
```

### `AnalysisConfig` fields

| Field | Type | Description |
|-------|------|-------------|
| `forensic` | `bool` | Run `full_analysis` (autocorrelation, spectral, bias, distribution, stationarity, runs) |
| `entropy` | `bool` | Run `min_entropy_estimate` (detailed entropy breakdown) |
| `chaos` | `bool` | Run `chaos_analysis` (Hurst, Lyapunov, correlation dimension, BiEntropy, epiplexity) |
| `trials` | `Option<TrialConfig>` | Run `trial_analysis` with given config; `None` = skip |
| `cross_correlation` | `bool` | Run `cross_correlation_matrix` when 2+ sources present |

### `AnalysisProfile` presets

| Profile | Forensic | Entropy | Chaos | Trials | Cross-Correlation |
|---------|----------|---------|-------|--------|-------------------|
| `Quick` | ✓ | — | — | — | — |
| `Standard` | ✓ | — | — | — | — |
| `Deep` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `Security` | ✓ | ✓ | — | — | — |

### Custom config

```rust
use openentropy_core::dispatcher::{analyze, AnalysisConfig};
use openentropy_core::trials::TrialConfig;

let config = AnalysisConfig {
    forensic: true,
    entropy: false,
    chaos: true,
    trials: Some(TrialConfig::default()),
    cross_correlation: false,
};

let report = analyze(&[
    ("source_a", &data_a),
    ("source_b", &data_b),
], &config);
```

## Verdict System (`openentropy_core::verdict`)

Automated pass/fail classification for every forensic and chaos metric.
Each source report includes a `VerdictSummary` with up to 11 verdict fields.
See [Verdict System](/openentropy/concepts/analysis-verdicts/) for
all thresholds and interpretation guidance.

```rust
use openentropy_core::dispatcher::{analyze, AnalysisProfile, VerdictSummary};

let config = AnalysisProfile::Deep.to_config();
let report = analyze(&[("src", &data)], &config);

for source in &report.sources {
    let v = &source.verdicts;
    println!("Autocorrelation: {:?}", v.autocorrelation);
    println!("Spectral: {:?}", v.spectral);
    println!("Bias: {:?}", v.bias);
    println!("Distribution: {:?}", v.distribution);
    println!("Stationarity: {:?}", v.stationarity);
    println!("Runs: {:?}", v.runs);
    println!("Hurst: {:?}", v.hurst);
    println!("Lyapunov: {:?}", v.lyapunov);
    println!("Correlation dim: {:?}", v.correlation_dimension);
    println!("BiEntropy: {:?}", v.bientropy);
    println!("Compression: {:?}", v.compression);
}
```

Verdict values: `Pass`, `Warn`, `Fail`, `Na`. Serializes as `"PASS"`, `"WARN"`, `"FAIL"`, `"N/A"`.

## Trial Analysis (`openentropy_core::trials`)

PEAR-style 200-bit trial analysis. Slices byte data into fixed-length trials
and computes Z-scores, cumulative deviation, and effect sizes. See
[Trial Analysis Methodology](/openentropy/concepts/trials/) for the
statistical model.

### `trial_analysis(data: &[u8], config: &TrialConfig) -> TrialAnalysis`

```rust
use openentropy_core::trials::{trial_analysis, TrialConfig};

let config = TrialConfig::default(); // 200 bits per trial
let result = trial_analysis(&data, &config);
println!("Trials: {}, Terminal Z: {:.4}, Effect: {:.6}, p={:.4}",
    result.num_trials, result.terminal_z,
    result.effect_size, result.terminal_p_value);
```

### `stouffer_combine(analyses: &[&TrialAnalysis]) -> StoufferResult`

Weighted Stouffer composition across multiple sessions (weights = √num\_trials).

```rust
use openentropy_core::trials::{trial_analysis, stouffer_combine, TrialConfig};

let config = TrialConfig::default();
let t1 = trial_analysis(&data_a, &config);
let t2 = trial_analysis(&data_b, &config);
let combined = stouffer_combine(&[&t1, &t2]);
println!("Combined Z: {:.4}, p={:.4}", combined.stouffer_z, combined.p_value);
```

### `calibration_check(data: &[u8], config: &TrialConfig) -> CalibrationResult`

Pre-recording suitability check. Thresholds: |Z| < 2.0, bit bias < 0.005,
Shannon > 7.9, Z-score std in \[0.85, 1.15\].

```rust
use openentropy_core::trials::{calibration_check, TrialConfig};

let result = calibration_check(&data, &TrialConfig::default());
println!("Suitable: {}, Warnings: {:?}", result.is_suitable, result.warnings);
```

## Comparison (`openentropy_core::comparison`)

Differential statistical analysis between two byte streams. See
[Analysis System](/openentropy/concepts/analysis/) for context on how
comparison fits into the analysis pipeline.

### `compare(label_a, data_a, label_b, data_b) -> ComparisonResult`

Full differential report including aggregate deltas, two-sample tests,
temporal analysis, digram analysis, Markov transitions, multi-lag
autocorrelation, and run-length distributions.

```rust
use openentropy_core::compare;

let result = compare("session_a", &data_a, "session_b", &data_b);
println!("KS p-value: {:.4}", result.two_sample.ks_p_value);
println!("Cliff's d: {:.4}", result.two_sample.cliffs_delta);
```

### Individual comparison functions

| Function | Returns | Description |
|----------|---------|-------------|
| `aggregate_delta(a, b)` | `AggregateDelta` | Shannon/min-entropy/mean/variance deltas, Cohen's d |
| `two_sample_tests(a, b)` | `TwoSampleTests` | KS, chi-squared, Mann-Whitney, Cliff's delta |
| `cliffs_delta(a, b)` | `f64` | Non-parametric effect size \[-1, 1\] |
| `temporal_analysis(a, b, window, z)` | `TemporalAnalysis` | Sliding-window anomaly detection |
| `digram_analysis(a, b)` | `DigramAnalysis` | Digram chi-squared uniformity |
| `markov_analysis(a, b)` | `MarkovAnalysis` | Per-bit transition probabilities |
| `multi_lag_analysis(a, b)` | `MultiLagAnalysis` | Autocorrelation at multiple lags |
| `run_length_comparison(a, b)` | `RunLengthComparison` | Byte run-length distributions |
