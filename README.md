<div align="center">

<img src="assets/logo.png" alt="openentropy logo" width="200">

# openentropy

**Harvest real entropy from hardware noise. Study it raw or condition it for crypto.**

[![Crates.io](https://img.shields.io/crates/v/openentropy-core.svg)](https://crates.io/crates/openentropy-core)
[![docs.rs](https://docs.rs/openentropy-core/badge.svg)](https://docs.rs/openentropy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=master&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*58 entropy sources from the physics inside your computer — clock jitter, thermal noise, DRAM timing, cache contention, GPU scheduling, IPC latency, and more. Conditioned output for cryptography. Raw output for research.*

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

> By default, only fast sources (<2s) are used. Pass `--sources all` to include slower sources (DNS, TCP, GPU, BLE).

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
| **SHA-256** (default) | `--conditioning sha256` | Full NIST SP 800-90B conditioning. Cryptographic quality output. |
| **Von Neumann** | `--conditioning vonneumann` | Debiasing only — removes bias while preserving more of the raw signal structure. |
| **Raw** | `--conditioning raw` | No processing. Source bytes with zero whitening — preserves the actual hardware noise signal for research. |

Raw mode is what makes OpenEntropy useful for research. Most HWRNG APIs run DRBG post-processing that makes every source look like uniform random bytes, destroying the information researchers need. Raw output preserves per-source noise structure: bias, autocorrelation, spectral features, and cross-source correlations. See [Conditioning](docs/CONDITIONING.md) for details.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [Source Catalog](docs/SOURCES.md) | All 58 entropy sources with physics explanations |
| [Conditioning](docs/CONDITIONING.md) | Raw vs VonNeumann vs SHA-256 conditioning modes |
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

58 sources across 12 mechanism-based categories. Results from `openentropy bench` on Apple Silicon:

### Thermal (3)

Each source taps a **physically independent** oscillator. They beat the CPU's 24 MHz crystal against other independent PLLs on the SoC, capturing uncorrelated Johnson-Nyquist thermal noise.

| Source | Description |
|--------|-------------|
| `audio_pll_timing` | Audio PLL clock drift from CoreAudio device property queries |
| `display_pll` | Display PLL phase noise from pixel clock (~533 MHz) domain crossing |
| `pcie_pll` | PCIe PHY PLL jitter from Thunderbolt/PCIe clock domain crossing |

### Timing (5)

| Source | Description |
|--------|-------------|
| `clock_jitter` | Phase noise between performance counter and monotonic clocks |
| `dram_row_buffer` | DRAM row buffer hit/miss timing from random memory accesses |
| `page_fault_timing` | Minor page fault timing via mmap/munmap cycles |
| `mach_continuous_timing` | mach_continuous_time() kernel sleep-offset path |
| `ane_timing` | Apple Neural Engine clock domain crossing jitter via IOKit |

### Scheduling (5)

| Source | Description |
|--------|-------------|
| `sleep_jitter` | Scheduling jitter in nanosleep() calls |
| `thread_lifecycle` | Thread create/join kernel scheduling and allocation jitter |
| `pe_core_arithmetic` | P-core/E-core migration timing entropy from arithmetic loop jitter |
| `dispatch_queue_timing` | GCD libdispatch global queue timing — system-wide thread pool entropy |
| `timer_coalescing` | OS timer coalescing wakeup jitter from system-wide timer queue state |

### IO (6)

| Source | Description |
|--------|-------------|
| `disk_io` | Block device I/O timing jitter |
| `fsync_journal` | APFS journal commit timing from full storage stack traversal |
| `usb_enumeration` | IOKit USB device enumeration timing |
| `nvme_iokit_sensors` | NVMe controller sensor polling via IOKit with clock domain crossing |
| `nvme_raw_device` | Direct raw block device reads bypassing filesystem *(requires root)* |
| `nvme_passthrough_linux` | Raw NVMe admin commands via ioctl passthrough *(Linux only)* |

### IPC (4)

| Source | Description |
|--------|-------------|
| `mach_ipc` | Mach port complex OOL message and VM remapping timing jitter |
| `pipe_buffer` | Multi-pipe kernel zone allocator competition and buffer timing jitter |
| `kqueue_events` | Kqueue event multiplexing timing from timers, files, and sockets |
| `keychain_timing` | Keychain/securityd round-trip timing jitter |

### Microarch (15)

| Source | Description |
|--------|-------------|
| `speculative_execution` | Branch predictor state timing via data-dependent branches |
| `dvfs_race` | Cross-core DVFS frequency race between thread pairs |
| `tlb_shootdown` | TLB invalidation broadcast timing via mprotect IPI storms |
| `amx_timing` | Apple AMX coprocessor matrix multiply timing jitter (debiased) |
| `icc_atomic_contention` | Apple Silicon ICC bus arbitration via cross-core atomic contention |
| `prefetcher_state` | Hardware prefetcher stride-learning state |
| `aprr_jit_timing` | Apple APRR undocumented register JIT toggle timing |
| `preemption_boundary` | Kernel scheduler preemption timing via CNTVCT_EL0 reads |
| `sev_event_timing` | ARM64 SEV/SEVL broadcast event timing via ICC fabric load |
| `cntfrq_cache_timing` | CNTFRQ_EL0 system-register cache timing |
| `gxf_register_timing` | Apple GXF EL0-accessible register trap-path timing |
| `dual_clock_domain` | 24 MHz CNTVCT x 41 MHz Apple private timer beat-frequency |
| `sitva` | Scheduler-induced timing variance amplification via NEON FMLA |
| `memory_bus_crypto` | AES-XTS crypto context switching timing from cache flush cycles |
| `commoncrypto_aes_timing` | CommonCrypto AES-128-CBC warm/cold key schedule bimodal timing |

### GPU (3)

| Source | Description |
|--------|-------------|
| `gpu_divergence` | GPU shader thread execution order divergence entropy |
| `iosurface_crossing` | IOSurface GPU/CPU memory domain crossing coherence jitter |
| `nl_inference_timing` | NaturalLanguage ANE inference timing via system-wide NLP cache state |

### Network (3)

| Source | Description |
|--------|-------------|
| `dns_timing` | DNS resolution timing jitter |
| `tcp_connect_timing` | TCP handshake timing variance |
| `wifi_rssi` | WiFi received signal strength fluctuations *(requires WiFi)* |

### System (7)

| Source | Description |
|--------|-------------|
| `sysctl_deltas` | Kernel counter fluctuations across 50+ sysctl keys |
| `vmstat_deltas` | VM subsystem page fault and swap counters |
| `process_table` | Process table snapshot entropy |
| `ioregistry` | IOKit registry value mining |
| `commpage_clock_timing` | macOS COMMPAGE seqlock update synchronization timing |
| `proc_info_timing` | proc_pidinfo / proc_pid_rusage syscall kernel proc_lock contention |
| `getentropy_timing` | getentropy() SEP TRNG reseed timing |

### Signal (3)

| Source | Description |
|--------|-------------|
| `compression_timing` | zlib compression timing oracle |
| `hash_timing` | SHA-256 hash timing data-dependency |
| `spotlight_timing` | Spotlight metadata query timing |

### Sensor (4)

| Source | Description |
|--------|-------------|
| `audio_noise` | Microphone ADC thermal noise (Johnson-Nyquist) *(requires mic)* |
| `camera_noise` | Camera sensor noise (read noise + dark current) *(requires camera)* |
| `bluetooth_noise` | BLE ambient RF noise |
| `smc_highvar_timing` | SMC thermistor ADC + fuel gauge I2C bus timing |

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
openentropy bench --telemetry
openentropy bench --output bench.json
```

`bench --output` JSON includes optional `telemetry_v1` when `--telemetry` is enabled.
Treat telemetry as run context (load, thermal/frequency/memory signals), not as an entropy score.

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
| ↑/↓ or j/k | Navigate source list |
| Space/Enter | Select source (starts collecting) |
| g | Cycle chart mode (time series, histogram, random walk, etc.) |
| c | Cycle conditioning mode (SHA-256 → Von Neumann → Raw) |
| n | Cycle sample size |
| +/- | Adjust refresh rate |
| Tab | Compare two sources (select one, move cursor to another, Tab) |
| p | Pause/resume collection |
| r | Start/stop recording |
| s | Export snapshot |
| q/Esc | Quit |

### `bench --sources` — Test specific sources

```bash
openentropy bench --sources mach_timing
openentropy bench --sources mach_timing,clock_jitter
```

### `bench` pool quality section

```bash
openentropy bench                    # includes pool quality by default
openentropy bench --no-pool          # skip pool section
```

### `stream --fifo` — Named pipe (FIFO)

```bash
openentropy stream --fifo /tmp/openentropy-rng
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
curl "http://localhost:8080/sources?telemetry=true"
curl "http://localhost:8080/pool/status?telemetry=true"
```

### `analyze` — Statistical source analysis

```bash
openentropy analyze                          # summary view, raw, entropy on
openentropy analyze --view detailed
openentropy analyze --sources mach_timing --no-entropy
openentropy analyze --cross-correlation --output analysis.json
openentropy analyze --telemetry --output analysis.json
```

### `telemetry` — Standalone telemetry capture

```bash
openentropy telemetry                      # single telemetry_v1 snapshot
openentropy telemetry --window-sec 5       # start/end window with deltas
openentropy telemetry --window-sec 5 --output telemetry.json
```

### `analyze --report` — NIST test battery

```bash
openentropy analyze --report
openentropy analyze --report --sources mach_timing --samples 50000
openentropy analyze --report --telemetry --output report.md
```

### `sessions` — Analyze recorded sessions

```bash
openentropy sessions sessions/<session-id> --analyze --entropy --telemetry --output session_analysis.json
```

---

## Rust API

```toml
[dependencies]
openentropy-core = "0.7"
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
Sources (58) → raw samples → Entropy Pool (XOR combine) → Conditioning (optional) → Output
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
| **MacBook (M-series)** | **58/58** | Full suite — WiFi, BLE, camera, mic |
| **Mac Mini / Studio / Pro** | 50–52 | No built-in camera, mic on some models |
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

## License

MIT — Copyright © 2026 [Amenti Labs](https://github.com/amenti-labs)
