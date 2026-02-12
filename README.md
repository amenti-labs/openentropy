<div align="center">

# esoteric-entropy

**Your computer is a quantum noise observatory.**

[![Crates.io](https://img.shields.io/crates/v/esoteric-entropy.svg)](https://crates.io/crates/esoteric-entropy)
[![docs.rs](https://img.shields.io/docsrs/esoteric-core)](https://docs.rs/esoteric-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/amenti-labs/esoteric-entropy/ci.yml?branch=main&label=CI)](https://github.com/amenti-labs/esoteric-entropy/actions)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()

*Harvests entropy from 30 unconventional hardware sources hiding inside your computer -- clock jitter, kernel counters, DRAM row buffers, GPU scheduling, cache contention, and more.*

**Built for Apple Silicon. No special hardware. No API keys. Just physics.**

**By [Amenti Labs](https://github.com/amenti-labs)**

</div>

---

## Quick Start

### CLI (Rust)

```bash
cargo install esoteric-entropy
```

```bash
esoteric-entropy scan       # discover entropy sources on your machine
esoteric-entropy bench      # benchmark all sources
esoteric-entropy monitor    # live TUI dashboard
```

### Python SDK

```bash
pip install esoteric-entropy
```

```python
from esoteric_entropy import EntropyPool

pool = EntropyPool.auto()
data = pool.get_random_bytes(256)
print(f"{len(data)} random bytes from {pool.source_count} sources")
```

---

## What Makes This Different

Most random number generators are **pseudorandom** -- deterministic algorithms seeded once. Esoteric-entropy is different. It continuously harvests **real physical noise** from your computer's hardware:

- **Timing jitter** from clock phase noise, scheduling nondeterminism, and nanosleep drift
- **Silicon microarchitecture** effects: DRAM row buffer conflicts, CPU cache contention, speculative execution variance, page fault latency
- **Thermal fluctuations** in sensor readouts, GPU dispatch scheduling, disk I/O latency
- **Network nondeterminism** from DNS resolution timing and TCP handshake variance
- **Cross-domain beat frequencies** where CPU, memory, and I/O subsystems interfere

Every source exploits a different physical phenomenon. The pool XOR-combines independent streams and applies SHA-256 conditioning (NIST SP 800-90B) to produce cryptographic-quality output. No single source failure can compromise the pool.

---

## Source Catalog

30 entropy sources across 7 categories. Benchmark results from `esoteric-entropy bench` on Apple Silicon:

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

The binary is called `esoteric-entropy` and provides 9 commands:

### `esoteric-entropy scan`

Discover available entropy sources on this machine.

```bash
esoteric-entropy scan
```

### `esoteric-entropy probe <source>`

Test a specific source and show quality statistics.

```bash
esoteric-entropy probe mach_timing
```

### `esoteric-entropy bench`

Benchmark all available sources with a ranked report.

```bash
esoteric-entropy bench
```

### `esoteric-entropy stream`

Continuous entropy output to stdout.

```bash
esoteric-entropy stream --format hex --bytes 256         # hex output
esoteric-entropy stream --format raw --bytes 1024 > /dev/random  # raw bytes
esoteric-entropy stream --format base64 --rate 1024      # rate-limited base64
```

Options: `--format raw|hex|base64`, `--rate N` (bytes/sec), `--bytes N` (0 = infinite), `--sources filter`

### `esoteric-entropy device <path>`

Create a named pipe (FIFO) that continuously provides entropy. Useful for feeding entropy to other programs.

```bash
esoteric-entropy device /tmp/entropy-fifo
```

Options: `--buffer-size N`, `--sources filter`

### `esoteric-entropy server`

HTTP entropy server with ANU QRNG-compatible API.

```bash
esoteric-entropy server --port 8080
```

Options: `--port N` (default 8042), `--host addr` (default 127.0.0.1), `--sources filter`

### `esoteric-entropy monitor`

Live interactive TUI dashboard -- the showpiece of the project.

```bash
esoteric-entropy monitor
esoteric-entropy monitor --refresh 0.25
esoteric-entropy monitor --sources silicon,timing
```

Keyboard controls:

| Key | Action |
|-----|--------|
| Space | Toggle selected source on/off |
| i | Show/hide physics info for selected source |
| a | Enable all sources |
| n | Disable all sources |
| f | Cycle refresh speed (2s / 1s / 0.5s / 0.25s) |
| r | Force immediate refresh |
| Up/Down | Navigate source list |
| q | Quit |

### `esoteric-entropy report`

Run the full NIST SP 800-22 inspired test battery and generate a report.

```bash
esoteric-entropy report
esoteric-entropy report --source mach_timing --samples 50000
esoteric-entropy report --output report.md
```

Options: `--samples N`, `--source name`, `--output path`

### `esoteric-entropy pool`

Show entropy pool health metrics.

```bash
esoteric-entropy pool
```

---

## Rust API

Add `esoteric-core` to your `Cargo.toml`:

```toml
[dependencies]
esoteric-core = "0.3"
```

```rust
use esoteric_core::{EntropyPool, detect_available_sources};

let pool = EntropyPool::auto();
let random_bytes = pool.get_random_bytes(256);
let health = pool.health_report();
println!("{} sources, {} bytes collected", health.total, health.raw_bytes);
```

The `esoteric-core` crate exposes:

- `EntropyPool` -- thread-safe multi-source entropy pool with SHA-256 conditioning
- `EntropySource` trait -- implement your own entropy sources
- `detect_available_sources()` -- platform-aware source discovery
- `quick_shannon()` / `quick_quality()` -- entropy quality analysis utilities
- `SourceCategory` / `SourceInfo` -- source metadata types

---

## Python SDK

The Python package is built with PyO3 and maturin, providing native Rust performance with a Pythonic API.

```bash
pip install esoteric-entropy
```

```python
from esoteric_entropy import EntropyPool

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
esoteric-entropy server --port 8080
```

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/random?length=1024&type=hex16` | Random data (hex16, uint8, uint16) |
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
esoteric-entropy device /tmp/esoteric-rng

# Terminal 2: Run Ollama with hardware entropy
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

Or via HTTP with quantum-llama.cpp:

```bash
# Terminal 1: Start entropy server
esoteric-entropy server --port 8080

# Terminal 2: Point quantum-llama.cpp at it
./llama-cli -m model.gguf --qrng-url http://localhost:8080/api/v1/random
```

---

## Architecture

Esoteric-entropy is a Rust workspace with 5 crates:

| Crate | Description |
|-------|-------------|
| `esoteric-core` | Core entropy harvesting library -- sources, pool, conditioning |
| `esoteric-cli` | CLI binary with 9 commands and interactive TUI dashboard |
| `esoteric-server` | Axum-based HTTP entropy server (ANU QRNG API compatible) |
| `esoteric-tests` | NIST SP 800-22 inspired randomness test battery |
| `esoteric-python` | Python bindings via PyO3/maturin |

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
    (esoteric-core)  (esoteric-cli)  (esoteric-server)
            |
            v
     Python bindings
    (esoteric-python)
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
git clone https://github.com/amenti-labs/esoteric-entropy.git
cd esoteric-entropy

# Build everything
cargo build --release

# Run the CLI directly
cargo run -p esoteric-cli -- scan
cargo run -p esoteric-cli -- bench
cargo run -p esoteric-cli -- monitor

# Run tests
cargo test --workspace

# Install the CLI binary
cargo install --path crates/esoteric-cli
```

### Building the Python package

Requires [maturin](https://github.com/PyO3/maturin):

```bash
pip install maturin
cd crates/esoteric-python
maturin develop --release
```

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
git clone https://github.com/amenti-labs/esoteric-entropy.git
cd esoteric-entropy
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
