# Rust API Reference

Complete API documentation for the openentropy Rust crates. For Python API, see [PYTHON_SDK.md](PYTHON_SDK.md).

---

## openentropy-core

The foundational library crate. Provides entropy sources, pool management, conditioning, and platform detection.

**Crate:** `openentropy-core`
**Path:** `crates/openentropy-core/`

### Public Re-exports (`openentropy_core`)

```rust
pub use conditioning::{QualityReport, quick_quality, quick_shannon};
pub use platform::{detect_available_sources, platform_info};
pub use pool::EntropyPool;
pub use source::{EntropySource, SourceCategory, SourceInfo};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

---

### EntropyPool

Thread-safe multi-source entropy pool with SHA-256 conditioning.

**Module:** `openentropy_core::pool`

```rust
pub struct EntropyPool {
    // All fields are Mutex-wrapped for thread safety
    sources: Vec<Mutex<SourceState>>,
    buffer: Mutex<Vec<u8>>,
    state: Mutex<[u8; 32]>,
    counter: Mutex<u64>,
    total_output: Mutex<u64>,
}
```

#### Construction

```rust
/// Create an empty pool with optional seed.
/// If no seed is provided, the initial state is seeded from /dev/urandom.
pub fn new(seed: Option<&[u8]>) -> Self

/// Create a pool with all available sources auto-discovered on this machine.
/// Calls detect_available_sources() and adds each source with weight 1.0.
pub fn auto() -> Self
```

**Example:**

```rust
use openentropy_core::EntropyPool;

// Auto-discover all available sources
let pool = EntropyPool::auto();
println!("{} sources available", pool.source_count());

// Create with explicit seed
let pool = EntropyPool::new(Some(b"my-seed"));

// Create empty and add sources manually
let mut pool = EntropyPool::new(None);
```

#### Source Management

```rust
/// Register an entropy source with a collection weight.
/// The source must implement EntropySource + Send + Sync.
pub fn add_source(&mut self, source: Box<dyn EntropySource>, weight: f64)

/// Number of registered sources.
pub fn source_count(&self) -> usize
```

#### Collection

```rust
/// Collect entropy from every registered source (serial).
/// Returns the total number of raw bytes collected.
/// Each source's collect() is called with n_samples=1000.
/// Sources that panic are caught via catch_unwind and marked unhealthy.
pub fn collect_all(&self) -> usize

/// Collect entropy from all sources in parallel using scoped threads.
/// Each source runs in its own thread with a shared deadline.
/// Returns the total number of raw bytes collected.
pub fn collect_all_parallel(&self, timeout_secs: f64) -> usize
```

#### Random Output

```rust
/// Return n_bytes of SHA-256 conditioned random output.
///
/// Auto-collects from sources if the buffer is less than 2x the requested size.
/// Each 32-byte output block mixes:
///   1. Internal state (32 bytes, chained from previous output)
///   2. Pool buffer (up to 256 bytes drained from source buffer)
///   3. Monotonic counter (u64, prevents repetition)
///   4. System timestamp (nanoseconds since epoch)
///   5. 8 bytes from /dev/urandom (safety net)
///
/// The output digest becomes the new internal state (forward secrecy).
pub fn get_random_bytes(&self, n_bytes: usize) -> Vec<u8>
```

**Example:**

```rust
let pool = EntropyPool::auto();
let bytes = pool.get_random_bytes(256);
assert_eq!(bytes.len(), 256);

// Large block
let megabyte = pool.get_random_bytes(1_048_576);
```

#### Health Monitoring

```rust
/// Health report with per-source statistics.
pub fn health_report(&self) -> HealthReport

/// Pretty-print health report to stdout.
pub fn print_health(&self)

/// Get source metadata snapshots for all registered sources.
pub fn source_infos(&self) -> Vec<SourceInfoSnapshot>
```

#### Associated Types

```rust
pub struct HealthReport {
    pub healthy: usize,          // Number of healthy sources
    pub total: usize,            // Total registered sources
    pub raw_bytes: u64,          // Total raw bytes collected (lifetime)
    pub output_bytes: u64,       // Total conditioned bytes output (lifetime)
    pub buffer_size: usize,      // Current buffer size in bytes
    pub sources: Vec<SourceHealth>,
}

