# Architecture

## Overview

esoteric-entropy is a multi-source entropy harvesting library. It treats every computer as a collection of noisy analog subsystems and extracts randomness from their unpredictable behavior.

## Pipeline

```
Sources (30 classes)
    │
    │  each: collect(n_samples) → uint8 array
    │
    ▼
EntropyPool
    │  - XOR-combine independent streams
    │  - Weight by measured entropy rate
    │  - Health monitoring per source
    │  - Thread-safe buffer
    │
    ▼
Conditioning (SHA-256, NIST SP 800-90B)
    │  - Counter mode for arbitrary output length
    │  - Mixes: pool buffer + internal state + counter + timestamp + os.urandom
    │
    ▼
Output interfaces
    ├── get_random_bytes(n) — Python API
    ├── stream — continuous stdout (CLI)
    ├── device — named pipe / FIFO (CLI)
    ├── server — HTTP API (CLI)
    └── EsotericRandom — NumPy Generator
```

## Module Layout

```
esoteric_entropy/
├── __init__.py          # Public API: EntropyPool, EsotericRandom
├── pool.py              # EntropyPool — multi-source collection + conditioning
├── conditioning.py      # Von Neumann, XOR fold, SHA-256 conditioning
├── platform.py          # Source auto-discovery
├── cli.py               # Click CLI (scan/probe/bench/stream/device/server)
├── http_server.py       # Stdlib HTTP server (ANU QRNG compatible)
├── numpy_compat.py      # NumPy BitGenerator adapter
├── test_suite.py        # NIST-inspired test battery
├── stats.py             # Statistical utilities
├── report.py            # Markdown report generation
└── sources/
    ├── base.py          # EntropySource ABC
    ├── timing.py        # ClockJitter, MachTiming, SleepJitter
    ├── sysctl.py        # Kernel counter mining
    ├── vmstat.py        # VM subsystem counters
    ├── process.py       # Process table entropy
    ├── network.py       # DNS timing, TCP connect
    ├── wifi.py          # WiFi RSSI noise
    ├── disk.py          # Block I/O timing
    ├── memory.py        # DRAM access timing
    ├── gpu.py           # GPU scheduling jitter
    ├── audio.py         # Microphone thermal noise
    ├── camera.py        # Sensor dark current
    ├── sensor.py        # SMC sensor readouts
    ├── bluetooth.py     # BLE RF noise
    ├── ioregistry.py    # IOKit deep mining
    ├── silicon.py       # DRAM row buffer, cache, page fault, speculative
    ├── cross_domain.py  # Beat frequency sources
    ├── compression.py   # Compression/hash timing
    └── novel.py         # GCD dispatch, dyld, VM page, Spotlight
```

## Security Model

- **Not a CSPRNG replacement.** This provides entropy *input*, not a complete cryptographic RNG.
- SHA-256 conditioning ensures output is computationally indistinguishable from random, even if individual sources are weak.
- Pool always mixes `os.urandom(8)` into every output block as a safety net.
- Health monitoring detects degraded sources and flags them.

## Thread Safety

`EntropyPool` uses a threading lock around the internal buffer. Multiple threads can call `get_random_bytes()` concurrently.
