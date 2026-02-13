<div align="center">

# openentropy

**Your computer is a quantum noise observatory.**

[![Crates.io](https://img.shields.io/crates/v/openentropy.svg)](https://crates.io/crates/openentropy)
[![docs.rs](https://img.shields.io/docsrs/openentropy-core)](https://docs.rs/openentropy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/openentropy/ci.yml?branch=main&label=CI)](https://github.com/amenti-labs/openentropy/actions)
[![PyPI](https://img.shields.io/pypi/v/openentropy.svg)](https://pypi.org/project/openentropy/)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*Harvests entropy from 30 unconventional hardware sources hiding inside your computer -- clock jitter, kernel counters, DRAM row buffers, GPU scheduling, cache contention, and more.*

**Built for Apple Silicon. No special hardware. No API keys. Just physics.**

**By [Amenti Labs](https://github.com/amenti-labs)**

</div>

---

## Quick Start

### CLI (Rust)

```bash
# Install from source (crates.io coming soon)
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
cargo install --path crates/openentropy-cli
```

```bash
openentropy scan       # discover entropy sources on your machine
openentropy bench      # benchmark all fast sources (~1s)
openentropy monitor    # live TUI dashboard
openentropy stream --format hex --bytes 64   # output random bytes
openentropy pool       # show pool health metrics
```

> By default, only fast sources (<2s) are used. Add `--sources all` to include slow sources (DNS, TCP, GPU, BLE).

### Python SDK

```bash
# Requires Rust toolchain + maturin
pip install maturin
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
maturin develop --release
```

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} entropy sources available")

pool = EntropyPool.auto()
data = pool.get_random_bytes(256)
print(f"{len(data)} random bytes from hardware entropy")
```

---

## What Makes This Different

Most random number generators are **pseudorandom** -- deterministic algorithms seeded once. Esoteric-entropy is different. It continuously harvests **real physical noise** from your computer's hardware:

- **Timing jitter** from clock phase noise, scheduling nondeterminism, and nanosleep drift
- **Silicon microarchitecture** effects: DRAM row buffer conflicts, CPU cache contention, speculative execution variance, page fault latency
- **Thermal fluctuations** in sensor readouts, GPU dispatch scheduling, disk I/O latency
- **Network nondeterminism** from DNS resolution timing and TCP handshake variance
- **Cross-domain beat frequencies** where CPU, memory, and I/O subsystems interfere

Every source exploits a different physical phenomenon. The pool XOR-combines independent streams and optionally applies SHA-256 conditioning (NIST SP 800-90B) to produce cryptographic-quality output. No single source failure can compromise the pool.

**Crucially, openentropy supports raw (unconditioned) output.** Most QRNG APIs (ANU, Outshift, etc.) apply DRBG post-processing that destroys the raw noise signal. We preserve it. Use `--unconditioned` on the CLI or `?raw=true` on the HTTP API to get XOR-combined source bytes with zero whitening — ideal for researchers studying the actual hardware noise characteristics. See [docs/CONDITIONING.md](docs/CONDITIONING.md) for the full architecture.

---

## Source Catalog

30 entropy sources across 7 categories. Benchmark results from `openentropy bench` on Apple Silicon:

### Timing (3 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `clock_jitter` | 6.507 | 0.00s | Phase noise between performance counter and monotonic clocks |
| `mach_timing` | 7.832 | 0.00s | Mach absolute time LSB jitter (Apple Silicon) |
| `sleep_jitter` | 7.963 | 0.00s | Scheduling jitter in nanosleep() calls |

### System (3 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `sysctl_deltas` | 7.968 | 0.28s | Kernel counter fluctuations across 50+ sysctl keys |
| `vmstat_deltas` | 7.965 | 0.38s | VM subsystem page fault and swap counters |
| `process_table` | 7.971 | 1.99s | Process table snapshot entropy |

### Network (2 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `dns_timing` | 7.958 | 21.91s | DNS resolution timing jitter |
| `tcp_connect_timing` | 7.967 | 39.08s | TCP handshake timing variance |

### Hardware (6 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `disk_io` | 7.960 | 0.02s | Block device I/O timing jitter |
| `memory_timing` | 5.056 | 0.01s | DRAM access timing variations |
| `gpu_timing` | 7.966 | 46.96s | GPU compute dispatch scheduling jitter |
| `sensor_noise` | 7.997 | 0.97s | SMC sensor readout jitter |
| `bluetooth_noise` | 7.961 | 10.01s | BLE ambient RF noise |
| `ioregistry` | 7.964 | 2.15s | IOKit registry value mining |

> `wifi_rssi`, `audio_noise`, and `camera_noise` are also available on machines with the corresponding hardware but are platform-dependent and not always present.

### Silicon Microarchitecture (4 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `dram_row_buffer` | 7.959 | 0.00s | DRAM row buffer conflict timing |
| `cache_contention` | 7.960 | 0.01s | CPU cache line contention noise |
| `page_fault_timing` | 7.967 | 0.01s | Virtual memory page fault latency |
| `speculative_execution` | 7.967 | 0.00s | Branch prediction / speculative execution jitter |

### Cross-Domain Beat Frequencies (3 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `cpu_io_beat` | 6.707 | 0.04s | CPU and I/O subsystem beat frequency |
| `cpu_memory_beat` | 6.256 | 0.00s | CPU and memory controller beat pattern |
| `multi_domain_beat` | 3.867 | 0.00s | Multi-subsystem interference pattern |

### Novel (6 sources)

| Source | Shannon H | Time | Description |
|--------|:---------:|-----:|-------------|
| `compression_timing` | 7.966 | 1.02s | zlib compression timing oracle |
| `hash_timing` | 7.122 | 0.04s | SHA-256 hash timing data-dependency |
| `dispatch_queue` | 6.688 | 0.09s | GCD dispatch queue scheduling jitter |
| `dyld_timing` | 7.967 | 1.35s | Dynamic linker dlsym() timing |
| `vm_page_timing` | 7.963 | 0.07s | Mach VM page allocation timing |
| `spotlight_timing` | 7.969 | 12.91s | Spotlight metadata query timing |

Shannon entropy H is measured on a scale of 0-8 bits per byte. Grade A sources score H >= 7.9.

---

## CLI Reference

The binary is called `openentropy` and provides 9 commands:

### `openentropy scan`

Discover available entropy sources on this machine.

```bash
openentropy scan
```

### `openentropy probe <source>`

Test a specific source and show quality statistics.

```bash
openentropy probe mach_timing
```

### `openentropy bench`

Benchmark sources with a ranked report. Defaults to fast sources only.

```bash
openentropy bench                    # fast sources (~1s)
openentropy bench --sources all      # all 26+ sources (slow!)
openentropy bench --sources silicon  # filter by name
```

### `openentropy stream`

Continuous entropy output to stdout.

```bash
openentropy stream --format hex --bytes 256         # hex output
openentropy stream --format raw --bytes 1024 > /dev/random  # raw bytes
openentropy stream --format base64 --rate 1024      # rate-limited base64
openentropy stream --unconditioned --format raw     # raw, no SHA-256
```

Options: `--format raw|hex|base64`, `--rate N` (bytes/sec), `--bytes N` (0 = infinite), `--sources filter`, `--unconditioned` (skip conditioning)

### `openentropy device <path>`

Create a named pipe (FIFO) that continuously provides entropy. Useful for feeding entropy to other programs.

```bash
openentropy device /tmp/entropy-fifo
```

Options: `--buffer-size N`, `--sources filter`

### `openentropy server`

HTTP entropy server with ANU QRNG-compatible API.

```bash
openentropy server --port 8080
openentropy server --port 8080 --allow-raw   # enable ?raw=true endpoint
```

Options: `--port N` (default 8042), `--host addr` (default 127.0.0.1), `--sources filter`, `--allow-raw` (enable unconditioned output)

### `openentropy monitor`

Live interactive TUI dashboard -- the showpiece of the project.

```bash
openentropy monitor
openentropy monitor --refresh 0.25
openentropy monitor --sources silicon,timing
```

One source active at a time — navigate and select to watch live.

| Key | Action |
|-----|--------|
| ↑/↓ | Navigate source list |
| Space | Select/deselect source (starts collecting) |
| r | Force immediate refresh |
| q | Quit |

The right panel shows physics info and a live entropy chart for the active source.

### `openentropy report`

Run the full NIST SP 800-22 inspired test battery and generate a report. Tests raw (unconditioned) source output.

```bash
openentropy report                              # fast sources
openentropy report --source mach_timing         # single source
openentropy report --samples 50000 --output report.md
```

Options: `--samples N`, `--source name`, `--output path`

> Raw source scores are typically D-F. The conditioned pool output scores A (7.9+ bits/byte Shannon entropy). This is by design — conditioning is the value add.

### `openentropy pool`

Show entropy pool health metrics with per-source stats.

```bash
openentropy pool                    # fast sources
openentropy pool --sources all      # all sources
```

---

## Rust API

Add `openentropy-core` to your `Cargo.toml`:

```toml
[dependencies]
openentropy-core = "0.3"
```

```rust
use openentropy_core::{EntropyPool, detect_available_sources};

let pool = EntropyPool::auto();
let random_bytes = pool.get_random_bytes(256);
let health = pool.health_report();
println!("{} sources, {} bytes collected", health.total, health.raw_bytes);
```

The `openentropy-core` crate exposes:

- `EntropyPool` -- thread-safe multi-source entropy pool with SHA-256 conditioning
- `EntropySource` trait -- implement your own entropy sources
- `detect_available_sources()` -- platform-aware source discovery
- `quick_shannon()` / `quick_quality()` -- entropy quality analysis utilities
- `SourceCategory` / `SourceInfo` -- source metadata types

---

## Python SDK

The Python package is built with PyO3 and maturin, providing native Rust performance with a Pythonic API.

```bash
pip install openentropy
```

```python
from openentropy import EntropyPool

pool = EntropyPool.auto()
data = pool.get_random_bytes(256)
print(f"{len(data)} random bytes from {pool.source_count} sources")

# Health monitoring
report = pool.health_report()
print(f"Healthy sources: {report['healthy']}/{report['total']}")

# Parallel collection with timeout
pool.collect_all(parallel=True, timeout=10.0)
```

---

## HTTP Server and Ollama Integration

### Server API

Start the server and query entropy over HTTP:

```bash
openentropy server --port 8080
```

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/random?length=1024&type=hex16` | Random data (hex16, uint8, uint16). Add `&raw=true` for unconditioned output (requires `--allow-raw` flag) |
| `GET /health` | Pool health status |
| `GET /sources` | List all sources with stats |
| `GET /pool/status` | Detailed pool metrics |

```bash
curl "http://localhost:8080/api/v1/random?length=256&type=uint8"
curl "http://localhost:8080/health"
curl "http://localhost:8080/sources"
curl "http://localhost:8080/pool/status"
```

The API is compatible with the ANU QRNG format, so any client expecting that protocol will work.

### Ollama Integration

Feed hardware entropy into LLM inference via a named pipe:

```bash
# Terminal 1: Start entropy device
openentropy device /tmp/esoteric-rng

# Terminal 2: Run Ollama with hardware entropy
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

Or via HTTP with quantum-llama.cpp:

```bash
# Terminal 1: Start entropy server
openentropy server --port 8080

# Terminal 2: Point quantum-llama.cpp at it
./llama-cli -m model.gguf --qrng-url http://localhost:8080/api/v1/random
```

---

## Architecture

Esoteric-entropy is a Rust workspace with 5 crates:

| Crate | Description |
|-------|-------------|
| `openentropy-core` | Core entropy harvesting library -- sources, pool, conditioning |
| `openentropy-cli` | CLI binary with 9 commands and interactive TUI dashboard |
| `openentropy-server` | Axum-based HTTP entropy server (ANU QRNG API compatible) |
| `openentropy-tests` | NIST SP 800-22 inspired randomness test battery |
| `openentropy-python` | Python bindings via PyO3/maturin |

Data flow:

```
+-------------------------------------------------------+
|              ENTROPY SOURCES (30)                      |
|                                                       |
|  Timing    System    Network    Hardware               |
|  Silicon   Cross-Domain   Novel                        |
+---------------------------+---------------------------+
                            | raw samples (u8)
                            v
                  +--------------------+
                  |   ENTROPY POOL     |
                  |   XOR combine      |
                  |   health monitor   |
                  +--------+-----------+
                           |
                           v
                  +--------------------+
                  |   CONDITIONING     |
                  |   SHA-256 (NIST)   |
                  |   counter mode     |
                  +--------+-----------+
                           |
            +--------------+--------------+
            |              |              |
            v              v              v
       Rust API       CLI / TUI      Server / FIFO
    (openentropy-core)  (openentropy-cli)  (openentropy-server)
            |
            v
     Python bindings
    (openentropy-python)
```

The pool is thread-safe (`Mutex`-guarded state) and supports parallel collection across all sources. SHA-256 conditioning in counter mode ensures that even if individual sources produce biased output, the combined conditioned stream is cryptographic quality.

---

## Platform Support

**Primary target: macOS on Apple Silicon** (M1/M2/M3/M4)

| Platform | Sources | Notes |
|----------|:-------:|-------|
| **MacBook (M-series)** | **30/30** | Full suite -- WiFi, BLE, camera, mic, sensors, all silicon sources |
| **Mac Mini / Studio / Pro** | 27-28/30 | Most sources -- no built-in camera, mic, or motion sensors |
| **Intel Mac** | ~20/30 | Timing, system, network sources work; some silicon sources are ARM-specific |
| **Linux** | 10-15/30 | Timing, network, disk, process sources; system sources planned |

The library gracefully detects available hardware and only activates sources that work on your machine. MacBooks provide the richest entropy because they pack the most sensors into a single device.

---

## Building from Source

Requirements: Rust 2024 edition (1.85+), macOS or Linux.

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy

# Build everything
cargo build --release

# Run the CLI directly
cargo run -p openentropy-cli -- scan
cargo run -p openentropy-cli -- bench
cargo run -p openentropy-cli -- monitor

# Run tests
cargo test --workspace

# Install the CLI binary
cargo install --path crates/openentropy-cli
```

### Building the Python package

Requires [maturin](https://github.com/PyO3/maturin) and Python 3.10+:

```bash
pip install maturin
maturin develop --release    # install in current Python env

# Verify
python3 -c "from openentropy import EntropyPool; print(EntropyPool.auto().get_random_bytes(16).hex())"
```

To build a distributable wheel:

```bash
maturin build --release
# Output in target/wheels/
```

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
cargo build
cargo test --workspace
cargo clippy --workspace
```

Ideas for contributions:

- New entropy sources (especially Linux-specific ones)
- Performance improvements to collection and conditioning
- Additional NIST test implementations
- Platform support for Windows

---

## License

MIT License -- Copyright (c) 2026 [Amenti Labs](https://github.com/amenti-labs)

See [LICENSE](LICENSE) for the full text.
