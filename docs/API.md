# Rust API Reference

Accurate reference for the current Rust workspace API.

For Python bindings, see `docs/PYTHON_SDK.md`.

## openentropy-core

Crate: `openentropy-core`  
Path: `crates/openentropy-core/`

### Public re-exports (`openentropy_core`)

```rust
pub use conditioning::{
    ConditioningMode, MinEntropyReport, QualityReport, condition, grade_min_entropy,
    min_entropy_estimate, quick_min_entropy, quick_quality, quick_shannon,
};
pub use platform::{detect_available_sources, platform_info};
pub use pool::{EntropyPool, HealthReport, SourceHealth, SourceInfoSnapshot};
pub use source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

### `EntropyPool` (`openentropy_core::pool`)

```rust
pub fn new(seed: Option<&[u8]>) -> Self
pub fn auto() -> Self
pub fn add_source(&mut self, source: Box<dyn EntropySource>, weight: f64)
pub fn source_count(&self) -> usize

pub fn collect_all(&self) -> usize
pub fn collect_all_parallel(&self, timeout_secs: f64) -> usize
pub fn collect_enabled(&self, enabled_names: &[String]) -> usize
pub fn collect_enabled_n(&self, enabled_names: &[String], n_samples: usize) -> usize

pub fn get_raw_bytes(&self, n_bytes: usize) -> Vec<u8>
pub fn get_random_bytes(&self, n_bytes: usize) -> Vec<u8>
pub fn get_bytes(&self, n_bytes: usize, mode: ConditioningMode) -> Vec<u8>

pub fn health_report(&self) -> HealthReport
pub fn print_health(&self)
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
    Composite,
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
pub fn all_sources() -> Vec<Box<dyn EntropySource>> // currently 44 sources
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
pub async fn run_server(pool: EntropyPool, host: &str, port: u16, allow_raw: bool)
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
- `device`
- `server`
- `monitor`
- `report`
- `record`
- `sessions`
