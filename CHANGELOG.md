# Changelog

## 0.8.0 — 2026-02-27

### Added

- **`record --all`** — Record from every available source with a single flag, matching `bench`, `analyze`, and `stream`. Positional source names remain supported.
- **TUI multiselect recording** — Select multiple sources with `Space`/`Enter` (yellow = selected), then press `r` to record from all selected sources simultaneously. Recording uses 1000-byte samples for throughput regardless of display sample size.
- **TUI help modal** — Press `?` for a full keybinding reference overlay.
- **Quantum category** — New `quantum` source category with QCicada QRNG source. Live mode switching (`m` key) cycles between raw/sha256/samples post-processing modes.
- **TUI category grouping** — Sources organized into collapsible category groups with `{`/`}` jump navigation and `C` fold all.
- **14 new entropy sources** across timing, scheduling, network, signal, and microarch categories.

### Changed

- **Source directory reorganization** — 63 sources moved from flat layout into 13 category-based subdirectories (`gpu/`, `io/`, `ipc/`, `microarch/`, `network/`, `quantum/`, `scheduling/`, `sensor/`, `signal/`, `system/`, `thermal/`, `timing/`).
- **Pool batched parallel collection** for improved throughput.
- **TUI source selection unified** — `Space`/`Enter` toggles sources on/off (selected = yellow, active = yellow+bold+`●`). No separate mark/select distinction.
- Source count increased from 45 to 63.

### Removed

- **TUI compare overlay** (`Tab`) — Removed non-functional compare feature. The compare source never received live data; only showed stale history.

### Fixed

- **SIGILL crash from undocumented register probes** — Added `probe_jit_instruction_safe()` fork-based probe for `gxf_register_timing` (S3\_6\_c15\_c1\_5) and `dual_clock_domain` (S3\_1\_c15\_c0\_6, S3\_4\_c15\_c10\_5). These undocumented Apple Silicon registers may not be accessible on all chips/OS versions; the fork probe prevents process-killing SIGILL.
- **cargo fmt** — Applied formatting to all source files.
- **Source extraction pipeline fixes** — Corrected entropy extraction in `dual_clock_domain` (phase data, not timing deltas), `mach_timing` (raw truncation → proper pipeline), `preemption_boundary` (sparse events), `amx_timing` (deterministic workload → LCG randomization), `clock_jitter` (fake entropy from epoch bytes), `dns_timing`/`tcp_connect` (weak LSB extraction), `smc_highvar_timing`/`getentropy_timing` (cross-platform compatibility).
- **Invalid entropy rate estimates** — `ane_timing` (1800→3.0), `nvme_iokit_sensors` (1500→3.0).
- **Slow source timeouts** — `nl_inference_timing` and `spotlight_timing` now reliably complete within pool budget.

## 0.7.0 — 2026-02-23

### Changed

- **Pool collection no longer blocks health reads.** `SourceState.source` changed from `Box<dyn EntropySource>` to `Arc<dyn EntropySource>`. The pool mutex is now released during `collect()` calls, allowing `health_report()` and TUI updates to proceed without blocking on slow sources.
- `run_server` now returns `std::io::Result<()>` instead of panicking on bind/serve failures. Server CLI handles `Runtime::new()` failure gracefully.
- Server `SourceEntry` and pool status API now include `min_entropy` field.
- Deduplicated CoreAudio FFI bindings into shared `coreaudio_ffi` module used by `audio_pll_timing` and `counter_beat` (~160 lines removed).

### Removed

- **`memory_timing`** — true duplicate of `vm_page_timing` (identical mmap→write_volatile→munmap pattern). Use `vm_page_timing` instead.
- **`gpu_timing`** — strictly inferior to `gpu_divergence` (~47s via sips subprocess vs ~0.8s native Metal compute, same GPU clock domain). Use `gpu_divergence` instead.
- Source count reduced from 47 to 45. All documentation and metadata updated.