pub struct SourceHealth {
    pub name: String,
    pub healthy: bool,           // true if last_entropy > 1.0 bits/byte
    pub bytes: u64,              // Total bytes from this source
    pub entropy: f64,            // Shannon entropy of last collection (bits/byte)
    pub time: f64,               // Last collection time in seconds
    pub failures: u64,           // Lifetime failure count
}

pub struct SourceInfoSnapshot {
    pub name: String,
    pub description: String,
    pub physics: String,
    pub category: String,
    pub entropy_rate_estimate: f64,
}
```

---

### EntropySource Trait

The interface every entropy source must implement.

**Module:** `openentropy_core::source`

```rust
/// Trait that every entropy source must implement.
/// Sources must be Send + Sync to support parallel collection.
pub trait EntropySource: Send + Sync {
    /// Source metadata: name, description, physics, category, platform requirements.
    fn info(&self) -> &SourceInfo;

    /// Check if this source can operate on the current machine.
    /// Must be fast and side-effect-free.
    fn is_available(&self) -> bool;

    /// Collect raw entropy samples.
    /// Returns a Vec<u8> of up to n_samples bytes.
    /// Must handle hardware failures gracefully (return empty Vec).
    fn collect(&self, n_samples: usize) -> Vec<u8>;

    /// Convenience: name from info.
    fn name(&self) -> &'static str {
        self.info().name
    }
}
```

### SourceInfo

Static metadata attached to each source implementation.

```rust
pub struct SourceInfo {
    pub name: &'static str,                             // e.g. "clock_jitter"
    pub description: &'static str,                      // Short human description
    pub physics: &'static str,                          // Detailed physics explanation
    pub category: SourceCategory,                       // Category enum
    pub platform_requirements: &'static [&'static str], // e.g. &["macOS"]
    pub entropy_rate_estimate: f64,                     // Estimated bits per second
}
```

### SourceCategory

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCategory {
    Timing,       // Clock phase noise, scheduler jitter
    System,       // Kernel counters, process tables
    Network,      // DNS latency, TCP timing, WiFi RSSI
    Hardware,     // Disk I/O, memory, GPU, audio, camera, sensors
    Silicon,      // DRAM row buffer, cache, page faults, speculative exec
    CrossDomain,  // Beat frequencies between clock domains
    Novel,        // GCD dispatch, dyld timing, VM pages, Spotlight
}

// Implements Display: formats as lowercase string
// e.g. SourceCategory::Silicon => "silicon"
```

### SourceState

Runtime state for a source registered in the pool.

```rust
pub struct SourceState {
    pub source: Box<dyn EntropySource>,
    pub weight: f64,                    // Collection weight
    pub total_bytes: u64,               // Lifetime bytes collected
    pub failures: u64,                  // Collection failure count
    pub last_entropy: f64,              // Shannon entropy of last collection
    pub last_collect_time: Duration,    // Duration of last collection
    pub healthy: bool,                  // true if last_entropy > 1.0
}

impl SourceState {
    pub fn new(source: Box<dyn EntropySource>, weight: f64) -> Self
}
```

---

### Conditioning Functions

**Module:** `openentropy_core::conditioning`

```rust
/// SHA-256 condition raw entropy into high-quality output.
/// Feeds: state || sample || counter || timestamp || extra.
/// Returns (new_state, output_block) where both are [u8; 32].
pub fn sha256_condition(
    state: &[u8; 32],
    sample: &[u8],
    counter: u64,
    extra: &[u8],
) -> ([u8; 32], [u8; 32])

/// Von Neumann debiasing: extract unbiased bits from a biased stream.
/// Takes pairs of bits: (0,1) -> 0, (1,0) -> 1, same -> discard.
/// Returns packed bytes. Throughput is approximately 25% of input.
pub fn von_neumann_debias(data: &[u8]) -> Vec<u8>

/// XOR-fold: reduce data by XORing the first half with the second half.
/// Output length is input_length / 2.
pub fn xor_fold(data: &[u8]) -> Vec<u8>

/// Shannon entropy in bits per byte for a byte slice.
/// Returns a value in [0.0, 8.0]. Higher values indicate more randomness.
/// Maximum (8.0) means perfectly uniform byte distribution.
pub fn quick_shannon(data: &[u8]) -> f64

/// Quick quality assessment with multiple metrics.
/// Computes Shannon entropy, compression ratio, unique value count,
/// and an overall quality score (0-100) with letter grade.
pub fn quick_quality(data: &[u8]) -> QualityReport
```

