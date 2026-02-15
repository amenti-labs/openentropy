<div align="center">

# openentropy

**Harvest real entropy from hardware noise. Study it raw or condition it for crypto.**

[![Crates.io](https://img.shields.io/crates/v/openentropy-core.svg)](https://crates.io/crates/openentropy-core)
[![docs.rs](https://docs.rs/openentropy-core/badge.svg)](https://docs.rs/openentropy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=master&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*44 entropy sources from the physics inside your computer — clock jitter, thermal noise, DRAM timing, cache contention, GPU scheduling, IPC latency, and more. Conditioned output for cryptography. Raw output for research.*

**Built for Apple Silicon. No special hardware. No API keys. Just physics.**

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

> By default, only fast sources (<2s) are used. Add `--sources all` to include slower sources (DNS, TCP, GPU, BLE).

### Python

```bash
pip install maturin
git clone https://github.com/amenti-labs/openentropy.git && cd openentropy
maturin develop --release
```

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} entropy sources available")

pool = EntropyPool.auto()
data = pool.get_random_bytes(256)
```

---

## Two Audiences

**Security engineers** use OpenEntropy to seed CSPRNGs, generate keys, and supplement `/dev/urandom` with independent hardware entropy. The SHA-256 conditioned output (`--conditioning sha256`, the default) meets NIST SP 800-90B requirements.

**Researchers** use OpenEntropy to study the raw noise characteristics of hardware subsystems. Pass `--conditioning raw` to get unwhitened, unconditioned bytes that preserve the actual noise signal from each source.

Raw mode enables:
- **Hardware characterization** — measure min-entropy, autocorrelation, and spectral properties of individual noise sources
- **Silicon validation** — compare noise profiles across chip revisions, thermal states, and voltage domains
- **Anomaly detection** — monitor entropy source health for signs of hardware degradation or tampering
- **Cross-domain analysis** — study correlations between independent entropy domains (thermal vs timing vs IPC)

---

## What Makes This Different

Most random number generators are **pseudorandom** — deterministic algorithms seeded once. OpenEntropy continuously harvests **real physical noise** from your hardware:

- **Thermal noise** — denormal FPU micropower, audio PLL drift, power delivery network resonance
- **Timing and microarchitecture** — clock phase noise, DRAM row buffer conflicts, cache contention, speculative execution variance, TLB shootdowns, DVFS races
- **I/O and IPC** — disk and NVMe latency, USB timing, Mach port IPC, pipe buffer allocation, kqueue event multiplexing
- **GPU and compute** — GPU dispatch scheduling, warp divergence, IOSurface cross-domain timing
- **Scheduling and system** — nanosleep drift, GCD dispatch queues, thread lifecycle, kernel counters, process table snapshots
- **Network and sensors** — DNS resolution timing, TCP handshake variance, WiFi RSSI, BLE ambient RF, audio ADC noise
- **Composite beat frequencies** — interference patterns between CPU, memory, and I/O subsystems

The pool XOR-combines independent streams. No single source failure can compromise the pool.

### Conditioning Modes

Conditioning is **optional and configurable**. Use `--conditioning` on the CLI or `?conditioning=` on the HTTP API:

| Mode | Flag | Description |
|------|------|-------------|
| **SHA-256** (default) | `--conditioning sha256` | Full NIST SP 800-90B conditioning. Cryptographic quality output. |
| **Von Neumann** | `--conditioning vonneumann` | Debiasing only — removes bias while preserving more of the raw signal structure. |
| **Raw** | `--conditioning raw` | No processing. Source bytes with zero whitening — preserves the actual hardware noise signal for research. |

Raw mode is what makes OpenEntropy useful for research. Most HWRNG APIs run DRBG post-processing that makes every source look like uniform random bytes, destroying the information researchers need. Raw output preserves per-source noise structure: bias, autocorrelation, spectral features, and cross-source correlations. See [Conditioning](docs/CONDITIONING.md) for details.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [Source Catalog](docs/SOURCES.md) | All 44 entropy sources with physics explanations |
| [Conditioning](docs/CONDITIONING.md) | Raw vs VonNeumann vs SHA-256 conditioning modes |
| [API Reference](docs/API.md) | HTTP server endpoints and response formats |
| [Architecture](docs/ARCHITECTURE.md) | Crate structure and design decisions |
| [Integrations](docs/INTEGRATIONS.md) | Named pipe device, HTTP server, piping to other programs |
| [Python SDK](docs/PYTHON_SDK.md) | PyO3 bindings and NumPy integration |
| [Examples](examples/) | Rust and Python code examples |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Common issues and fixes |
| [Security](SECURITY.md) | Threat model and responsible disclosure |

---

## Entropy Sources

44 sources across 12 mechanism-based categories. Results from `openentropy bench` on Apple Silicon:

### Thermal (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `denormal_timing` | 1.031 | <0.01s | Denormal FPU micropower thermal noise |
| `audio_pll_timing` | 7.795 | 0.08s | Audio PLL clock drift from thermal perturbation |
| `pdn_resonance` | 0.861 | <0.01s | Power delivery network LC resonance noise |

### Timing (7)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `clock_jitter` | 6.507 | 0.00s | Phase noise between performance counter and monotonic clocks |
| `mach_timing` | 7.832 | 0.00s | Mach absolute time LSB jitter |
| `memory_timing` | 5.056 | 0.01s | DRAM access timing variations |
| `dram_row_buffer` | 7.959 | 0.00s | DRAM row buffer conflict timing |
| `cache_contention` | 7.960 | 0.01s | CPU cache line contention noise |
| `page_fault_timing` | 7.967 | 0.01s | Virtual memory page fault latency |
| `vm_page_timing` | 7.963 | 0.07s | Mach VM page allocation timing |

### Scheduling (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `sleep_jitter` | 7.963 | 0.00s | Scheduling jitter in nanosleep() calls |
| `dispatch_queue` | 6.688 | 0.09s | GCD dispatch queue scheduling jitter |
| `thread_lifecycle` | 6.788 | 0.08s | pthread create/join cycle timing |

### IO (4)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `disk_io` | 7.960 | 0.02s | Block device I/O timing jitter |
| `nvme_latency` | 5.321 | 0.01s | NVMe command submission/completion timing |
| `usb_timing` | 7.734 | 0.03s | USB bus transaction timing jitter |
| `fsync_journal` | 7.796 | 16.69s | fsync journal commit latency noise |

### IPC (4)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `mach_ipc` | 4.924 | 0.04s | Mach port IPC allocation/deallocation timing |
| `pipe_buffer` | 3.220 | 0.01s | Kernel zone allocator via pipe lifecycle |
| `kqueue_events` | 7.489 | 12.25s | BSD kqueue event multiplexing timer/file/socket jitter |
| `keychain_timing` | 7.543 | 0.02s | macOS Keychain Services API timing jitter |

### Microarch (5)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `speculative_execution` | 7.967 | 0.00s | Branch prediction / speculative execution jitter |
| `dvfs_race` | 7.804 | 0.13s | Cross-core DVFS frequency race (H∞=7.288) |
| `cas_contention` | 2.352 | <0.01s | Multi-thread atomic CAS arbitration contention |
| `tlb_shootdown` | 6.456 | 0.03s | mprotect() TLB invalidation IPI latency |
| `amx_timing` | 5.188 | 0.05s | Apple AMX coprocessor matrix dispatch jitter |

### GPU (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `gpu_timing` | 7.966 | 46.96s | GPU compute dispatch scheduling jitter |
| `gpu_divergence` | 7.837 | 0.76s | GPU warp divergence timing variance |
| `iosurface_crossing` | 5.048 | 0.08s | IOSurface CPU-GPU cross-domain timing |

### Network (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `dns_timing` | 7.958 | 21.91s | DNS resolution timing jitter |
| `tcp_connect_timing` | 7.967 | 39.08s | TCP handshake timing variance |
| `wifi_rssi` | — | — | WiFi received signal strength fluctuations *(requires WiFi)* |

### System (4)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `sysctl_deltas` | 7.968 | 0.28s | Kernel counter fluctuations across 50+ sysctl keys |
| `vmstat_deltas` | 7.965 | 0.38s | VM subsystem page fault and swap counters |
| `process_table` | 7.971 | 1.99s | Process table snapshot entropy |
| `ioregistry` | 7.964 | 2.15s | IOKit registry value mining |

### Composite (2)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `cpu_io_beat` | 6.707 | 0.04s | CPU and I/O subsystem beat frequency |
| `cpu_memory_beat` | 6.256 | 0.00s | CPU and memory controller beat pattern |

### Signal (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `compression_timing` | 7.966 | 1.02s | zlib compression timing oracle |
| `hash_timing` | 7.122 | 0.04s | SHA-256 hash timing data-dependency |
| `spotlight_timing` | 7.969 | 12.91s | Spotlight metadata query timing |

### Sensor (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `audio_noise` | — | — | Audio ADC thermal noise floor *(requires mic)* |
| `camera_noise` | — | — | Image sensor dark current noise *(requires camera)* |
| `bluetooth_noise` | 7.961 | 10.01s | BLE ambient RF noise |

Shannon entropy is measured 0–8 bits per byte. Sources scoring ≥ 7.9 are grade A. See the [Source Catalog](docs/SOURCES.md) for physics details on each source.

---

## CLI Reference

### `scan` — Discover sources

```bash
openentropy scan
```

### `bench` — Benchmark sources

```bash
openentropy bench                    # fast sources (~1s)
openentropy bench --sources all      # all sources
openentropy bench --sources silicon  # filter by name
```

### `stream` — Continuous output

```bash
openentropy stream --format hex --bytes 256
openentropy stream --format raw --bytes 1024 | your-program
openentropy stream --format base64 --rate 1024           # rate-limited
openentropy stream --conditioning raw --format raw       # no conditioning
openentropy stream --conditioning vonneumann --format hex # debiased only
openentropy stream --conditioning sha256 --format hex    # full conditioning (default)
```

### `monitor` — Interactive TUI dashboard

```bash
openentropy monitor
```

| Key | Action |
|-----|--------|
| ↑/↓ | Navigate source list |
| Space | Select source (starts collecting) |
| r | Force refresh |
| q | Quit |

### `probe` — Test a single source

```bash
openentropy probe mach_timing
```

### `pool` — Pool health metrics

```bash
openentropy pool
```

### `device` — Named pipe (FIFO)

```bash
openentropy device /tmp/openentropy-rng
# Another terminal: head -c 32 /tmp/openentropy-rng | xxd
```

### `server` — HTTP entropy server

```bash
openentropy server --port 8080
openentropy server --port 8080 --allow-raw    # enable raw output
```

```bash
curl "http://localhost:8080/api/v1/random?length=256&type=uint8"
curl "http://localhost:8080/health"
```

### `entropy` — Deep min-entropy analysis

```bash
openentropy entropy
openentropy entropy --sources mach_timing
```

### `report` — NIST test battery

```bash
openentropy report
openentropy report --source mach_timing --samples 50000
```

---

## Rust API

```toml
[dependencies]
openentropy-core = "0.4"
```

```rust
use openentropy_core::{EntropyPool, detect_available_sources};

let pool = EntropyPool::auto();
let bytes = pool.get_random_bytes(256);
let health = pool.health_report();
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

```
Sources (44) → raw samples → Entropy Pool (XOR combine) → Conditioning (optional) → Output
                                                                 │                       ├── Rust API
                                                           ┌─────┴─────┐                ├── CLI / TUI
                                                           │ sha256    │ (default)       ├── HTTP Server
                                                           │ vonneumann│                 ├── Named Pipe
                                                           │ raw       │ (passthrough)   └── Python SDK
                                                           └───────────┘
```

---

## Platform Support

| Platform | Sources | Notes |
|----------|:-------:|-------|
| **MacBook (M-series)** | **44/44** | Full suite — WiFi, BLE, camera, mic |
| **Mac Mini / Studio / Pro** | 39–41 | No built-in camera, mic on some models |
| **Intel Mac** | ~20 | Some silicon/microarch sources are ARM-specific |
| **Linux** | 10–15 | Timing, network, disk, process sources |

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

## License

MIT — Copyright © 2026 [Amenti Labs](https://github.com/amenti-labs)