### Fixed

- **Mach IPC port lifecycle** — Fixed use-after-free (`mach_port_deallocate` before `mach_port_mod_refs` on same port), send right leak on cleanup, and `MachMsgOOLDescriptor` struct field order (`type` and `pad` were swapped vs XNU kernel layout).
- **CoreAudio constant** — `AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE` was `0x61737264` ('asrd'), corrected to `0x61737274` ('asrt') per Apple SDK headers.
- **Signal handler safety** — `stream.rs` FIFO cleanup now pre-computes `CString` into a `OnceLock` and uses `libc::unlink`/`libc::_exit` instead of heap-allocating Rust stdlib functions inside the signal handler.
- **Network source runaway** — DNS and TCP entropy sources now have iteration caps to prevent infinite loops when all targets are unreachable.
- **Install script** — `VERSION` and `ASSET_NAME` were computed before `LATEST` was fetched, producing empty values.
- **Platform metadata consistency** — `process` corrected to `Platform::MacOS` (uses `/bin/ps`); `pipe_buffer` and `tlb_shootdown` `is_available()` aligned with `Platform::MacOS`; `vm_page_timing` corrected to `Platform::Any` (pure POSIX `mmap`).
- **Composite flag** — `cpu_io_beat` and `cpu_memory_beat` now correctly declare `composite: true`.
- **Binary paths** — `sysctl`, `uptime`, `vm_stat`, `netstat` now use full absolute paths (`/usr/sbin/sysctl`, etc.) in session and telemetry modules.
- **Documentation** — Corrected stale version references, source counts, CLI command counts, and code examples across SECURITY.md, CONTRIBUTING.md, ARCHITECTURE.md, README.md, CONDITIONING.md, API.md, and per-crate READMEs. Renumbered SOURCES.md catalog (1-45 sequential, was 1-50 with gaps).

## 0.6.0 — 2026-02-22

### Added

- Telemetry system (`telemetry_v1`): `TelemetrySnapshot`, `TelemetryWindowReport`, and standalone `telemetry` CLI command with `--window-sec` support.
- Shannon entropy and min-entropy fields in `SourceAnalysis`.
- Server `/sources` and `/pool/status` endpoints now accept `?telemetry=true` for start/end telemetry reports.
- 20 frontier entropy sources exploiting previously-unharvested hardware nondeterminism (AMX timing, TLB shootdown, pipe buffer contention, Mach IPC, kqueue events, DVFS race, CAS contention, Keychain timing, audio PLL, counter beat, display PLL, PCIe PLL, and more).

### Changed

- Merged `device` command into `stream --fifo` and `report` command into `analyze --report`.
- Improved bench scoring: graduated reliability penalty, stability index edge-case fix, entropy-gated source interpretation.
- DRY refactoring: extracted `filter_sources`, `print_cross_correlation`, `write_json`, `unix_timestamp_now`, `parse_conditioning` into shared module.
- TUI chart mode now persists across source switches.

## 0.5.1 — 2026-02-18

### Changed

- Added a Python source-parity CI check to ensure Python bindings expose the same detected source set as the Rust pool.
- Added dedicated package metadata docs:
  - `README.pypi.md` for PyPI
  - per-crate `README.md` files for crates.io rendering

### Fixed

- Updated release workflow PyPI publish command to valid `maturin publish` arguments.
- Expanded Python bindings parity with Rust core:
  - added missing metadata fields in health/source reports
  - added `source_names`, `get_source_bytes`, `get_source_raw_bytes`
  - added `platform_info`, `detect_machine_info`, and conditioning/quality helper exports
  - invalid conditioning mode now raises `ValueError` instead of silently defaulting
- Refreshed documentation to match current code paths and packaging:
  - rewritten `docs/PYTHON_SDK.md`
  - updated `docs/API.md` and `docs/ARCHITECTURE.md`
  - corrected Python install guidance and source-count references

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