```rust
pub struct QualityReport {
    pub samples: usize,           // Number of bytes analyzed
    pub unique_values: usize,     // Number of distinct byte values (max 256)
    pub shannon_entropy: f64,     // Bits per byte [0.0, 8.0]
    pub compression_ratio: f64,   // zlib compressed/original (1.0 = incompressible)
    pub quality_score: f64,       // Weighted composite score [0, 100]
    pub grade: char,              // A (>=80), B (>=60), C (>=40), D (>=20), F
}
```

**Quality score formula:**
```
score = (shannon / 8.0) * 60  +  min(compression_ratio, 1.0) * 20  +  (unique / 256) * 20
```

---

### Platform Detection

**Module:** `openentropy_core::platform`

```rust
/// Discover all entropy sources available on this machine.
/// Creates all 30 source instances, calls is_available() on each,
/// and returns only those that pass.
pub fn detect_available_sources() -> Vec<Box<dyn EntropySource>>

/// Platform information: OS, architecture, family.
pub fn platform_info() -> PlatformInfo
```

```rust
pub struct PlatformInfo {
    pub system: String,    // e.g. "macos", "linux"
    pub machine: String,   // e.g. "aarch64", "x86_64"
    pub family: String,    // e.g. "unix"
}
```

---

### Source Registry

**Module:** `openentropy_core::sources`

```rust
/// All 30 entropy source constructors.
/// Returns a Vec of boxed sources in category order:
///   Timing (3) -> System (3) -> Network (3) -> Hardware (5+3) ->
///   Silicon (4) -> Cross-Domain (3) -> Compression/Hash (2) -> Novel (4)
pub fn all_sources() -> Vec<Box<dyn EntropySource>>
```

#### Source Structs by Module

| Module | Structs |
|--------|---------|
| `sources::timing` | `ClockJitterSource`, `MachTimingSource`, `SleepJitterSource` |
| `sources::sysctl` | `SysctlSource` (constructor: `SysctlSource::new()`) |
| `sources::vmstat` | `VmstatSource` (constructor: `VmstatSource::new()`) |
| `sources::process` | `ProcessSource` (constructor: `ProcessSource::new()`) |
| `sources::network` | `DNSTimingSource` (`.new()`), `TCPConnectSource` (`.new()`) |
| `sources::wifi` | `WiFiRSSISource` (constructor: `WiFiRSSISource::new()`) |
| `sources::disk` | `DiskIOSource` |
| `sources::memory` | `MemoryTimingSource` |
| `sources::gpu` | `GPUTimingSource` |
| `sources::audio` | `AudioNoiseSource` |
| `sources::camera` | `CameraNoiseSource` |
| `sources::sensor` | `SensorNoiseSource` |
| `sources::bluetooth` | `BluetoothNoiseSource` |
| `sources::ioregistry` | `IORegistryEntropySource` |
| `sources::silicon` | `DRAMRowBufferSource`, `CacheContentionSource`, `PageFaultTimingSource`, `SpeculativeExecutionSource` |
| `sources::cross_domain` | `CPUIOBeatSource`, `CPUMemoryBeatSource`, `MultiDomainBeatSource` |
| `sources::compression` | `CompressionTimingSource`, `HashTimingSource` |
| `sources::novel` | `DispatchQueueSource`, `DyldTimingSource`, `VMPageTimingSource`, `SpotlightTimingSource` |

---

## openentropy-tests

NIST SP 800-22 inspired randomness test battery with 31 statistical tests.

**Crate:** `openentropy-tests`
**Path:** `crates/openentropy-tests/`

### Core Types

