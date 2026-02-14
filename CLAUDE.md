# CLAUDE.md — OpenEntropy Developer Guide

## What This Is
OpenEntropy harvests hardware entropy from 39 unconventional sources on consumer devices. Rust workspace with Python bindings.

## Architecture
```
crates/
  openentropy-core/     # Core library: sources, pool, conditioning, min-entropy estimators
  openentropy-cli/      # CLI binary (clap): scan, bench, stream, monitor (ratatui TUI), etc.
  openentropy-server/   # HTTP server (axum): ANU-compatible API
  openentropy-tests/    # NIST SP 800-22 randomness test battery
  openentropy-python/   # PyO3/maturin Python bindings
```

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
- **Fast sources by default.** CLI commands use 27 fast sources (<2s). `--sources all` for everything.
- **Min-entropy (H∞) over Shannon.** Grading based on NIST SP 800-90B min-entropy, not Shannon which overestimates.
- **212 tests, 0 clippy warnings.** Keep it that way.

## Source Categories
- **Timing** (3): clock_jitter, mach_timing, sleep_jitter
- **System** (4): sysctl_deltas, vmstat_deltas, process_table, ioregistry
- **Network** (2): dns_timing, tcp_connect_timing
- **Hardware** (6): disk_io, memory_timing, gpu_timing, bluetooth_noise, audio_noise, camera_noise
- **Silicon** (4): dram_row_buffer, cache_contention, page_fault_timing, speculative_execution
- **Cross-Domain** (3): cpu_io_beat, cpu_memory_beat, multi_domain_beat
- **Novel** (6): compression_timing, hash_timing, dispatch_queue, dyld_timing, vm_page_timing, spotlight_timing
- **Frontier** (9): amx_timing, thread_lifecycle, mach_ipc, tlb_shootdown, pipe_buffer, kqueue_events, dvfs_race, cas_contention, interleaved_frontier

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

29 tests are `#[ignore]` because they require specific hardware (camera, BLE, WiFi, etc.).

## Platform
Primary: macOS Apple Silicon (M1-M4). 35/39 sources available on Mac Mini, 39/39 on MacBook.
Linux: 10-15 sources (timing, network, disk, process). No macOS-specific sources.
