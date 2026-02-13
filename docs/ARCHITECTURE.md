# Architecture

## Overview

openentropy is a multi-source entropy harvesting system written in Rust. It treats every computer as a collection of noisy analog subsystems and extracts randomness from their unpredictable physical behavior. The project is structured as a Cargo workspace with five crates, each with a focused responsibility.

**Version:** 0.3.0 (Rust rewrite)
**Edition:** Rust 2024
**License:** MIT

## Workspace Layout

```
openentropy/
├── Cargo.toml                      # Workspace root
├── crates/
│   ├── openentropy-core/              # Core library
│   │   └── src/
│   │       ├── lib.rs              # Public API re-exports
│   │       ├── source.rs           # EntropySource trait, SourceInfo, SourceCategory
│   │       ├── pool.rs             # EntropyPool — thread-safe multi-source collector
│   │       ├── conditioning.rs     # SHA-256, Von Neumann, XOR-fold, quality metrics
│   │       ├── platform.rs         # Source auto-discovery, platform detection
│   │       └── sources/            # 30 source implementations
│   │           ├── mod.rs          # all_sources() registry
│   │           ├── timing.rs       # ClockJitter, MachTiming, SleepJitter
│   │           ├── sysctl.rs       # Kernel counter mining
│   │           ├── vmstat.rs       # VM subsystem counters
│   │           ├── process.rs      # Process table entropy
│   │           ├── network.rs      # DNS timing, TCP connect
│   │           ├── wifi.rs         # WiFi RSSI noise
│   │           ├── disk.rs         # Block I/O timing
│   │           ├── memory.rs       # DRAM access timing
│   │           ├── gpu.rs          # GPU scheduling jitter
│   │           ├── audio.rs        # Microphone thermal noise
│   │           ├── camera.rs       # Sensor dark current
│   │           ├── sensor.rs       # SMC sensor readouts
│   │           ├── bluetooth.rs    # BLE RF noise
│   │           ├── ioregistry.rs   # IOKit deep mining
│   │           ├── silicon.rs      # DRAM row buffer, cache, page fault, speculative
│   │           ├── cross_domain.rs # Beat frequency sources
│   │           ├── compression.rs  # Compression/hash timing oracles
│   │           └── novel.rs        # GCD dispatch, dyld, VM page, Spotlight
│   │
│   ├── openentropy-cli/               # CLI binary
│   │   └── src/
│   │       ├── main.rs             # clap argument parsing, 9 subcommands
│   │       ├── commands/           # One module per subcommand
│   │       │   ├── mod.rs          # make_pool() helper with source filtering
│   │       │   ├── scan.rs         # Discover available sources
│   │       │   ├── probe.rs        # Test single source quality
│   │       │   ├── bench.rs        # Benchmark all sources with ranking
│   │       │   ├── stream.rs       # Continuous entropy to stdout
│   │       │   ├── device.rs       # Named pipe (FIFO) provider
│   │       │   ├── server.rs       # Launch HTTP server
│   │       │   ├── monitor.rs      # Launch TUI dashboard
│   │       │   ├── report.rs       # NIST test battery with Markdown output
│   │       │   └── pool.rs         # Pool health metrics
│   │       └── tui/                # Interactive dashboard
│   │           ├── mod.rs
│   │           ├── app.rs          # Application state, event loop
│   │           └── ui.rs           # ratatui widget rendering
│   │
│   ├── openentropy-server/            # HTTP entropy server
│   │   └── src/
│   │       └── lib.rs              # axum router, ANU QRNG API compatible
│   │
│   ├── openentropy-tests/             # Statistical test battery
│   │   └── src/
│   │       └── lib.rs              # 31 NIST SP 800-22 inspired tests
│   │
│   └── openentropy-python/            # Python bindings
│       └── src/
│           └── lib.rs              # PyO3 module: EntropyPool, run_all_tests, etc.
│
├── openentropy/               # Pure Python fallback package
├── pyproject.toml                  # Python packaging (pip install)
└── tests/                          # Python integration tests
```

## The Five Crates

### 1. openentropy-core

The foundational library. Contains all 30 entropy source implementations, the mixing pool, conditioning pipeline, quality metrics, and platform detection.

**Key dependencies:** `sha2`, `flate2`, `libc`, `rand`, `tempfile`, `libloading`, `log`

**Public API:**
- `EntropyPool` -- thread-safe multi-source collector with SHA-256 conditioning
- `EntropySource` trait -- interface every source must implement
- `SourceInfo`, `SourceCategory` -- metadata types
- `detect_available_sources()` -- auto-discovery
- `quick_shannon()`, `quick_quality()` -- quality assessment functions

### 2. openentropy-cli

The command-line binary (`openentropy`). Provides nine subcommands for interacting with the entropy system, plus an interactive TUI monitor built with ratatui and crossterm.

**Key dependencies:** `openentropy-core`, `openentropy-server`, `openentropy-tests`, `clap`, `ratatui`, `crossterm`, `tokio`