```rust
/// Result of a single randomness test.
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,          // Test name
    pub passed: bool,          // Pass/fail determination
    pub p_value: Option<f64>,  // p-value (None for non-parametric tests)
    pub statistic: f64,        // Test statistic
    pub details: String,       // Human-readable details
    pub grade: char,           // Letter grade: A, B, C, D, or F
}

impl TestResult {
    /// Assign letter grade from p-value:
    ///   A: p >= 0.1
    ///   B: p >= 0.01
    ///   C: p >= 0.001
    ///   D: p >= 0.0001
    ///   F: otherwise or None
    pub fn grade_from_p(p: Option<f64>) -> char

    /// Pass/fail from p-value against threshold (default 0.01).
    pub fn pass_from_p(p: Option<f64>, threshold: f64) -> bool
}
```

### Test Battery

```rust
/// Run the complete 31-test battery on a byte slice.
/// Tests are run sequentially. Each test that panics is caught and
/// returns a failing TestResult.
/// Recommended minimum data size: 10,000 bytes for reliable results.
pub fn run_all_tests(data: &[u8]) -> Vec<TestResult>

/// Calculate overall quality score (0-100) from test results.
/// Each grade maps to a score: A=100, B=75, C=50, D=25, F=0.
/// Returns the mean score across all tests.
pub fn calculate_quality_score(results: &[TestResult]) -> f64
```

**Example:**

```rust
use openentropy_tests::{run_all_tests, calculate_quality_score};

let data = pool.get_random_bytes(10_000);
let results = run_all_tests(&data);
let passed = results.iter().filter(|r| r.passed).count();
let score = calculate_quality_score(&results);

println!("Passed: {}/{}", passed, results.len());
println!("Score: {:.1}/100", score);

for r in &results {
    let status = if r.passed { "PASS" } else { "FAIL" };
    println!("  [{}] {} {} -- {}", status, r.grade, r.name, r.details);
}
```

### Individual Test Functions

All test functions have the signature `fn(data: &[u8]) -> TestResult`. Each test validates minimum data length and returns a failing result with "Insufficient data" details if the input is too short.

#### Frequency Tests (3)

| Function | Min Data | Method |
|----------|---------|--------|
| `monobit_frequency` | 13 bytes | Proportion of 1s vs 0s. Uses erfc for p-value. |
| `block_frequency` | 160 bytes | Chi-squared on 128-bit blocks. |
| `byte_frequency` | 256 bytes | Chi-squared on 256 byte value bins. |

#### Runs Tests (2)

| Function | Min Data | Method |
|----------|---------|--------|
| `runs_test` | 13 bytes | Count of uninterrupted runs of 0s or 1s. |
| `longest_run_of_ones` | 16 bytes | Longest run within 8-bit blocks, chi-squared. |

#### Serial Tests (2)

| Function | Min Data | Method |
|----------|---------|--------|
| `serial_test` | 3 bytes | Overlapping 4-bit pattern frequencies. Truncates to 20K bits. |
| `approximate_entropy` | 8 bytes | Compare 3-bit and 4-bit pattern frequencies. |

#### Spectral Tests (2)

| Function | Min Data | Method |
|----------|---------|--------|
| `dft_spectral` | 8 bytes | Detect periodic features via FFT (rustfft). |
| `spectral_flatness` | 64 bytes | Geometric/arithmetic mean ratio of power spectrum. |

#### Entropy Tests (5)

| Function | Min Data | Method |
|----------|---------|--------|
| `shannon_entropy` | 16 bytes | Bits per byte (max 8.0). Pass threshold: > 0.85 ratio. |
| `min_entropy` | 16 bytes | NIST SP 800-90B: -log2(p_max). Pass threshold: > 0.7 ratio. |
| `permutation_entropy` | 14 bytes | Ordinal pattern complexity (order=4). Pass: > 0.85 normalized. |
| `compression_ratio` | 32 bytes | zlib best compression ratio. Pass: > 0.85. |
| `kolmogorov_complexity` | 32 bytes | Compression at levels 1 and 9, complexity and spread. |

#### Correlation Tests (4)

| Function | Min Data | Method |
|----------|---------|--------|
| `autocorrelation` | 60 bytes | Lags 1-50, Poisson violation counting. |
| `serial_correlation` | 20 bytes | Adjacent value correlation, z-test. |
| `lag_n_correlation` | 42 bytes | Lags [1, 2, 4, 8, 16, 32], threshold-based. |
| `cross_correlation` | 100 bytes | Even vs odd byte independence, Pearson r. |

