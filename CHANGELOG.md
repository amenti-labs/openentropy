# Changelog

## 0.2.0 — 2026-02-11

### New Features
- **`stream` command** — Continuous entropy output to stdout with rate limiting and format options (raw/hex/base64)
- **`device` command** — Named pipe (FIFO) entropy feeder for ollama-auxrng integration
- **`server` command** — HTTP entropy server with ANU QRNG-compatible API for quantum-llama.cpp
- **NumPy Generator interface** — `EsotericRandom()` returns a `numpy.random.Generator` backed by hardware entropy
- **EsotericBitGenerator** — NumPy `BitGenerator` subclass for low-level integration

### Sources (30 total)
- Added 15 new sources since v0.1.0:
  - Silicon microarchitecture: DRAM row buffer, cache contention, page fault timing, speculative execution
  - IORegistry deep mining
  - Cross-domain beat frequencies: CPU↔IO, CPU↔memory, multi-domain
  - Compression/hash timing oracles
  - Novel: GCD dispatch, dyld timing, VM page, Spotlight timing

### Improvements
- NIST test battery: 28/31 pass on conditioned pool (Grade A)
- Source filter support on all CLI commands (`--sources`)
- Professional documentation overhaul (ARCHITECTURE, API, SOURCES, OLLAMA_INTEGRATION)
- Updated CI: macOS + Ubuntu, Python 3.10–3.13, ruff + pytest + build
- Repo cleanup: removed stale files, updated .gitignore

### Meta
- Author: Amenti Labs
- License: MIT (unchanged)

## 0.1.0 — 2026-02-11

Initial release.

### Features
- 15 entropy source implementations (timing, sysctl, vmstat, network, disk, memory, GPU, process, audio, camera, sensor, bluetooth)
- Sysctl kernel counter source — auto-discovers 50+ fluctuating keys on macOS
- Multi-source entropy pool with SHA-256 conditioning and health monitoring
- Statistical test suite (Shannon, min-entropy, chi-squared, permutation entropy, compression)
- Conditioning algorithms (Von Neumann debiasing, XOR folding, SHA-256)
- CLI tool: `scan`, `probe`, `bench`, `stream`, `report`, `pool`
- Platform auto-detection for macOS (Linux partial support)
- Thread-safe pool with graceful degradation