**Subcommands:** `scan`, `probe`, `bench`, `stream`, `device`, `server`, `monitor`, `report`, `pool`

### 3. openentropy-server

An HTTP entropy server built on axum. Implements an API compatible with the ANU Quantum Random Number Generator format, allowing any QRNG client to consume hardware entropy over HTTP.

**Key dependencies:** `openentropy-core`, `axum`, `tokio`, `serde`, `serde_json`

**Endpoints:** `/api/v1/random`, `/health`, `/sources`, `/pool/status`

### 4. openentropy-tests

A self-contained crate implementing 31 statistical tests inspired by the NIST SP 800-22 randomness test suite. Tests are organized into ten categories: frequency, runs, serial, spectral, entropy, correlation, distribution, pattern, advanced, and practical.

**Key dependencies:** `statrs` (chi-squared, normal, Poisson CDFs), `rustfft` (FFT for spectral tests), `flate2` (compression ratio tests)

### 5. openentropy-python

PyO3 bindings that expose the Rust library to Python. Compiles as a `cdylib` that can be loaded as a native Python extension module. The Python package falls back to a pure-Python implementation if the Rust extension is not available.

**Key dependencies:** `openentropy-core`, `openentropy-tests`, `pyo3`

## Data Flow

```
                         ┌─────────────────────────────────────────────┐
                         │          30 ENTROPY SOURCES                 │
                         │                                             │
                         │  Timing      System      Network   Hardware │
                         │  Silicon     CrossDomain  Novel             │
                         └──────────────────┬──────────────────────────┘
                                            │
                           each: collect(n_samples) -> Vec<u8>
                                            │
                            ┌───────────────┴───────────────┐
                            │  Per-Source Conditioning       │
                            │  (varies by source)            │
                            │                                │
                            │  raw timings                   │
                            │    -> consecutive deltas        │
                            │    -> XOR whitening             │
                            │    -> LSB extraction            │
                            │    -> SHA-256 block hashing     │
                            │    -> Von Neumann debiasing     │
                            └───────────────┬───────────────┘
                                            │
                                            ▼
                              ┌──────────────────────┐
                              │     ENTROPY POOL     │
                              │                      │
                              │  Mutex<Vec<u8>>       │
                              │  buffer              │
                              │                      │
                              │  Health monitoring:  │
                              │  - per-source H rate │
                              │  - failure tracking  │
                              │  - timing stats      │
                              │  - graceful degrade  │
                              └──────────┬───────────┘
                                         │
                                         ▼
                         ┌───────────────────────────────┐
                         │    SHA-256 FINAL CONDITIONING  │
                         │    (NIST SP 800-90B)           │
                         │                                │
                         │  Inputs mixed per 32-byte      │
                         │  output block:                 │
                         │    1. internal state (32 bytes) │
                         │    2. pool buffer (up to 256B)  │
                         │    3. monotonic counter         │
                         │    4. system timestamp (nanos)  │
                         │    5. 8 bytes from /dev/urandom │
                         │                                │
                         │  State is chained: each output │
                         │  updates the internal state    │
                         └───────────┬─────────────────── ┘
                                     │
                 ┌───────────────────┼───────────────────┐
                 │                   │                   │
                 ▼                   ▼                   ▼
         get_random_bytes()     stream/device        HTTP server
           (Rust / Python)      (stdout/FIFO)        (axum)
                 │                                       │
                 ▼                                       ▼
         NumPy Generator                         ANU QRNG API
         EsotericBitGenerator                    /api/v1/random
```

## Key Traits and Types

### `EntropySource` trait

Every entropy source implements this trait. Sources must be `Send + Sync` to support parallel collection.

```rust
pub trait EntropySource: Send + Sync {
    /// Source metadata: name, description, physics, category, platform requirements.
    fn info(&self) -> &SourceInfo;

    /// Check if this source can operate on the current machine.
    fn is_available(&self) -> bool;

    /// Collect raw entropy samples. Returns up to n_samples bytes.
    fn collect(&self, n_samples: usize) -> Vec<u8>;

    /// Convenience: source name from info.
    fn name(&self) -> &'static str { self.info().name }
}
```

### `SourceInfo` struct

Static metadata attached to each source implementation.

```rust
pub struct SourceInfo {
    pub name: &'static str,                        // e.g. "clock_jitter"
    pub description: &'static str,                 // Short human description
    pub physics: &'static str,                     // Detailed physics explanation
    pub category: SourceCategory,                  // Category enum
    pub platform_requirements: &'static [&'static str], // e.g. &["macOS"]
    pub entropy_rate_estimate: f64,                // Estimated bits/second
}
```

### `SourceCategory` enum

Seven categories that classify entropy sources by the physical domain they exploit.

