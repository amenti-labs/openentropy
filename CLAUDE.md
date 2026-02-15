# CLAUDE.md — OpenEntropy Developer Guide

## What This Is
OpenEntropy harvests hardware entropy from 44 unconventional sources on consumer devices. Rust workspace with Python bindings.

## Architecture
```
crates/
  openentropy-core/     # Core library: sources, pool, conditioning, min-entropy estimators
  openentropy-cli/      # CLI binary (clap): scan, bench, stream, monitor (ratatui TUI), etc.
  openentropy-server/   # HTTP server (axum): ANU-compatible API
  openentropy-tests/    # NIST SP 800-22 randomness test battery
  openentropy-python/   # PyO3/maturin Python bindings
```

## Research Applications
OpenEntropy serves dual purposes:

1. **Cryptographic entropy harvesting** — Pool, condition (SHA-256), and deliver high-quality random bytes via CLI, HTTP API, or Python SDK. All output is conservatively graded using NIST SP 800-90B min-entropy (H∞), not Shannon entropy.

2. **Raw signal research** — Each source can be sampled in `raw` conditioning mode for analysis of hardware nondeterminism, microarchitectural side channels, and cross-domain timing phenomena. The `research/poc/` directory contains independent C validation programs and a quality audit (`quality_audit.md`) with per-source H∞, autocorrelation, and cross-correlation measurements.

Key research artifacts:
- `research/poc/quality_audit.md` — Independent validation of all thermal/timing/microarch/composite sources
- `research/poc/validate_*.c` — 23 standalone C programs for reproducible entropy measurement
- `research/poc/thermal_*.c` — 7 thermal noise PoC programs (audio ADC, SMC, DRAM, denormal, PLL, USB, instruction)
- `research/poc/unprecedented_*.c/.m` — 8 unprecedented entropy PoC programs (convection, NVMe, ANE, GPU, PDN, IOSurface, quantum, fsync)
- `docs/findings/thermal_noise_research_2026-02-14.md` — Thermal noise research findings
- `docs/findings/unprecedented_entropy_2026-02-14.md` — Unprecedented entropy research findings
- `docs/SOURCE_CATALOG.md` — Physics explanations and grading for every source

## Build
```bash
cargo build --release                                          # all crates
cargo build --release -p openentropy-cli                       # just CLI
cargo test --workspace --exclude openentropy-python             # tests (skip Python, needs maturin)
cargo clippy --workspace --exclude openentropy-python           # lint
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop --release # Python bindings
```

Note: `.cargo/config.toml` sets `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` automatically for cargo commands.

## Key Design Decisions
- **Sources never self-condition.** All conditioning goes through `crates/openentropy-core/src/conditioning.rs`.
- **Three conditioning modes:** Raw (passthrough), VonNeumann (debias only), Sha256 (full, default).
- **Fast sources by default.** CLI commands use 34 fast sources (<2s). `--sources all` for everything.
- **Min-entropy (H∞) over Shannon.** Grading based on NIST SP 800-90B min-entropy, not Shannon which overestimates.
- **265 tests, 0 clippy warnings.** Keep it that way.

## Source Categories
- **Thermal** (3): denormal_timing, audio_pll_timing, pdn_resonance
- **Timing** (7): clock_jitter, mach_timing, memory_timing, dram_row_buffer, cache_contention, page_fault_timing, vm_page_timing
- **Scheduling** (3): sleep_jitter, dispatch_queue, thread_lifecycle
- **IO** (4): disk_io, nvme_latency, usb_timing, fsync_journal
- **IPC** (4): mach_ipc, pipe_buffer, kqueue_events, keychain_timing
- **Microarch** (5): speculative_execution, dvfs_race, cas_contention, tlb_shootdown, amx_timing
- **GPU** (3): gpu_timing, gpu_divergence, iosurface_crossing
- **Network** (3): dns_timing, tcp_connect_timing, wifi_rssi
- **System** (4): sysctl_deltas, vmstat_deltas, process_table, ioregistry
- **Composite** (2): cpu_io_beat, cpu_memory_beat
- **Signal** (3): compression_timing, hash_timing, spotlight_timing
- **Sensor** (3): audio_noise, camera_noise, bluetooth_noise

## Adding a New Source
1. Create `crates/openentropy-core/src/sources/your_source.rs`
2. Implement the `EntropySource` trait
3. Register in `crates/openentropy-core/src/sources/mod.rs`
4. Add platform detection in `crates/openentropy-core/src/platform.rs`
5. Add to `FAST_SOURCES` in `crates/openentropy-cli/src/commands/mod.rs` if <2s
6. Document physics in `docs/SOURCE_CATALOG.md`

## Testing
```bash
cargo test                              # unit tests (fast, no hardware)
cargo test -- --ignored                 # hardware-dependent tests (may hang)
```

54 tests are `#[ignore]` because they require specific hardware (camera, BLE, WiFi, etc.).

## Platform
Primary: macOS Apple Silicon (M1-M4). 39/44 sources available on Mac Mini, 44/44 on MacBook.
Linux: 10-15 sources (timing, network, disk, process). No macOS-specific sources.
