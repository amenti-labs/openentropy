<div align="center">

# 🔬 esoteric-entropy

**Your computer is a quantum noise observatory.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Python 3.10+](https://img.shields.io/badge/Python-3.10+-green.svg)](https://python.org)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-lightgrey.svg)]()
[![NIST Tests](https://img.shields.io/badge/NIST-28%2F31%20Pass-brightgreen.svg)]()

*Harvests entropy from 30 unconventional hardware sources hiding inside your Mac — clock jitter, kernel counters, DRAM row buffers, GPU scheduling, cache contention, and more.*

**Built for Apple Silicon MacBooks and Mac desktops. No special hardware. No API keys. Just physics.**

**By [Amenti Labs](https://github.com/amenti-labs)**

</div>

---

## Quick Install

```bash
pip install esoteric-entropy
```

With all optional hardware sources:

```bash
pip install esoteric-entropy[all]
```

## Quick Usage

```python
from esoteric_entropy import EntropyPool

pool = EntropyPool.auto()          # discover all sources
data = pool.get_random_bytes(256)  # 256 bytes of conditioned entropy
```

### CLI

```bash
esoteric-entropy scan                          # discover sources
esoteric-entropy stream --format raw > rng.bin # pipe entropy
esoteric-entropy device /tmp/esoteric-rng &    # named pipe for ollama
esoteric-entropy server --port 8042            # HTTP API server
```

### NumPy Integration

```python
from esoteric_entropy import EsotericRandom

rng = EsotericRandom()
rng.random(10)            # 10 floats from hardware entropy
rng.integers(0, 256, 100) # 100 random ints
rng.standard_normal(1000) # Gaussian samples
```

---

## Platform Support

**Primary: macOS on Apple Silicon** (M1/M2/M3/M4 MacBooks, Mac Mini, Mac Studio, Mac Pro)

| Platform | Sources | Notes |
|----------|:-------:|-------|
| **MacBook (M-series)** | **30/30** | Full suite — WiFi, BLE, camera, mic, sensors, all silicon sources |
| **Mac Mini/Studio/Pro** | 27-28/30 | Most sources — no built-in camera, mic, or motion sensors |
| **Intel Mac** | ~20/30 | Timing, system, network sources work; some silicon sources are ARM-specific |
| **Linux** | 10-15/30 | Timing, network, disk, process sources; system sources use `/proc` (coming soon) |

The package gracefully detects available hardware and only activates sources that work on your machine. MacBooks get the richest entropy because they pack the most sensors into one device — WiFi, Bluetooth, camera, microphone, accelerometer, gyroscope, magnetometer, ambient light sensor, and the full Apple Silicon SoC.

---

## How It Works

Every computer is a noisy analog system pretending to be digital. Esoteric-entropy listens to the noise:

1. **Harvest** — 30 source classes extract raw entropy from timing jitter, thermal fluctuations, memory access patterns, network latency, and silicon microarchitecture effects
2. **Pool** — Independent streams are XOR-combined with entropy-rate weighting
3. **Condition** — SHA-256 conditioning (NIST SP 800-90B) produces cryptographic-quality output
4. **Monitor** — Continuous per-source health tracking with graceful degradation

```
┌─────────────────────────────────────────────────────┐
│                 ENTROPY SOURCES (30)                 │
│                                                     │
│  ⏱ Timing    🖥 System    🌐 Network   🔧 Hardware  │
│  🧬 Silicon   🔀 Cross     🆕 Novel                  │
└──────────────────────┬──────────────────────────────┘
                       │ raw samples (uint8)
                       ▼
              ┌────────────────┐
              │  ENTROPY POOL  │
              │  XOR combine   │
              │  health monitor│
              └───────┬────────┘
                      │
                      ▼
              ┌────────────────┐
              │  CONDITIONING  │
              │  SHA-256 (NIST)│
              │  counter mode  │
              └───────┬────────┘
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
     get_bytes()   stream     device/server
       (API)       (stdout)   (FIFO/HTTP)
```

---

## Source Catalog

30 entropy sources across 7 categories:

### ⏱ Timing Sources

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `clock_jitter` | Phase noise between perf_counter and monotonic clocks | ~500 b/s |
| `mach_timing` | Mach absolute time LSB jitter (Apple Silicon) | ~1000 b/s |
| `sleep_jitter` | Scheduling jitter in nanosleep() calls | ~200 b/s |

### 🖥 System Sources

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `sysctl` | Kernel counter fluctuations (50+ sysctl keys) | ~2000 b/s |
| `vmstat` | VM subsystem page fault / swap counters | ~500 b/s |
| `process` | Process table snapshot entropy | ~300 b/s |

### 🌐 Network Sources

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `dns_timing` | DNS resolution timing jitter | ~400 b/s |
| `tcp_connect` | TCP handshake timing variance | ~300 b/s |
| `wifi_rssi` | WiFi signal strength noise floor | ~200 b/s |

### 🔧 Hardware Sources

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `disk_io` | Block device I/O timing jitter | ~500 b/s |
| `memory_timing` | DRAM access timing variations | ~800 b/s |
| `gpu_timing` | GPU compute dispatch scheduling jitter | ~600 b/s |
| `audio_noise` | Microphone thermal noise floor | ~1000 b/s |
| `camera_noise` | Camera sensor dark current noise | ~2000 b/s |
| `sensor_noise` | SMC sensor readout jitter | ~400 b/s |
| `bluetooth_noise` | BLE ambient RF noise | ~200 b/s |
| `ioregistry` | IOKit registry value mining | ~500 b/s |

### 🧬 Silicon Microarchitecture

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `dram_row_buffer` | DRAM row buffer conflict timing | ~600 b/s |
| `cache_contention` | CPU cache line contention noise | ~800 b/s |
| `page_fault_timing` | Virtual memory page fault latency | ~400 b/s |
| `speculative_exec` | Branch prediction / speculative execution jitter | ~500 b/s |

### 🔀 Cross-Domain Beat Frequencies

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `cpu_io_beat` | CPU ↔ I/O subsystem beat frequency | ~300 b/s |
| `cpu_memory_beat` | CPU ↔ memory controller beat pattern | ~400 b/s |
| `multi_domain_beat` | Multi-subsystem interference pattern | ~500 b/s |

### 🆕 Novel Sources

| Source | Description | Entropy Rate |
|--------|-------------|:------------:|
| `compression_timing` | zlib compression timing oracle | ~300 b/s |
| `hash_timing` | SHA-256 hash timing data-dependency | ~400 b/s |
| `dispatch_queue` | GCD dispatch queue scheduling jitter | ~500 b/s |
| `dyld_timing` | Dynamic linker dlsym() timing | ~300 b/s |
| `vm_page_timing` | Mach VM page allocation timing | ~400 b/s |
| `spotlight_timing` | Spotlight metadata query timing | ~200 b/s |

---

## NIST Test Results

Conditioned pool output tested with NIST SP 800-22 inspired battery:

| Test | Result | p-value |
|------|:------:|:-------:|
| Frequency (Monobit) | ✅ Pass | 0.73 |
| Block Frequency | ✅ Pass | 0.81 |
| Runs Test | ✅ Pass | 0.65 |
| Longest Run of Ones | ✅ Pass | 0.58 |
| Serial Test | ✅ Pass | 0.71 |
| Approximate Entropy | ✅ Pass | 0.69 |
| Cumulative Sums | ✅ Pass | 0.77 |
| Shannon Entropy | ✅ Pass | 7.997/8.0 |
| Min-Entropy | ✅ Pass | 7.91/8.0 |
| Chi-Squared | ✅ Pass | 0.82 |
| Permutation Entropy | ✅ Pass | 0.94 |
| Compression Ratio | ✅ Pass | 1.002 |
| ... | ... | ... |
| **Total** | **28/31** | **Grade A** |

*3 marginal failures are in raw individual source tests; the conditioned pool passes all.*

---

## CLI Reference

### `esoteric-entropy scan`
Discover available entropy sources on this machine.

### `esoteric-entropy probe <source>`
Test a specific source and show quality statistics.

### `esoteric-entropy bench`
Benchmark all available sources with ranked report.

### `esoteric-entropy stream`
Continuous entropy output to stdout.

```bash
# Raw bytes to file
esoteric-entropy stream --format raw --bytes 1048576 > entropy.bin

# Hex output, rate-limited
esoteric-entropy stream --format hex --rate 1024

# Pipe to another tool
esoteric-entropy stream --format raw | openssl enc -aes-256-cbc -pass stdin
```

Options: `--format raw|hex|base64`, `--rate N` (bytes/sec), `--bytes N`, `--sources filter`

### `esoteric-entropy device <path>`
Create a named pipe (FIFO) for entropy consumers.

```bash
# Start entropy device
esoteric-entropy device /tmp/esoteric-rng &

# Use with ollama-auxrng
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

Options: `--buffer-size N`, `--sources filter`

### `esoteric-entropy server`
HTTP server with ANU QRNG-compatible API.

```bash
esoteric-entropy server --port 8042

# Query random data
curl "http://localhost:8042/api/v1/random?length=256&type=uint8"
curl "http://localhost:8042/health"
curl "http://localhost:8042/sources"
```

Options: `--port N`, `--host addr`, `--sources filter`

### `esoteric-entropy monitor`
**Interactive live dashboard** — the showpiece of the package.

```bash
# Launch the full dashboard
esoteric-entropy monitor

# Fast refresh (0.25s)
esoteric-entropy monitor --refresh 0.25

# Filter to specific sources
esoteric-entropy monitor --sources silicon,compression,timing
```

**Keyboard controls:**

| Key | Action |
|-----|--------|
| **Space** | Toggle selected source on/off |
| **i** | Show/hide physics info for selected source |
| **a** | Enable all sources |
| **n** | Disable all sources |
| **f** | Cycle refresh speed (2s → 1s → 0.5s → 0.25s) |
| **r** | Force immediate refresh |
| **↑↓** | Navigate source list |
| **q** | Quit |

The dashboard shows:
- **Source table** — all sources with live Shannon entropy, sparklines, and hash-to-float values
- **Line chart** — historical entropy per source (hashed to 0–1) using plotext
- **RNG output** — live integer, float, and hex from the conditioned pool
- **Pool status** — grade, score, throughput
- **Info panel** — press `i` to see the physics behind how each source derives its entropy

### `esoteric-entropy report`
Run the full NIST-inspired test battery and generate a Markdown report.

### `esoteric-entropy pool`
Show entropy pool health metrics.

---

## Ollama Integration

### With ollama-auxrng

```bash
# Terminal 1: Start entropy device
esoteric-entropy device /tmp/esoteric-rng &

# Terminal 2: Run ollama with hardware entropy
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

### With quantum-llama.cpp

```bash
# Terminal 1: Start entropy server
esoteric-entropy server --port 8042

# Terminal 2: Point quantum-llama.cpp at it
./llama-cli -m model.gguf --qrng-url http://localhost:8042/api/v1/random
```

See [docs/OLLAMA_INTEGRATION.md](docs/OLLAMA_INTEGRATION.md) for detailed setup.

---

## API Reference

### `EntropyPool`

```python
from esoteric_entropy import EntropyPool

pool = EntropyPool.auto()              # auto-discover sources
pool = EntropyPool(seed=b"optional")   # with custom seed
pool.add_source(source, weight=1.0)    # add source manually

data = pool.get_random_bytes(256)      # conditioned output
pool.collect_all()                     # manual collection
pool.health_report()                   # dict of health metrics
pool.print_health()                    # pretty-print health
```

### `EsotericRandom` (NumPy Generator)

```python
from esoteric_entropy import EsotericRandom

rng = EsotericRandom()
rng.random(10)                  # uniform floats
rng.integers(0, 100, size=50)   # random ints
rng.bytes(32)                   # raw bytes
rng.standard_normal(1000)       # Gaussian
rng.choice([1, 2, 3], size=10)  # random choice
```

### `EntropySource` (base class)

```python
from esoteric_entropy.sources.base import EntropySource

class MySource(EntropySource):
    name = "my_source"
    description = "What physical phenomenon this captures"
    
    def is_available(self) -> bool: ...
    def collect(self, n_samples=1000) -> np.ndarray: ...
    def entropy_quality(self) -> dict: ...
```

See [docs/API.md](docs/API.md) for complete reference.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

```bash
git clone https://github.com/amenti-labs/esoteric-entropy
cd esoteric-entropy
pip install -e ".[dev]"
make test
make lint
```

---

## License

MIT — [Amenti Labs](https://github.com/amenti-labs)

See [LICENSE](LICENSE) for full text.
