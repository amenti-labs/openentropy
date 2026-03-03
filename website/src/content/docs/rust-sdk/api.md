---
title: 'Rust API Reference'
description: 'Complete Rust API reference for openentropy-core and related crates'
---

Accurate reference for the current Rust workspace API.

For Python bindings, see [Python SDK](/openentropy/python-sdk/).

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