#### Distribution Tests (2)

| Function | Min Data | Method |
|----------|---------|--------|
| `ks_test` | 50 bytes | Kolmogorov-Smirnov vs uniform. Asymptotic p-value. |
| `anderson_darling` | 50 bytes | A-squared statistic. Critical: 1.933 (5%), 2.492 (2.5%), 3.857 (1%). |

#### Pattern Tests (3)

| Function | Min Data | Method |
|----------|---------|--------|
| `overlapping_template` | 125 bytes | Overlapping bit pattern (1,1,1,1) frequency. |
| `non_overlapping_template` | 125 bytes | Non-overlapping pattern (0,0,1,1). |
| `maurers_universal` | 555 bytes | Universal statistical test (L=6, Q=640). |

#### Advanced Tests (5)

| Function | Min Data | Method |
|----------|---------|--------|
| `binary_matrix_rank` | 4,864 bytes | GF(2) Gaussian elimination on 32x32 matrices. |
| `linear_complexity` | 150 bytes | Berlekamp-Massey LFSR complexity on 200-bit blocks. |
| `cusum_test` | 13 bytes | Cumulative sums drift/bias detection. |
| `random_excursions` | 125 bytes | Cycles in cumulative sum random walk. |
| `birthday_spacing` | 100 bytes | Spacing between repeated values, Poisson test. |

#### Practical Tests (3)

| Function | Min Data | Method |
|----------|---------|--------|
| `bit_avalanche` | 100 bytes | Adjacent bytes should differ by ~4 bits. |
| `monte_carlo_pi` | 200 bytes | Estimate pi using (x,y) pairs. Pass: < 5% error. |
| `mean_variance` | 50 bytes | Mean (~127.5) and variance (~5461.25) of uniform bytes. |

---

## openentropy-server

HTTP entropy server with ANU QRNG API compatibility.

**Crate:** `openentropy-server`
**Path:** `crates/openentropy-server/`

### Primary Function

```rust
/// Run the HTTP entropy server.
///
/// Starts an axum server on the given host and port. The server runs
/// until the process is killed. Uses tokio::sync::Mutex for async safety.
///
/// Endpoints:
///   GET /api/v1/random?length=N&type=T  -- random data
///   GET /health                          -- health check
///   GET /sources                         -- source listing
///   GET /pool/status                     -- detailed pool status
///
/// Parameters for /api/v1/random:
///   length: 1-65536 (default 1024)
///   type: "hex16" (default), "uint8", "uint16"
pub async fn run_server(pool: EntropyPool, host: &str, port: u16)
```

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/random?length=N&type=T` | GET | Random data in specified format |
| `/health` | GET | Server health: status, source counts, byte totals |
| `/sources` | GET | Per-source health with bytes, entropy, timing, failures |
| `/pool/status` | GET | Full pool status including buffer size |

### Response Types

```rust
// /api/v1/random
struct RandomResponse {
    data_type: String,          // "hex16", "uint8", or "uint16"
    length: usize,              // Number of values returned
    data: serde_json::Value,    // Array of values
    success: bool,              // Always true
}

// /health
struct HealthResponse {
    status: String,             // "healthy" or "degraded"
    sources_healthy: usize,
    sources_total: usize,
    raw_bytes: u64,
    output_bytes: u64,
}

