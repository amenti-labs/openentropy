<div align="center">

<img src="assets/logo_with_text.png" alt="openentropy logo" width="320">

# openentropy

**Measure and operationalize hardware entropy for research and security.**

[![Crates.io](https://img.shields.io/crates/v/openentropy-core.svg)](https://crates.io/crates/openentropy-core)
[![docs.rs](https://docs.rs/openentropy-core/badge.svg)](https://docs.rs/openentropy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=master&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*Collect entropy from on-device hardware sources, analyze source quality with statistical tests, and deliver raw or conditioned output through CLI, Rust, Python, and HTTP interfaces.*

**One workflow for entropy research, validation, and cryptographic deployment.**

**By [Amenti Labs](https://github.com/amenti-labs)**

</div>

---

## Quick Start

```bash
# Install
cargo install openentropy-cli

# Discover entropy sources on your machine
openentropy scan

# Benchmark all fast sources
openentropy bench

# Output 64 random hex bytes
openentropy stream --format hex --bytes 64

# Live TUI dashboard
openentropy monitor
```

> By default, only fast sources (<2s) are used. Pass `all` as a positional argument to include slower sources (DNS, TCP, GPU, BLE).

### Python

```bash
pip install openentropy
```

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} entropy sources available")

pool = EntropyPool.auto()
source = sources[0]["name"]
data = pool.get_source_bytes(source, 256, conditioning="sha256")
```

Build from source (native extension):

```bash
git clone https://github.com/amenti-labs/openentropy.git && cd openentropy
pip install maturin
maturin develop
```

---

## Two Audiences

**Security engineers** use OpenEntropy to validate entropy quality and seed CSPRNGs:
```bash
openentropy analyze --profile security --output audit.md
```

**Researchers** use OpenEntropy to study raw noise characteristics:
```bash
openentropy analyze --profile deep --output analysis.json
```

Security engineers seed CSPRNGs, generate keys, and supplement `/dev/urandom` with independent hardware entropy. The SHA-256 conditioned output (`--conditioning sha256`, the default) produces cryptographic-quality random bytes. The `security` profile enables the NIST test battery, min-entropy breakdown, and SHA-256 conditioning in one flag.

Researchers study the raw noise characteristics of hardware subsystems. Pass `--conditioning raw` to get unwhitened, unconditioned bytes that preserve the actual noise signal from each source. The `deep` profile enables 100K samples, cross-correlation, and PEAR-style trial analysis.

Raw mode enables:
- **Hardware characterization** — measure min-entropy, autocorrelation, and spectral properties of individual noise sources
- **Silicon validation** — compare noise profiles across chip revisions, thermal states, and voltage domains
- **Anomaly detection** — monitor entropy source health for signs of hardware degradation or tampering
- **Cross-domain analysis** — study correlations between independent entropy domains (thermal vs timing vs IPC)

---

## What Makes This Different

Most random number generators are **pseudorandom** — deterministic algorithms seeded once. OpenEntropy continuously harvests **real physical noise** from your hardware:

- **Thermal noise** — three independent oscillator beats (CPU crystal vs audio PLL, display PLL, PCIe PHY PLLs)
- **Timing and microarchitecture** — clock phase noise, DRAM row buffer conflicts, speculative execution variance, TLB shootdowns, DVFS races, ICC bus contention, prefetcher state, APRR JIT timing, ANE clock domain crossing
- **I/O and IPC** — disk and NVMe latency (including IOKit sensor polling, raw device, and Linux passthrough), USB enumeration, Mach port IPC, pipe buffer allocation, kqueue events, fsync journal
- **GPU and compute** — GPU warp divergence, IOSurface cross-domain timing, Neural Engine inference timing
- **Scheduling and system** — nanosleep drift, GCD dispatch queues, thread lifecycle, P/E-core migration, timer coalescing, kernel counters, process table snapshots
- **Network and sensors** — DNS resolution timing, TCP handshake variance, WiFi RSSI, BLE ambient RF, audio ADC noise
- **Deep hardware** — dual clock domain beats, SITVA, AES-XTS context switching, SEV broadcast, COMMPAGE seqlock, SMC thermistor, getentropy TRNG reseed

The pool XOR-combines independent streams. No single source failure can compromise the pool.

### Conditioning Modes

Conditioning is **optional and configurable**. Use `--conditioning` on the CLI or `?conditioning=` on the HTTP API:

| Mode | Flag | Description |
|------|------|-------------|
| **SHA-256** (default) | `--conditioning sha256` | SHA-256 conditioning. Cryptographic quality output. |
| **Von Neumann** | `--conditioning vonneumann` | Debiasing only — removes bias while preserving more of the raw signal structure. |
| **Raw** | `--conditioning raw` | No processing. Source bytes with zero whitening — preserves the actual hardware noise signal for research. |

Raw mode is what makes OpenEntropy useful for research. Most HWRNG APIs run DRBG post-processing that makes every source look like uniform random bytes, destroying the information researchers need. Raw output preserves per-source noise structure: bias, autocorrelation, spectral features, and cross-source correlations. See [Conditioning](https://amenti-labs.github.io/openentropy/concepts/conditioning/) for details.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [Source Catalog](https://amenti-labs.github.io/openentropy/concepts/sources/) | All 63 entropy sources with physics explanations |
| [CLI Reference](https://amenti-labs.github.io/openentropy/cli/reference/) | Full command reference and examples |
| [Conditioning](https://amenti-labs.github.io/openentropy/concepts/conditioning/) | Raw vs VonNeumann vs SHA-256 conditioning modes |
| [Trial Analysis Methodology](https://amenti-labs.github.io/openentropy/concepts/trials/) | PEAR-style 200-bit trials, calibration gate, and references |
| [Telemetry Model](https://amenti-labs.github.io/openentropy/concepts/telemetry/) | Experimental telemetry_v1 context model and integration points |
| [Rust API Reference](https://amenti-labs.github.io/openentropy/rust-sdk/api/) | Core Rust API surface |
| [Architecture](https://amenti-labs.github.io/openentropy/concepts/architecture/) | Crate structure and design decisions |
| [Python SDK](https://amenti-labs.github.io/openentropy/python-sdk/reference/) | PyO3 bindings and Python API reference |
| [Examples](examples/) | Rust and Python code examples |
| [Troubleshooting](https://amenti-labs.github.io/openentropy/guides/troubleshooting/) | Common issues and fixes |
| [Security](SECURITY.md) | Threat model and responsible disclosure |

---

## Entropy Sources

63 sources across 13 mechanism-based categories:

| Category | Count |
|----------|:-----:|
| Thermal | 4 |
| Timing | 7 |
| Scheduling | 6 |
| IO | 6 |
| IPC | 4 |
| Microarchitecture | 16 |
| GPU | 3 |
| Network | 3 |
| System | 6 |
| Signal | 3 |
| Sensor | 4 |
| Quantum | 1 |

For full per-source descriptions, platform availability, and physics notes, see
[Source Catalog](https://amenti-labs.github.io/openentropy/concepts/sources/).

---

## CLI Reference

For the full command reference and examples, see [CLI Reference](https://amenti-labs.github.io/openentropy/cli/reference/).

Most-used workflows:

```bash
openentropy scan
openentropy bench
openentropy stream --format hex --bytes 64
openentropy analyze --profile security         # NIST battery + entropy + sha256
openentropy analyze --profile deep             # 100K + forensic + cross-corr + trials
openentropy record clock_jitter --duration 30s
openentropy sessions sessions/<id> --profile deep
openentropy compare sessions/<id-a> sessions/<id-b> --profile deep
```

On `sessions`, profile presets apply only when a specific session path is
provided. `openentropy sessions` with no path always stays in list mode.

PEAR-style trial methodology references (200-bit trials, terminal Z, weighted
Stouffer composition, calibration gating) are documented in
[Trial Analysis Methodology](https://amenti-labs.github.io/openentropy/concepts/trials/).

---

## Rust API

```toml
[dependencies]
openentropy-core = "0.10"
```

```rust
use openentropy_core::{ConditioningMode, EntropyPool, detect_available_sources};

let sources = detect_available_sources();
println!("{} sources available", sources.len());

let pool = EntropyPool::auto();
let source = pool.source_names()[0].clone();
let bytes = pool
    .get_source_bytes(&source, 256, ConditioningMode::Sha256)
    .unwrap();
let health = pool.health_report();
```

Analyze and compare entropy data programmatically:

```rust
use openentropy_core::{full_analysis, compare, trial_analysis};

let data = pool.get_source_raw_bytes(&source, 5000).unwrap();

// Per-source statistical analysis
let analysis = full_analysis(&source, &data);
println!("Shannon entropy: {:.4} bits/byte", analysis.shannon_entropy);

// Differential comparison of two streams
let other = pool.get_source_raw_bytes(&source, 5000).unwrap();
let diff = compare("stream_a", &data, "stream_b", &other);

// PEAR-style trial analysis
let trials = trial_analysis(&data, &Default::default());
println!("Terminal Z: {:.4}, p = {:.4}", trials.terminal_z, trials.terminal_p_value);
```

Chaos theory analysis (distinguish true randomness from deterministic chaos):

```rust
use openentropy_core::chaos::chaos_analysis;

let result = chaos_analysis(&data);
println!("Hurst H={:.4}, Lyapunov λ={:.4}, D₂={:.4}",
    result.hurst.hurst_exponent,
    result.lyapunov.lyapunov_exponent,
    result.correlation_dimension.dimension);
```

---

## Architecture

Cargo workspace with 6 crates:

| Crate | Description |
|-------|-------------|
| `openentropy-core` | Core library — sources, pool, conditioning |
| `openentropy-cli` | CLI binary with TUI dashboard |
| `openentropy-server` | Axum HTTP entropy server |
| `openentropy-tests` | NIST SP 800-22 inspired test battery |
| `openentropy-python` | Python bindings via PyO3/maturin |
| `openentropy-wasm` | WebAssembly/browser entropy crate |

```mermaid
flowchart TD
    Sources[Sources 63] --> Raw[raw samples]
    Raw --> Pool[Entropy Pool XOR combine]
    Pool --> Modes{Conditioning mode}
    Modes -->|default| Sha[sha256]
    Modes --> Vn[vonneumann]
    Modes -->|passthrough| RawMode[raw]
    Sha --> Output[Output surfaces]
    Vn --> Output
    RawMode --> Output
    Output --> Rust[Rust API]
    Output --> CLI[CLI and TUI]
    Output --> Http[HTTP Server]
    Output --> Pipe[Named Pipe]
    Output --> Py[Python SDK]
```

---

## Platform Support

| Platform | Sources | Notes |
|----------|:-------:|-------|
| **MacBook (M-series)** | **63/63** | Full suite — WiFi, BLE, camera, mic |
| **Mac Mini / Studio / Pro** | 50–55 | No built-in camera, mic on some models |
| **Intel Mac** | ~20 | Some silicon/microarch sources are ARM-specific |
| **Linux** | 12–15 | Timing, network, disk, process sources + NVMe passthrough |

The library detects available hardware at runtime and only activates working sources.

---

## Building from Source

Requires Rust 1.85+ and macOS or Linux.

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
cargo build --release --workspace --exclude openentropy-python
cargo test --workspace --exclude openentropy-python
cargo install --path crates/openentropy-cli
```

### Python package

```bash
pip install maturin
maturin develop --release
python3 -c "from openentropy import EntropyPool; print(EntropyPool.auto().get_random_bytes(16).hex())"
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Ideas:

- New entropy sources (especially Linux-specific)
- Performance improvements
- Additional NIST test implementations
- Windows platform support

---

## References

- NIST SP 800-22: [A Statistical Test Suite for Random and Pseudorandom Number Generators](https://csrc.nist.gov/publications/detail/sp/800-22/rev-1a/final)
- Bone, A. (2026): [QRNG Analysis Toolkit](https://github.com/vikingdude81/qrng-analysis-toolkit) — chaos theory analysis methods (Hurst exponent, Lyapunov exponent, correlation dimension, BiEntropy, Epiplexity) were reimplemented in Rust from mathematical definitions, inspired by this Python toolkit
- Croll, G.J. (2013): [BiEntropy — The Approximate Entropy of a Finite Binary String](https://arxiv.org/abs/1305.0954)
- Hurst, H.E. (1951): Long-term storage capacity of reservoirs
- Rosenstein, M.T., Collins, J.J. & De Luca, C.J. (1993): A practical method for calculating largest Lyapunov exponents from small data sets
- Grassberger, P. & Procaccia, I. (1983): Measuring the strangeness of strange attractors
- Shannon, C.E. (1948): A Mathematical Theory of Communication

---


## License

MIT — Copyright © 2026 [Amenti Labs](https://github.com/amenti-labs)
