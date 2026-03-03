---
title: 'Architecture'
description: 'Crate structure and design decisions'
---

## Overview

openentropy is a multi-source entropy harvesting system written in Rust. It treats every computer as a collection of noisy analog subsystems and extracts randomness from their unpredictable physical behavior. The project is structured as a Cargo workspace with multiple crates, each with a focused responsibility.

**Version:** 0.11.0
**Edition:** Rust 2024
**License:** MIT

## Workspace Layout

```mermaid
flowchart TD
    Root[openentropy workspace]
    Cargo[Cargo.toml]
    PyProject[pyproject.toml]
    Examples[examples/ Rust + Python]
    PyPkg[openentropy/ Python package wrapper]

    Root --> Cargo
    Root --> Crates[crates/]
    Root --> PyProject
    Root --> PyPkg
    Root --> Examples

    Crates --> Core[openentropy-core]
    Crates --> CLI[openentropy-cli]
    Crates --> Server[openentropy-server]
    Crates --> Tests[openentropy-tests]
    Crates --> Python[openentropy-python]
    Crates --> Wasm[openentropy-wasm]

    Core --> CoreAPI[lib.rs public API]
    Core --> CorePool[pool.rs + conditioning.rs]
    Core --> CoreAnalysis[analysis.rs + comparison.rs + trials.rs + dispatcher.rs]
    Core --> CoreSources[sources/ 63 implementations across 13 categories]

    CLI --> CLIMain[main.rs clap entrypoint]
    CLI --> CLICommands[commands/ scan bench analyze stream monitor record sessions compare server]
    CLI --> CLITui[tui/ app + ui]

    Server --> ServerLib[lib.rs axum router]
    Tests --> TestsLib[lib.rs NIST-inspired statistical battery]
    Python --> PyBindings[lib.rs + *_bindings.rs via PyO3]
    Wasm --> WasmLib[lib.rs wasm-bindgen exports]
```

## Core Crates

### 1. openentropy-core

The foundational library. Contains all 63 entropy source implementations, the mixing pool, conditioning pipeline, quality metrics, and platform detection.

**Key dependencies:** `sha2`, `flate2`, `libc`, `rand`, `tempfile`, `log`, `getrandom`

**Public API:**
- `EntropyPool` -- thread-safe multi-source collector with SHA-256 conditioning
- `EntropySource` trait -- interface every source must implement
- `SourceInfo`, `SourceCategory` -- metadata types
- `detect_available_sources()` -- auto-discovery
- `quick_shannon()`, `quick_quality()` -- quality assessment functions
- `full_analysis()`, `cross_correlation_matrix()` -- statistical analysis
- `comparison::compare()`, `comparison::compare_with_analysis()` -- differential session comparison
- `trial_analysis()`, `stouffer_combine()`, `calibration_check()` -- PEAR-style trial analysis

### 2. openentropy-cli

The command-line binary (`openentropy`). Provides nine subcommands for interacting with the entropy system, plus an interactive TUI monitor built with ratatui and crossterm.

**Key dependencies:** `openentropy-core`, `openentropy-server`, `openentropy-tests`, `clap`, `ratatui`, `crossterm`, `tokio`

**Subcommands:** `scan`, `bench`, `analyze`, `record`, `sessions`, `compare`, `monitor`, `stream`, `server`

### 3. openentropy-server

An HTTP entropy server built on axum. Implements an API compatible with the ANU QRNG format, allowing any QRNG client to consume hardware entropy over HTTP.

**Key dependencies:** `openentropy-core`, `axum`, `tokio`, `serde`, `serde_json`

**Endpoints:** `/api/v1/random`, `/health`, `/sources`, `/pool/status`

### 4. openentropy-tests

A self-contained crate implementing 31 statistical tests inspired by the NIST SP 800-22 randomness test suite. Tests are organized into ten categories: frequency, runs, serial, spectral, entropy, correlation, distribution, pattern, advanced, and practical.

