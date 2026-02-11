# 🔬 esoteric-entropy

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Python 3.10+](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org)
[![CI](https://github.com/esoteric-entropy/esoteric-entropy/actions/workflows/ci.yml/badge.svg)](https://github.com/esoteric-entropy/esoteric-entropy/actions)

**Your computer is a quantum noise observatory. This library knows where to listen.**

Harvests entropy from unconventional hardware sources — clock jitter, kernel counters, memory timing, GPU scheduling, network latency, and more. Combines them into a high-quality random byte stream via a multi-source entropy pool with SHA-256 conditioning.

## Quick Install

```bash
pip install esoteric-entropy
```

## Quick Start

```python
from esoteric_entropy import EntropyPool

pool = EntropyPool.auto()          # discover all sources on this machine
random_bytes = pool.get_random_bytes(32)  # 32 bytes of conditioned entropy
pool.print_health()                # see what's feeding the pool
```

## CLI

```bash
# Discover available sources
$ esoteric-entropy scan
Platform: Darwin arm64 (Python 3.14.3)

Found 11 available entropy source(s):
  ✅ clock_jitter              Phase noise between perf_counter and monotonic clocks
  ✅ mach_timing               Mach kernel absolute-time LSB jitter
  ✅ sysctl_counters           Kernel counter deltas from 50+ fluctuating sysctl keys
  ✅ vmstat                    VM statistics counter deltas (page faults, swaps, etc.)
  ✅ dns_timing                DNS query round-trip timing jitter
  ...

# Test a specific source
$ esoteric-entropy probe clock_jitter
Probing: clock_jitter
  Grade:           B
  Shannon entropy: 4.2310 / 8.0 bits
  Compression:     0.6842

# Benchmark everything
$ esoteric-entropy bench

# Stream entropy to stdout (pipe to file, other tools, etc.)
$ esoteric-entropy stream --bytes 1024 > random.bin

# Full characterisation report in Markdown
$ esoteric-entropy report

# Run the entropy pool with health monitoring
$ esoteric-entropy pool
```

## Source Catalog

| Source | Type | Platform | Physics | Est. Rate |
|--------|------|----------|---------|-----------|
| `clock_jitter` | Timing | All | PLL phase noise between clock domains | 500 b/s |
| `mach_timing` | Timing | macOS | ARM system counter LSB jitter | 2000 b/s |
| `sleep_jitter` | Timing | All | OS scheduler timing inaccuracy | 200 b/s |
| `sysctl_counters` | Kernel | macOS | 50+ fluctuating kernel counters (TCP, VM, etc.) | 5000 b/s |
| `vmstat` | Kernel | macOS | VM page fault / swap counter deltas | 1000 b/s |
| `dns_timing` | Network | All | UDP round-trip jitter across physical links | 100 b/s |
| `tcp_connect` | Network | All | TCP handshake timing jitter | 50 b/s |
| `disk_io` | Storage | All | NVMe/SSD read latency jitter (NAND physics) | 800 b/s |
| `memory_timing` | Memory | All | DRAM refresh, cache miss, TLB timing | 1500 b/s |
| `gpu_timing` | Compute | macOS | GPU dispatch completion jitter | 300 b/s |
| `process_table` | System | All | Process churn, PID allocation | 400 b/s |
| `audio_thermal` | Sensor | Mic req. | Johnson-Nyquist thermal noise | 10000 b/s |
| `camera_shot_noise` | Sensor | Camera req. | Photon shot noise / dark current | 50000 b/s |
| `bluetooth_ble` | RF | macOS+BT | BLE RSSI multipath fading | 50 b/s |

## Architecture

```
┌──────────────────────────────────────────────────┐
│                  Entropy Pool                     │
│                                                   │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐            │
│  │ Clock   │ │ Sysctl  │ │  DNS    │  ...more   │
│  │ Jitter  │ │Counters │ │ Timing  │  sources   │
│  └────┬────┘ └────┬────┘ └────┬────┘            │
│       │           │           │                   │
│       ▼           ▼           ▼                   │
│  ┌─────────────────────────────────┐             │
│  │     XOR Combine + Buffer        │             │
│  └──────────────┬──────────────────┘             │
│                 │                                  │
│                 ▼                                  │
│  ┌─────────────────────────────────┐             │
│  │   SHA-256 Conditioning          │             │
│  │   (state + pool + counter +     │             │
│  │    timestamp + os.urandom)      │             │
│  └──────────────┬──────────────────┘             │
│                 │                                  │
│                 ▼                                  │
│          Conditioned Output                       │
└──────────────────────────────────────────────────┘
```

## The Sysctl Source (Crown Jewel)

The most unique source in the package. On macOS, `sysctl` exposes 1600+ kernel counters. Our discovery found **58 keys that change within 0.2 seconds** — TCP statistics, VM page faults, network counters, security subsystem state, and more. Each delta is unpredictable at fine granularity, giving us a rich, high-bandwidth entropy stream from the kernel itself.

```python
from esoteric_entropy.sources.sysctl import SysctlSource

src = SysctlSource()
keys = src.discover_fluctuating_keys()
print(f"Found {len(keys)} fluctuating sysctl keys")
print(src.categorize_keys())
```

## Adding a New Source

See [CONTRIBUTING.md](CONTRIBUTING.md) for the template. The interface is simple:

```python
class YourSource(EntropySource):
    name = "your_source"
    def is_available(self) -> bool: ...
    def collect(self, n_samples: int) -> np.ndarray: ...
    def entropy_quality(self) -> dict: ...
```

## Research Background

The entropy sources in this package are grounded in well-understood physics:

- **Clock jitter**: PLL phase noise — [IEEE 802.3 jitter specs](https://standards.ieee.org/ieee/802.3/10422/)
- **Thermal noise**: Johnson-Nyquist noise — [Physical Review, 1928](https://doi.org/10.1103/PhysRev.32.97)
- **Shot noise**: Photon arrival statistics — [Schottky, 1918](https://doi.org/10.1002/andp.19183621105)
- **DRAM timing**: Row hammer / refresh timing — [Kim et al., ISCA 2014](https://doi.org/10.1109/ISCA.2014.6853210)

## License

MIT — see [LICENSE](LICENSE).
