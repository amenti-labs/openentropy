# Changelog

## 0.4.0 — 2026-02-13

### New Frontier Sources (37 → 39 total)

- **`dvfs_race`** — Cross-core DVFS frequency race. Spawns two threads on different CPU cores running tight counting loops; the difference in iteration counts captures physical frequency jitter from independent DVFS controllers. PoC measured H∞ = 7.288 bits/byte — the highest of any discovered source.
- **`cas_contention`** — Multi-thread atomic CAS arbitration. 4 threads race on compare-and-swap operations targeting shared cache lines. Hardware coherence engine arbitration timing is physically nondeterministic. PoC measured H∞ = 2.463 bits/byte.

### Research

- **6 proof-of-concept experiments** documented in `docs/findings/deep_research_2026-02-13.md`:
  - DRAM refresh interference timing (H∞ = 0.949 — too low)
  - P-core vs E-core frequency drift / software ring oscillator (**H∞ = 7.288 — promoted**)
  - Cache coherence fabric ICE timing (H∞ = 0.991 — too deterministic)
  - Mach thread QoS scheduling (H∞ = 0.567 — scheduler too quantized)
  - GPU/Accelerate framework timing (H∞ = 3.573 — overlaps amx_timing)
  - Atomic CAS contention (**H∞ = 2.619 — promoted**)

### Improvements

- Both new sources added to `FAST_SOURCES` (27 fast sources total)
- `interleaved_frontier` composite now round-robins 8 standalone frontier sources
- Comprehensive documentation updates: SOURCE_CATALOG, README, CLAUDE.md, ARCHITECTURE, all docs
- Version bump to 0.4.0 across workspace, pyproject.toml
- Removed dead code (vdsp_timing.rs)
- Fixed stale source counts across all documentation and Cargo.toml files
- `cargo fmt` clean, zero clippy warnings, 212 tests passing

---

## 0.3.0 — 2026-02-12

### Complete Rust Rewrite

The entire project has been rewritten in Rust as a Cargo workspace with 5 crates:
`openentropy-core`, `openentropy-cli`, `openentropy-server`, `openentropy-tests`, and `openentropy-python`.

### Highlights
- **30 entropy sources** across 7 categories (timing, system, network, hardware, silicon, cross-domain, novel), all with SHA-256 conditioning
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
