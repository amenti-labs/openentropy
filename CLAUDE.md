# CLAUDE.md — OpenEntropy Developer Guide

## What This Is
OpenEntropy harvests hardware entropy from 36 unconventional sources on consumer devices. Rust workspace with Python bindings.

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
- `research/poc/quality_audit.md` — Independent validation of all frontier/novel/silicon/cross-domain sources
- `research/poc/validate_*.c` — 23 standalone C programs for reproducible entropy measurement
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
- **Fast sources by default.** CLI commands use 26 fast sources (<2s). `--sources all` for everything.
- **Min-entropy (H∞) over Shannon.** Grading based on NIST SP 800-90B min-entropy, not Shannon which overestimates.
- **257 tests, 0 clippy warnings.** Keep it that way.

## Source Categories
- **Timing** (3): clock_jitter, mach_timing, sleep_jitter
- **System** (4): sysctl_deltas, vmstat_deltas, process_table, ioregistry
- **Network** (2): dns_timing, tcp_connect_timing
- **Hardware** (7): disk_io, memory_timing, gpu_timing, bluetooth_noise, audio_noise, camera_noise, wifi_rssi
- **Silicon** (4): dram_row_buffer, cache_contention, page_fault_timing, speculative_execution
- **Cross-Domain** (2): cpu_io_beat, cpu_memory_beat
- **Novel** (5): compression_timing, hash_timing, dispatch_queue, vm_page_timing, spotlight_timing
- **Frontier** (9): amx_timing, thread_lifecycle, mach_ipc, tlb_shootdown, pipe_buffer, kqueue_events, dvfs_race, cas_contention, keychain_timing

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

41 tests are `#[ignore]` because they require specific hardware (camera, BLE, WiFi, etc.).

## Platform
Primary: macOS Apple Silicon (M1-M4). 31/36 sources available on Mac Mini, 36/36 on MacBook.
Linux: 10-15 sources (timing, network, disk, process). No macOS-specific sources.
