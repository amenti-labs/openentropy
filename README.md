<div align="center">

<img src="assets/logo.png" alt="openentropy logo" width="200">

# openentropy

**Harvest real entropy from hardware noise. Study it raw or condition it for crypto.**

[![Crates.io](https://img.shields.io/crates/v/openentropy-core.svg)](https://crates.io/crates/openentropy-core)
[![docs.rs](https://docs.rs/openentropy-core/badge.svg)](https://docs.rs/openentropy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=master&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*47 entropy sources from the physics inside your computer — clock jitter, thermal noise, DRAM timing, cache contention, GPU scheduling, IPC latency, and more. Conditioned output for cryptography. Raw output for research.*

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

# Capture a 3-second telemetry window
openentropy telemetry --window-sec 3

# Output 64 random hex bytes
openentropy stream --format hex --bytes 64

# Live TUI dashboard
openentropy monitor
```

> By default, only fast sources (<2s) are used. Add `--sources all` to include slower sources (DNS, TCP, GPU, BLE).

### Python

```bash
pip install openentropy
```

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} entropy sources available")

pool = EntropyPool.auto()
data = pool.get_random_bytes(256)
```

Build from source (native extension):

```bash
git clone https://github.com/amenti-labs/openentropy.git && cd openentropy
pip install maturin
maturin develop
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

- **Thermal noise** — four independent oscillator beats (CPU crystal vs audio PLL, display PLL, PCIe PHY PLLs), denormal FPU micropower, power delivery network resonance
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
| [Source Catalog](docs/SOURCES.md) | All 47 entropy sources with physics explanations |
| [Conditioning](docs/CONDITIONING.md) | Raw vs VonNeumann vs SHA-256 conditioning modes |
| [Quantum Benchmark](docs/QUANTUM_BENCHMARK.md) | Detailed methodology for the experimental quantum:classical proxy |
| [Telemetry Model](docs/TELEMETRY.md) | Experimental telemetry_v1 context model and integration points |
| [API Reference](docs/API.md) | HTTP server endpoints and response formats |
| [Architecture](docs/ARCHITECTURE.md) | Crate structure and design decisions |
| [Integrations](docs/INTEGRATIONS.md) | Named pipe device, HTTP server, piping to other programs |
| [Python SDK](docs/PYTHON_SDK.md) | PyO3 bindings and Python API reference |
| [Examples](examples/) | Rust and Python code examples |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Common issues and fixes |
| [Security](SECURITY.md) | Threat model and responsible disclosure |

---

## Entropy Sources

47 sources across 12 mechanism-based categories. Results from `openentropy bench` on Apple Silicon:

### Thermal (6)

Each source taps a **physically independent** noise mechanism. The oscillator sources are especially noteworthy: they beat the CPU's 24 MHz crystal against other independent oscillators on the SoC, capturing uncorrelated Johnson-Nyquist thermal noise from separate crystal sustaining amplifiers or PLL VCO transistors.

| Source | Time | Description |
|--------|-----:|-------------|
| `denormal_timing` | <0.01s | Denormal FPU micropower thermal noise |
| `audio_pll_timing` | 0.08s | Audio PLL clock drift from thermal perturbation |
| `pdn_resonance` | <0.01s | Power delivery network LC resonance noise |
| `counter_beat` | 0.09s | Two-oscillator beat: CPU crystal (24 MHz) vs audio PLL crystal |
| `display_pll` | 0.07s | Display PLL phase noise from pixel clock (~533 MHz) domain crossing |
| `pcie_pll` | 0.10s | PCIe PHY PLL jitter from Thunderbolt/PCIe clock domain crossing |

### Timing (7)

| Source | Time | Description |
|--------|-----:|-------------|
| `clock_jitter` | 0.00s | Phase noise between performance counter and monotonic clocks |
| `mach_timing` | 0.00s | Mach absolute time LSB jitter |
| `memory_timing` | 0.01s | DRAM access timing variations |
| `dram_row_buffer` | 0.00s | DRAM row buffer conflict timing |
| `cache_contention` | 0.01s | CPU cache line contention noise |
| `page_fault_timing` | 0.01s | Virtual memory page fault latency |
| `vm_page_timing` | 0.07s | Mach VM page allocation timing |

### Scheduling (3)

| Source | Time | Description |
|--------|-----:|-------------|
| `sleep_jitter` | 0.00s | Scheduling jitter in nanosleep() calls |
| `dispatch_queue` | 0.09s | GCD dispatch queue scheduling jitter |
| `thread_lifecycle` | 0.08s | pthread create/join cycle timing |

### IO (4)

| Source | Time | Description |
|--------|-----:|-------------|
| `disk_io` | 0.02s | Block device I/O timing jitter |
| `nvme_latency` | 0.01s | NVMe command submission/completion timing |
| `usb_timing` | 0.03s | USB bus transaction timing jitter |
| `fsync_journal` | 16.69s | fsync journal commit latency noise |

### IPC (4)

| Source | Time | Description |
|--------|-----:|-------------|
| `mach_ipc` | 0.04s | Mach port IPC allocation/deallocation timing |
| `pipe_buffer` | 0.01s | Kernel zone allocator via pipe lifecycle |
| `kqueue_events` | 12.25s | BSD kqueue event multiplexing timer/file/socket jitter |
| `keychain_timing` | 0.02s | macOS Keychain Services API timing jitter |

### Microarch (5)

| Source | Time | Description |
|--------|-----:|-------------|
| `speculative_execution` | 0.00s | Branch prediction / speculative execution jitter |
| `dvfs_race` | 0.13s | Cross-core DVFS frequency race |
| `cas_contention` | <0.01s | Multi-thread atomic CAS arbitration contention |
| `tlb_shootdown` | 0.03s | mprotect() TLB invalidation IPI latency |
| `amx_timing` | 0.05s | Apple AMX coprocessor matrix dispatch jitter |

### GPU (3)

| Source | Time | Description |
|--------|-----:|-------------|
| `gpu_timing` | 46.96s | GPU compute dispatch scheduling jitter |
| `gpu_divergence` | 0.76s | GPU warp divergence timing variance |
| `iosurface_crossing` | 0.08s | IOSurface CPU-GPU cross-domain timing |

### Network (3)

| Source | Time | Description |
|--------|-----:|-------------|
| `dns_timing` | 21.91s | DNS resolution timing jitter |
| `tcp_connect_timing` | 39.08s | TCP handshake timing variance |
| `wifi_rssi` | — | WiFi received signal strength fluctuations *(requires WiFi)* |

### System (4)

| Source | Time | Description |
|--------|-----:|-------------|
| `sysctl_deltas` | 0.28s | Kernel counter fluctuations across 50+ sysctl keys |
| `vmstat_deltas` | 0.38s | VM subsystem page fault and swap counters |
| `process_table` | 1.99s | Process table snapshot entropy |
| `ioregistry` | 2.15s | IOKit registry value mining |

### Composite (2)

| Source | Time | Description |
|--------|-----:|-------------|
| `cpu_io_beat` | 0.04s | CPU and I/O subsystem beat frequency |
| `cpu_memory_beat` | 0.00s | CPU and memory controller beat pattern |

### Signal (3)

| Source | Time | Description |
|--------|-----:|-------------|
| `compression_timing` | 1.02s | zlib compression timing oracle |
| `hash_timing` | 0.04s | SHA-256 hash timing data-dependency |
| `spotlight_timing` | 12.91s | Spotlight metadata query timing |

### Sensor (3)

| Source | Time | Description |
|--------|-----:|-------------|
| `audio_noise` | — | Audio ADC thermal noise floor *(requires mic)* |
| `camera_noise` | — | Image sensor dark current noise *(requires camera)* |
| `bluetooth_noise` | 10.01s | BLE ambient RF noise |

Grade is based on min-entropy (H∞). See the [Source Catalog](docs/SOURCES.md) for physics details on each source.

---

## CLI Reference

### `scan` — Discover sources

```bash
openentropy scan
openentropy scan --telemetry
```

### `bench` — Benchmark sources

```bash
openentropy bench                    # standard profile on fast sources
openentropy bench --profile quick    # faster confidence pass
openentropy bench --profile deep     # higher-confidence benchmark
openentropy bench --sources all      # all sources
openentropy bench --sources silicon  # filter by name
openentropy bench --rank-by throughput
openentropy bench --quantum
openentropy bench --rank-by quantum
openentropy bench --quantum --quantum-live-stress
openentropy bench --quantum --quantum-calibration calibration.json
openentropy bench --telemetry
openentropy bench --output bench.json
```

`bench --output` JSON is split into `standard` and `experimental` sections.  
Quantum proxy diagnostics are under `experimental.quantum_proxy_v3` (model id/version included) when `--quantum` is enabled (or `--rank-by quantum` is used).
Telemetry snapshots are emitted under `experimental.telemetry_v1` when `--telemetry` is enabled.
When both `--quantum` and `--telemetry` are enabled, `quantum_proxy_v3` also applies a telemetry confound adjustment and reports it in `report.telemetry_confound`.
When `--rank-by quantum` is used, ordering uses the experimental proxy, but `standard.score` remains non-experimental.
Full method details: [docs/QUANTUM_BENCHMARK.md](docs/QUANTUM_BENCHMARK.md).

#### Quantum Benchmark (Experimental)

`--quantum` does **not** prove a source is quantum. It computes a versioned, operational proxy (`quantum_proxy_v3`) that estimates how much of each source's min-entropy may be attributable to plausibly quantum-dominated mechanisms vs clearly classical effects.

For each source, the model computes:

1. `physics_prior` in `[0,1]` from source mechanism/category priors.
2. `quality_factor` in `[0,1]` from observed stats (autocorrelation, bias, spectral flatness, stationarity, runs behavior).
3. `coupling_penalty` in `[0,1]` from cross-source dependence using **null-debiased** correlation + mutual information (Miller-Madow corrected MI, raw vs shifted-null baseline, plus BH-FDR significance diagnostics).
4. `stress_sensitivity` in `[0,1]` from measured instability: stream-window variability by default, or active CPU/memory/scheduler stress sweep with `--quantum-live-stress`.
5. optional telemetry confound penalty from measured host-state drift when telemetry is enabled.

Then:

```text
stress_effective = clamp01(stress_sensitivity + telemetry_confound_penalty)
q = physics_prior * quality_factor * (1 - stress_effective) * (1 - coupling_penalty)
q_bits = H∞ * q
c_bits = H∞ - q_bits
```

Aggregate quantum:classical is the sum of `q_bits` and `c_bits` across sources.

`v3` also reports uncertainty intervals (Monte Carlo + window variability), ablation/sensitivity diagnostics, and coupling significance summaries (q-value means and significant-pair fractions).

Default normalization constants:

- `corr_threshold = 0.30`
- `mi_threshold_bits = 0.02`
- `stress_delta_bits = 1.5`

What to use for decisions:

- Treat `standard` metrics (especially min-entropy `H∞`, stability, and throughput) as release/production benchmarks.
- Treat `experimental.quantum_proxy_v3` as research diagnostics and ranking context.
- Do not use this proxy as a compliance claim or a formal QRNG proof.

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
openentropy monitor --telemetry
```

| Key | Action |
|-----|--------|
| ↑/↓ | Navigate source list |
| Space | Select source (starts collecting) |
| r | Force refresh |
| q | Quit |

### `bench --source` — Test a single source

```bash
openentropy bench --source mach_timing
```

### `bench` pool quality section

```bash
openentropy bench                    # includes pool quality by default
openentropy bench --no-pool          # skip pool section
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
openentropy server --port 8080 --telemetry    # print startup telemetry snapshot
```

```bash
curl "http://localhost:8080/api/v1/random?length=256&type=uint8"
curl "http://localhost:8080/health"
curl "http://localhost:8080/sources?experimental=true&sample_bytes=1024"
curl "http://localhost:8080/sources?experimental=true&telemetry=true&sample_bytes=1024"
curl "http://localhost:8080/pool/status?telemetry=true"
```

### `analyze` — Statistical source analysis

```bash
openentropy analyze                          # summary view, raw, entropy on
openentropy analyze --view detailed
openentropy analyze --sources mach_timing --no-entropy
openentropy analyze --cross-correlation --output analysis.json
openentropy analyze --quantum-ratio
openentropy analyze --telemetry --output analysis.json
```

### `telemetry` — Standalone telemetry capture

```bash
openentropy telemetry                      # single telemetry_v1 snapshot
openentropy telemetry --window-sec 5       # start/end window with deltas
openentropy telemetry --window-sec 5 --output telemetry.json
```

### `report` — NIST test battery

```bash
openentropy report
openentropy report --source mach_timing --samples 50000
openentropy report --telemetry --output report.md
```

### `sessions` — Analyze recorded sessions

```bash
openentropy sessions sessions/<session-id> --analyze --entropy --quantum-ratio --telemetry --output session_analysis.json
```

---

## Rust API

```toml
[dependencies]
openentropy-core = "0.5"
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
Sources (47) → raw samples → Entropy Pool (XOR combine) → Conditioning (optional) → Output
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
| **MacBook (M-series)** | **47/47** | Full suite — WiFi, BLE, camera, mic |
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
