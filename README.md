<div align="center">

# openentropy

**Your computer is a hardware noise observatory.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=main&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*Harvest real entropy from 30 hardware sources hiding inside your computer — clock jitter, kernel counters, DRAM row buffers, cache contention, and more.*

**Built for Apple Silicon. No special hardware. No API keys. Just physics.**

**By [Amenti Labs](https://github.com/amenti-labs)**

</div>

---

## Quick Start

```bash
# Install
cargo install --git https://github.com/amenti-labs/openentropy openentropy-cli

# Discover entropy sources on your machine
openentropy scan

# Benchmark all fast sources (~1s)
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

## What Makes This Different

Most random number generators are **pseudorandom** — deterministic algorithms seeded once. OpenEntropy continuously harvests **real physical noise** from your hardware:

- **Timing jitter** — clock phase noise, scheduling nondeterminism, nanosleep drift
- **Silicon microarchitecture** — DRAM row buffer conflicts, CPU cache contention, speculative execution variance, page fault latency
- **Thermal fluctuations** — sensor readouts, GPU dispatch scheduling, disk I/O latency
- **Network nondeterminism** — DNS resolution timing, TCP handshake variance
- **Cross-domain beat frequencies** — interference patterns between CPU, memory, and I/O subsystems

The pool XOR-combines independent streams. No single source failure can compromise the pool.

### Conditioning Modes

Conditioning is **optional and configurable**. Use `--conditioning` on the CLI or `?conditioning=` on the HTTP API:

| Mode | Flag | Description |
|------|------|-------------|
| **SHA-256** (default) | `--conditioning sha256` | Full NIST SP 800-90B conditioning. Cryptographic quality output. |
| **Von Neumann** | `--conditioning vonneumann` | Debiasing only — removes bias while preserving more of the raw signal structure. |
| **Raw** | `--conditioning raw` | No processing. XOR-combined source bytes with zero whitening. |

Most hardware RNG APIs apply DRBG post-processing that destroys the raw noise signal. OpenEntropy preserves it — pass `--conditioning raw` for unwhitened bytes, ideal for researchers studying actual hardware noise characteristics. See [Conditioning](docs/CONDITIONING.md) for details.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [Source Catalog](docs/SOURCE_CATALOG.md) | All 30 entropy sources with physics explanations |
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

30 sources across 7 categories. Results from `openentropy bench` on Apple Silicon:

### Timing (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `clock_jitter` | 6.507 | 0.00s | Phase noise between performance counter and monotonic clocks |
| `mach_timing` | 7.832 | 0.00s | Mach absolute time LSB jitter |
| `sleep_jitter` | 7.963 | 0.00s | Scheduling jitter in nanosleep() calls |

### System (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `sysctl_deltas` | 7.968 | 0.28s | Kernel counter fluctuations across 50+ sysctl keys |
| `vmstat_deltas` | 7.965 | 0.38s | VM subsystem page fault and swap counters |
| `process_table` | 7.971 | 1.99s | Process table snapshot entropy |

### Network (2)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `dns_timing` | 7.958 | 21.91s | DNS resolution timing jitter |
| `tcp_connect_timing` | 7.967 | 39.08s | TCP handshake timing variance |

### Hardware (6)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `disk_io` | 7.960 | 0.02s | Block device I/O timing jitter |
| `memory_timing` | 5.056 | 0.01s | DRAM access timing variations |
| `gpu_timing` | 7.966 | 46.96s | GPU compute dispatch scheduling jitter |
| `sensor_noise` | 7.997 | 0.97s | SMC sensor readout jitter |
| `bluetooth_noise` | 7.961 | 10.01s | BLE ambient RF noise |
| `ioregistry` | 7.964 | 2.15s | IOKit registry value mining |

> `wifi_rssi`, `audio_noise`, and `camera_noise` are available on machines with the corresponding hardware.

### Silicon Microarchitecture (4)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `dram_row_buffer` | 7.959 | 0.00s | DRAM row buffer conflict timing |
| `cache_contention` | 7.960 | 0.01s | CPU cache line contention noise |
| `page_fault_timing` | 7.967 | 0.01s | Virtual memory page fault latency |
| `speculative_execution` | 7.967 | 0.00s | Branch prediction / speculative execution jitter |

### Cross-Domain Beat Frequencies (3)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `cpu_io_beat` | 6.707 | 0.04s | CPU and I/O subsystem beat frequency |
| `cpu_memory_beat` | 6.256 | 0.00s | CPU and memory controller beat pattern |
| `multi_domain_beat` | 3.867 | 0.00s | Multi-subsystem interference pattern |

### Novel (6)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `compression_timing` | 7.966 | 1.02s | zlib compression timing oracle |
| `hash_timing` | 7.122 | 0.04s | SHA-256 hash timing data-dependency |
| `dispatch_queue` | 6.688 | 0.09s | GCD dispatch queue scheduling jitter |
| `dyld_timing` | 7.967 | 1.35s | Dynamic linker dlsym() timing |
| `vm_page_timing` | 7.963 | 0.07s | Mach VM page allocation timing |
| `spotlight_timing` | 7.969 | 12.91s | Spotlight metadata query timing |

Shannon entropy is measured 0–8 bits per byte. Sources scoring ≥ 7.9 are grade A. See the [Source Catalog](docs/SOURCE_CATALOG.md) for physics details on each source.

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

### `report` — NIST test battery

```bash
openentropy report
openentropy report --source mach_timing --samples 50000
```

---

## Rust API

```toml
[dependencies]
openentropy-core = "0.3"
```

```rust
use openentropy_core::{EntropyPool, detect_available_sources};

let pool = EntropyPool::auto();
let bytes = pool.get_random_bytes(256);
let health = pool.health_report();
```

---

## Architecture

Cargo workspace with 5 crates:

| Crate | Description |
|-------|-------------|
| `openentropy-core` | Core library — sources, pool, conditioning |
| `openentropy-cli` | CLI binary with TUI dashboard |
| `openentropy-server` | Axum HTTP entropy server |
| `openentropy-tests` | NIST SP 800-22 inspired test battery |
| `openentropy-python` | Python bindings via PyO3/maturin |

```
Sources (30) → raw samples → Entropy Pool (XOR combine) → Conditioning (optional) → Output
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
| **MacBook (M-series)** | **30/30** | Full suite — WiFi, BLE, camera, mic, sensors |
| **Mac Mini / Studio / Pro** | 27–28 | No built-in camera, mic, or motion sensors |
| **Intel Mac** | ~20 | Some silicon sources are ARM-specific |
| **Linux** | 10–15 | Timing, network, disk, process sources |

The library detects available hardware at runtime and only activates working sources.

---

## Building from Source

Requires Rust 1.85+ and macOS or Linux.

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
cargo build --release
cargo test --workspace
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