**Key dependencies:** `statrs` (chi-squared, normal, Poisson CDFs), `rustfft` (FFT for spectral tests), `flate2` (compression ratio tests)

### 5. openentropy-python

PyO3 bindings that expose the Rust library to Python. Compiles as a `cdylib` that is loaded as a native Python extension module.

**Key dependencies:** `openentropy-core`, `openentropy-tests`, `pyo3`, `pythonize`

### 6. openentropy-wasm

WebAssembly bindings that expose selected entropy functionality for browser environments.

**Key dependencies:** `openentropy-core`, `wasm-bindgen`, `js-sys`

## Data Flow

```mermaid
flowchart TD
    Sources[63 entropy sources across timing system network IO IPC microarch GPU sensor thermal quantum signal]
    Collect[each source collect(n_samples) -> Vec<u8>]
    Pool[EntropyPool mutex buffer with health monitoring]
    Conditioning[SHA-256 final conditioning per 32-byte block]
    Inputs[Mixes internal_state pool_chunk counter timestamp os_random]

    Sources --> Collect --> Pool --> Conditioning
    Inputs --> Conditioning

    Conditioning --> RustOut[get_random_bytes() in Rust]
    Conditioning --> StreamOut[CLI stream and FIFO output]
    Conditioning --> HttpOut[HTTP server endpoints]
    RustOut --> PyOut[Python bindings via PyO3]
    HttpOut --> AnuCompat[ANU QRNG-compatible /api/v1/random]
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
    pub platform: Platform,                       // e.g. Platform::MacOS
    pub requirements: &'static [Requirement],     // e.g. &[Requirement::Wifi]
    pub composite: bool,                          // Whether source combines domains
    pub entropy_rate_estimate: f64,                // Estimated bits/byte (max 8.0)
    pub is_fast: bool,                            // true if collection < 2s
}
```

### `SourceCategory` enum

Thirteen categories classify entropy sources by physical mechanism.

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

## Conditioning Pipeline

Conditioning is centralized in `crates/openentropy-core/src/conditioning.rs`.

The pool can return:

- `Raw` bytes (`get_raw_bytes`)
- `VonNeumann` debiased bytes (`get_bytes(..., ConditioningMode::VonNeumann)`)
- `Sha256` conditioned bytes (`get_random_bytes`, default path)

The SHA-256 path used by `EntropyPool::get_random_bytes()` mixes:

```text
output_block = SHA-256(
    internal_state     ||    // 32 bytes, updated each round
    pool_buffer_chunk  ||    // up to 256 bytes from source buffer
    counter            ||    // monotonic u64, prevents repetition
    timestamp_nanos    ||    // system clock for freshness
    os_random          ||    // 8 bytes from /dev/urandom as safety net
)
```

The output digest is appended to the output buffer. The new internal state is derived separately: `SHA-256(output || "openentropy_state")`, ensuring that knowing the output does not reveal the internal state (forward secrecy). This counter-mode construction can produce arbitrary output lengths.

## Parallel Collection

`EntropyPool` supports timeout-bounded parallel collection and backoff for slow/hung sources:

- `collect_all()`
- `collect_all_parallel(timeout_secs)`

Collection workers run in detached threads with in-flight tracking and per-source backoff windows to prevent thread buildup.

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
- Every output block mixes 8 bytes from OS CSPRNG as a safety net. Even if hardware sources fail, output remains at least as strong as the OS entropy source.
- Health monitoring detects degraded sources and flags them, but never stops producing output.
- The internal state is derived separately from the output (`SHA-256(output || domain_separator)`), providing forward secrecy: observing output does not reveal the internal state.

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

The Python package (`openentropy`) imports symbols from the compiled extension module:

```python
from openentropy.openentropy import EntropyPool
```

See [Python SDK](/openentropy/python-sdk/) for the current Python API surface.
