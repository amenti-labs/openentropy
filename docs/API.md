# API Reference

## Rust API (`esoteric-core`)

### `EntropyPool`

The central type. Manages multiple entropy sources, collects and conditions output.

```rust
use esoteric_core::EntropyPool;
```

#### `EntropyPool::new(seed: Option<&[u8]>) -> Self`

Create an empty pool with optional seed. If no seed is provided, the pool is initialized with OS entropy.

#### `EntropyPool::auto() -> Self`

Create a pool with all available sources auto-detected on this machine.

```rust
let pool = EntropyPool::auto();
println!("{} sources", pool.source_count());
```

#### `pool.add_source(source: Box<dyn EntropySource>, weight: f64)`

Register an entropy source with a weight.

#### `pool.source_count() -> usize`

Number of registered sources.

#### `pool.collect_all() -> usize`

Collect entropy from every registered source (serial). Returns total raw bytes collected.

#### `pool.collect_all_parallel(timeout_secs: f64) -> usize`

Collect entropy from all sources in parallel using threads. Returns total raw bytes collected.

#### `pool.get_random_bytes(n_bytes: usize) -> Vec<u8>`

Return `n_bytes` of SHA-256 conditioned random output. Automatically collects from sources if the buffer is low.

```rust
let bytes = pool.get_random_bytes(256);
assert_eq!(bytes.len(), 256);
```

#### `pool.health_report() -> HealthReport`

Returns a structured health report with per-source details.

#### `pool.print_health()`

Pretty-print the health report to stdout.

#### `pool.source_infos() -> Vec<SourceInfoSnapshot>`

Get metadata (name, description, physics, category) for each registered source.

---

### `HealthReport`

```rust
pub struct HealthReport {
    pub healthy: usize,         // Number of healthy sources
    pub total: usize,           // Total registered sources
    pub raw_bytes: u64,         // Total raw bytes collected
    pub output_bytes: u64,      // Total conditioned output bytes
    pub buffer_size: usize,     // Current internal buffer size
    pub sources: Vec<SourceHealth>,
}
```

### `SourceHealth`

```rust
pub struct SourceHealth {
    pub name: String,      // Source name
    pub healthy: bool,     // Currently healthy (entropy > 1.0 bits/byte)
    pub bytes: u64,        // Total bytes collected from this source
    pub entropy: f64,      // Shannon entropy of last collection (bits/byte, max 8.0)
    pub time: f64,         // Time for last collection (seconds)
    pub failures: u64,     // Number of collection failures
}
```

### `SourceInfoSnapshot`

```rust
pub struct SourceInfoSnapshot {
    pub name: String,
    pub description: String,
    pub physics: String,
    pub category: String,
    pub entropy_rate_estimate: f64,
}
```

---

### `EntropySource` (Trait)

Every entropy source implements this trait.

```rust
pub trait EntropySource: Send + Sync {
    fn info(&self) -> &SourceInfo;
    fn is_available(&self) -> bool;
    fn collect(&self, n_samples: usize) -> Vec<u8>;
    fn name(&self) -> &'static str;  // Default: self.info().name
}
```

### `SourceInfo`

```rust
pub struct SourceInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub physics: &'static str,
    pub category: SourceCategory,
    pub platform_requirements: &'static [&'static str],
    pub entropy_rate_estimate: f64,
}
```

### `SourceCategory`

```rust
pub enum SourceCategory {
    Timing,      // Clock jitter, scheduler noise
    System,      // Kernel counters, process table
    Network,     // DNS/TCP latency, WiFi RSSI
    Hardware,    // Disk I/O, DRAM, GPU timing
    Silicon,     // CPU speculative execution, cache
    CrossDomain, // Multi-subsystem combination
    Novel,       // Spotlight, dyld, dispatch queue
}
```

---

### Conditioning Functions

```rust
use esoteric_core::conditioning::{quick_shannon, quick_quality};
```

#### `quick_shannon(data: &[u8]) -> f64`

Shannon entropy in bits per byte (max 8.0).

#### `quick_quality(data: &[u8], label: &str) -> QualityReport`

Quick quality assessment with grade (A-F), Shannon entropy, and basic statistics.

---

### Platform Detection

```rust
use esoteric_core::{detect_available_sources, platform_info};

let sources = detect_available_sources(); // Vec<Box<dyn EntropySource>>
let info = platform_info();               // PlatformInfo struct
```

---

## NIST Test Battery (`esoteric-tests`)

```rust
use esoteric_tests::{run_all_tests, calculate_quality_score, TestResult};

let data: Vec<u8> = pool.get_random_bytes(10_000);
let results: Vec<TestResult> = run_all_tests(&data);
let score: f64 = calculate_quality_score(&results);

for r in &results {
    println!("{}: {} (p={:?})", r.name, r.grade, r.p_value);
}
```

### `TestResult`

```rust
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub p_value: Option<f64>,
    pub statistic: f64,
    pub details: String,
    pub grade: char,  // 'A' through 'F'
}
```

31 tests based on NIST SP 800-22: frequency, block frequency, runs, longest run, spectral, non-overlapping template, overlapping template, universal, approximate entropy, serial, cumulative sums, random excursion, linear complexity, GF(2) matrix rank, and more.

---

## HTTP Server (`esoteric-server`)

ANU QRNG API-compatible HTTP server built on axum.

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/random?length=N&type=T` | GET | Random data (hex16, uint8, uint16) |
| `/health` | GET | Server health check |
| `/sources` | GET | Active entropy sources with status |
| `/pool/status` | GET | Detailed pool health report |

### Query Parameters for `/api/v1/random`

| Parameter | Values | Default | Description |
|-----------|--------|---------|-------------|
| `length` | 1-65536 | 1024 | Number of values to return |
| `type` | `hex16`, `uint8`, `uint16` | `hex16` | Output format |

### Response Example

```json
{
    "type": "uint8",
    "length": 256,
    "data": [142, 87, 203, 51, 174, 9, 230, 118, "..."],
    "success": true
}
```

See [OLLAMA_INTEGRATION.md](OLLAMA_INTEGRATION.md) for full endpoint documentation and curl examples.