// /sources
struct SourcesResponse {
    sources: Vec<SourceEntry>,  // Per-source details
    total: usize,
}
```

See [OLLAMA_INTEGRATION.md](OLLAMA_INTEGRATION.md) for full endpoint documentation, response examples, and curl commands.

---

## openentropy-cli

Command-line binary providing 9 subcommands.

**Crate:** `openentropy-cli`
**Binary name:** `openentropy`
**Path:** `crates/openentropy-cli/`

### Subcommands

| Command | Description | Key Options |
|---------|-------------|-------------|
| `scan` | Discover and list all available entropy sources | -- |
| `probe <source>` | Test a specific source with quality stats | `source_name` (positional) |
| `bench` | Benchmark all sources with ranked report | -- |
| `stream` | Continuous entropy to stdout | `--format raw\|hex\|base64`, `--rate N`, `--bytes N`, `--sources filter` |
| `device <path>` | Create named pipe (FIFO) | `--buffer-size N`, `--sources filter` |
| `server` | Start HTTP entropy server | `--port N`, `--host addr`, `--sources filter` |
| `monitor` | Interactive TUI dashboard | `--refresh secs`, `--sources filter` |
| `report` | NIST test battery with report | `--samples N`, `--source name`, `--output path` |
| `pool` | Display pool health metrics | -- |

### Source Filtering

The `--sources` flag accepts comma-separated name fragments (case-insensitive substring match):

```bash
openentropy stream --sources timing,silicon
# Matches: clock_jitter, mach_timing, sleep_jitter, dram_row_buffer,
#          cache_contention, page_fault_timing, speculative_execution
```

### Helper Function

```rust
/// Build an EntropyPool, optionally filtering sources by name.
/// If no filter is provided, returns EntropyPool::auto().
/// If filter matches no sources, prints a warning and falls back to auto().
pub fn make_pool(source_filter: Option<&str>) -> EntropyPool
```

---

## Usage Examples

### Minimal Rust Usage

```rust
use openentropy_core::EntropyPool;

fn main() {
    let pool = EntropyPool::auto();
    let random_bytes = pool.get_random_bytes(32);
    println!("{:02x?}", random_bytes);
}
```

### Health Monitoring

```rust
use openentropy_core::EntropyPool;

fn main() {
    let pool = EntropyPool::auto();
    pool.collect_all();
    pool.print_health();

    let report = pool.health_report();
    println!("Healthy: {}/{}", report.healthy, report.total);
    println!("Buffer: {} bytes", report.buffer_size);

    for s in &report.sources {
        if !s.healthy {
            println!("WARNING: {} is unhealthy (H={:.2})", s.name, s.entropy);
        }
    }
}
```

### Running NIST Tests

```rust
use openentropy_core::EntropyPool;
use openentropy_tests::{run_all_tests, calculate_quality_score};

fn main() {
    let pool = EntropyPool::auto();
    let data = pool.get_random_bytes(10_000);

    let results = run_all_tests(&data);
    let passed = results.iter().filter(|r| r.passed).count();
    let score = calculate_quality_score(&results);

    println!("Passed: {}/{}", passed, results.len());
    println!("Score: {:.1}/100", score);

    for r in &results {
        let status = if r.passed { "PASS" } else { "FAIL" };
        let p_str = r.p_value.map_or("N/A".to_string(), |p| format!("{:.4}", p));
        println!("  [{}] {} {:30} p={} -- {}",
            status, r.grade, r.name, p_str, r.details);
    }
}
```

### Starting the HTTP Server

```rust
use openentropy_core::EntropyPool;
use openentropy_server::run_server;

#[tokio::main]
async fn main() {
    let pool = EntropyPool::auto();
    println!("Entropy server starting on 127.0.0.1:8042");
    run_server(pool, "127.0.0.1", 8042).await;
}
```

### Custom Entropy Source

```rust
use openentropy_core::source::{EntropySource, SourceCategory, SourceInfo};
use openentropy_core::EntropyPool;

pub struct MySource;

static MY_SOURCE_INFO: SourceInfo = SourceInfo {
    name: "my_source",
    description: "Description of what this measures",
    physics: "Detailed explanation of the physical phenomenon \
              that produces the entropy...",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 500.0,
};

impl EntropySource for MySource {
    fn info(&self) -> &SourceInfo { &MY_SOURCE_INFO }

    fn is_available(&self) -> bool { true }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Your collection logic here
        let mut output = Vec::with_capacity(n_samples);
        for _ in 0..n_samples {
            // Measure some physical quantity and extract LSB
            let measurement = std::time::Instant::now()
                .elapsed().as_nanos() as u8;
            output.push(measurement);
        }
        output
    }
}

fn main() {
    let mut pool = EntropyPool::new(None);
    pool.add_source(Box::new(MySource), 1.0);

    let data = pool.get_random_bytes(32);
    println!("Output: {:02x?}", data);

    pool.print_health();
}
```