```rust
pub enum SourceCategory {
    Timing,       // Clock phase noise, scheduler jitter
    System,       // Kernel counters, process tables
    Network,      // DNS latency, TCP timing, WiFi RSSI
    Hardware,     // Disk I/O, memory, GPU, audio, camera, sensors
    Silicon,      // DRAM row buffer, cache, page faults, speculative exec
    CrossDomain,  // Beat frequencies between clock domains
    Novel,        // GCD dispatch, dyld timing, VM pages, Spotlight
}
```

## Conditioning Pipeline

The system applies conditioning at two levels.

### Per-Source Conditioning

Each source applies its own conditioning suited to the raw signal:

1. **Timing delta extraction** -- consecutive timestamp differences isolate the jitter component
2. **XOR whitening** -- adjacent deltas are XORed to decorrelate sequential bias
3. **LSB extraction** -- only the lowest byte of each delta is kept (highest entropy density)
4. **SHA-256 block conditioning** -- raw bytes are fed through chained SHA-256 in 64-byte blocks with a monotonic counter to ensure unique output even if raw data repeats
5. **Von Neumann debiasing** (where applicable) -- removes first-order bias by discarding same-bit pairs

### Pool-Level Conditioning

The `EntropyPool::get_random_bytes()` method applies a second conditioning pass:

```
output_block = SHA-256(
    internal_state     ||    // 32 bytes, updated each round
    pool_buffer_chunk  ||    // up to 256 bytes from source buffer
    counter            ||    // monotonic u64, prevents repetition
    timestamp_nanos    ||    // system clock for freshness
    os_random          ||    // 8 bytes from /dev/urandom as safety net
)
```

The output digest becomes the new internal state (chaining), and is appended to the output buffer. This counter-mode construction can produce arbitrary output lengths.

## Parallel Collection

`EntropyPool` supports two collection modes:

- **`collect_all()`** -- serial collection from each source in sequence
- **`collect_all_parallel(timeout_secs)`** -- spawns a scoped thread per source with a deadline; results are merged into the shared buffer under a mutex

Parallel collection uses `std::thread::scope` to safely share references without `Arc` lifetime complexity.

## Thread Safety

`EntropyPool` wraps all mutable state in `Mutex`:
- `sources: Vec<Mutex<SourceState>>` -- per-source state
- `buffer: Mutex<Vec<u8>>` -- raw entropy buffer
- `state: Mutex<[u8; 32]>` -- SHA-256 internal state
- `counter: Mutex<u64>` -- monotonic counter
- `total_output: Mutex<u64>` -- output byte count

Multiple threads can call `get_random_bytes()` concurrently. The pool auto-collects when the buffer runs low.

## Health Monitoring

Each source tracks runtime health via `SourceState`:

```rust
pub struct SourceState {
    pub source: Box<dyn EntropySource>,
    pub weight: f64,           // Collection weight
    pub total_bytes: u64,      // Lifetime bytes collected
    pub failures: u64,         // Collection failure count
    pub last_entropy: f64,     // Shannon entropy of last collection
    pub last_collect_time: Duration,  // Last collection duration
    pub healthy: bool,         // true if last_entropy > 1.0 bits/byte
}
```

Sources that panic during collection are caught via `catch_unwind` and marked unhealthy. The pool continues to operate with remaining healthy sources (graceful degradation).

## Security Model

- **Not a CSPRNG replacement.** This provides entropy *input*, not a complete cryptographic random number generator.
- SHA-256 conditioning ensures output is computationally indistinguishable from random, even if individual sources are weak or compromised.
- Every output block mixes 8 bytes from `/dev/urandom` as a safety net. Even if all 30 hardware sources fail simultaneously, the output remains at least as strong as the OS entropy source.
- Health monitoring detects degraded sources and flags them, but never stops producing output.
- The internal state is chained (each output updates the state), providing forward secrecy: compromising a past state does not reveal future output.

## Build and Toolchain

The workspace uses Rust edition 2024. Key version constraints:

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `sha2` | 0.10 | SHA-256 conditioning |
| `flate2` | 1 | Compression timing oracle + ratio tests |
| `libc` | 0.2 | `mach_absolute_time`, `mmap`, `sysconf` FFI |
| `rand` | 0.9 | Random indices for memory access patterns |
| `clap` | 4 | CLI argument parsing (derive mode) |
| `ratatui` | 0.29 | Terminal UI rendering |
| `crossterm` | 0.28 | Terminal I/O backend |
| `axum` | 0.8 | HTTP server framework |
| `tokio` | 1 | Async runtime for HTTP server |
| `pyo3` | 0.23 | Python native extension bindings |
| `statrs` | 0.18 | Statistical distribution functions |
| `rustfft` | 6 | FFT for spectral tests |

## Python Interop

The Python package (`openentropy`) uses a dual-backend architecture:

```python
try:
    from openentropy.openentropy import EntropyPool  # Rust (PyO3)
    __rust_backend__ = True
except ImportError:
    from openentropy.pool import EntropyPool              # Pure Python
    __rust_backend__ = False
```

When the Rust extension is compiled (via maturin), the Python API delegates to native code. When it is not available, the package falls back to the original pure-Python implementation with identical API surface.
