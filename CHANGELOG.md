# Changelog

## 0.5.0 — 2026-02-16

### Changed

- Major CLI UX update:
  - `bench` is now profile-driven (`quick`, `standard`, `deep`) with explicit configurability for rounds, warmup, timeout, and samples per round
  - `bench` now includes a pool-quality section by default (`--no-pool` to skip) and supports JSON output for automation
  - `analyze` now defaults to a verdict-driven summary view with `GOOD/WARNING/CRITICAL` status, top findings, and actionable recommendations
  - `analyze` defaults updated to `--samples 50000`, `--conditioning raw`, and min-entropy breakdown enabled by default (`--no-entropy` to skip)
- Session recording/analysis pipeline improvements:
  - session format v2 now stores both `raw.bin` and `conditioned.bin` streams with separate indexes and expanded `samples.csv` metrics
  - source-isolated recording path ensures conditioned data is derived from the exact raw sample it is paired with
- Entropy collection robustness improvements:
  - timeout-aware parallel collection with in-flight/backoff coordination to avoid repeated scheduling pressure from slow/hung sources
  - bounded raw-byte collection retries to prevent unbounded waiting

### Fixed

- TUI active-source sampling no longer contaminates history/recording with data from non-active sources.
- Unknown `source` parameter in server random endpoint now returns HTTP 400 with structured error.
- Statistical reporting language clarified to avoid overclaiming strict NIST compliance where heuristics are used.

## 0.4.1 — 2026-02-15

### Fixed

- Fixed release/install checksum flow so `install.sh` verifies against release `checksums-sha256.txt`
- Added `version()` export to Python package so examples and runtime version checks work
- Updated root `Makefile` targets to valid Rust workspace commands
- Made Python CI binding job strict and added a Python import smoke test

### Changed

- Synced Python package version to `0.4.1`

## 0.4.0 — 2026-02-13

### Source Taxonomy Refactor

- **Replaced 8 ad-hoc categories with 12 mechanism-based categories:** Thermal, Timing, Scheduling, IO, IPC, Microarch, GPU, Network, System, Composite, Signal, Sensor — each named after the physical mechanism that generates the entropy
- **Added `Platform` enum** (`Any`, `MacOS`, `Linux`) replacing string-based platform requirements
- **Added `Requirement` enum** (`Metal`, `AudioUnit`, `Wifi`, `Usb`, `Camera`, `AppleSilicon`, `Bluetooth`, `IOKit`, `IOSurface`, `SecurityFramework`) for precise hardware dependency tracking
- **Updated `SourceInfo` struct:** `platform_requirements: &[&str]` replaced with typed `platform: Platform` and `requirements: &[Requirement]`
- Updated `SourceInfoSnapshot` to include `platform` and `requirements` fields
- Updated CLI TUI category display with new short codes (THM, TMG, SCH, I/O, IPC, uAR, GPU, NET, SYS, CMP, SIG, SNS)
- All 44 sources reclassified by physical mechanism

### New Frontier Sources (39 → 36 total)

- **`dvfs_race`** — Cross-core DVFS frequency race. Spawns two threads on different CPU cores running tight counting loops; the difference in iteration counts captures physical frequency jitter from independent DVFS controllers.
- **`cas_contention`** — Multi-thread atomic CAS arbitration. 4 threads race on compare-and-swap operations targeting shared cache lines. Hardware coherence engine arbitration timing is physically nondeterministic.

### Research

- **6 proof-of-concept experiments** documented in `docs/findings/deep_research_2026-02-13.md`:
  - DRAM refresh interference timing (too low entropy)
  - P-core vs E-core frequency drift / software ring oscillator (promoted to dvfs_race)
  - Cache coherence fabric ICE timing (too deterministic)
  - Mach thread QoS scheduling (scheduler too quantized)
  - GPU/Accelerate framework timing (overlaps amx_timing)
  - Atomic CAS contention (promoted to cas_contention)

### Improvements

- Both new sources added to `FAST_SOURCES` (25 fast sources total)
- Comprehensive documentation updates: SOURCE_CATALOG, README, CLAUDE.md, ARCHITECTURE, all docs
- Version bump to 0.4.0 across workspace, pyproject.toml
- Removed dead code (vdsp_timing.rs)
- Quality audit: cut `sensor_noise` (redundant with ioregistry), `dyld_timing` (redundant with spotlight_timing), `multi_domain_beat` (too low entropy)
- Fixed stale source counts across all documentation and Cargo.toml files
- `cargo fmt` clean, zero clippy warnings, 212 tests passing

---

## 0.3.0 — 2026-02-12

### Complete Rust Rewrite

The entire project has been rewritten in Rust as a Cargo workspace with 5 crates:
`openentropy-core`, `openentropy-cli`, `openentropy-server`, `openentropy-tests`, and `openentropy-python`.

### Highlights
- **30 entropy sources** across 8 categories (timing, system, network, hardware, silicon, cross-domain, novel, frontier), all with SHA-256 conditioning
- **31 NIST SP 800-22 statistical tests** in a dedicated test suite crate
- **CLI with 9 commands**: `scan`, `probe`, `bench`, `stream`, `device`, `server`, `monitor`, `report`, `pool`
- **Interactive TUI monitor** built with ratatui — live charts, source toggling, RNG display
- **HTTP server** (axum) with ANU-compatible HTTP API
- **PyO3 Python bindings** via maturin for seamless Python interop
- **Zero clippy warnings**, cargo fmt clean across the entire workspace
- **24/27 available sources achieve Grade A** entropy quality

### Crate Breakdown
| Crate | Description |
|-------|-------------|
| `openentropy-core` | EntropySource trait, 30 sources, pool, SHA-256 conditioning, platform detection |
| `openentropy-cli` | clap-based CLI with 9 commands including interactive TUI monitor |
| `openentropy-server` | axum HTTP server with ANU QRNG-compatible `/api/v1/entropy` endpoint |
| `openentropy-tests` | 31 NIST SP 800-22 statistical tests (frequency, runs, spectral, matrix rank, etc.) |
| `openentropy-python` | PyO3 bindings exposing sources, pool, and test suite to Python |

### Meta
- Edition: Rust 2024
- Author: Amenti Labs
- License: MIT (unchanged)

---

## 0.2.0 — 2026-02-11

### New Features
- **`stream` command** — Continuous entropy output to stdout with rate limiting and format options (raw/hex/base64)
- **`device` command** — Named pipe (FIFO) entropy device for feeding hardware entropy to other programs
- **`server` command** — HTTP entropy server with ANU-compatible API
- **NumPy Generator interface** — `OpenEntropyRandom()` returns a `numpy.random.Generator` backed by hardware entropy
- **OpenEntropyBitGenerator** — NumPy `BitGenerator` subclass for low-level integration

### Sources (30 total)
- Added 15 new sources since v0.1.0:
  - Silicon microarchitecture: DRAM row buffer, cache contention, page fault timing, speculative execution
  - IORegistry deep mining
  - Cross-domain beat frequencies: CPU/IO, CPU/memory, multi-domain
  - Compression/hash timing oracles
  - Novel: GCD dispatch, dyld timing, VM page, Spotlight timing

### Improvements
- NIST test battery: 28/31 pass on conditioned pool (Grade A)
- Source filter support on all CLI commands (`--sources`)
- Professional documentation overhaul (ARCHITECTURE, API, SOURCES, INTEGRATIONS)
- Updated CI: macOS + Ubuntu, Python 3.10-3.13, ruff + pytest + build
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
