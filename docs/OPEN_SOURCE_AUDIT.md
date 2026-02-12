# Esoteric Entropy — Open-Source Readiness Audit

**Date:** 2026-02-12
**Auditor:** Sofi (AI agent) with Sunlover (human)
**Version:** 0.3.0
**License:** MIT — Amenti Labs

## Executive Summary

Esoteric Entropy is a Rust library that harvests entropy from 30 unconventional hardware sources on macOS (with Linux portability for ~15 sources). This audit verified open-source readiness across five dimensions:

| Area | Status | Notes |
|------|--------|-------|
| Python → Rust port | ✅ Complete | 18/18 Python sources have Rust equivalents |
| Legacy Python cleanup | ✅ Complete | Moved to `_python_legacy/`, gitignored |
| Raw entropy mode | ✅ Complete | Full stack: API, CLI, HTTP, PyO3 |
| Conditioning centralization | ✅ Complete | All SHA-256 removed from sources |
| Test suite | ✅ Passing | 79/79 tests pass, 0 clippy warnings |

## 1. Python → Rust Port Verification

All 18 original Python entropy sources have been ported to Rust:

| Python Source | Rust Equivalent | Status |
|---------------|-----------------|--------|
| `clock_jitter.py` | `timing.rs::ClockJitterSource` | ✅ |
| `mach_timing.py` | `timing.rs::MachTimingSource` | ✅ |
| `sleep_jitter.py` | `timing.rs::SleepJitterSource` | ✅ |
| `sysctl_monitor.py` | `sysctl.rs::SysctlDeltaSource` | ✅ |
| `vmstat_monitor.py` | `vmstat.rs::VmstatDeltaSource` | ✅ |
| `process_entropy.py` | `process.rs::ProcessTableSource` | ✅ |
| `dns_timing.py` | `network.rs::DnsTimingSource` | ✅ |
| `tcp_timing.py` | `network.rs::TcpConnectTimingSource` | ✅ |
| `disk_io.py` | `disk.rs::DiskIOSource` | ✅ |
| `memory_timing.py` | `memory.rs::MemoryTimingSource` | ✅ |
| `gpu_timing.py` | `gpu.rs::GpuTimingSource` | ✅ |
| `bluetooth_noise.py` | `bluetooth.rs::BluetoothNoiseSource` | ✅ |
| `audio_noise.py` | `audio.rs::AudioNoiseSource` | ✅ |
| `camera_noise.py` | `camera.rs::CameraNoiseSource` | ✅ |
| `wifi_signal.py` | `wifi.rs::WifiNoiseSource` | ✅ |
| `sensor_noise.py` | `sensor.rs::SensorNoiseSource` | ✅ |
| `compression_timing.py` | `compression.rs::CompressionTimingSource` | ✅ |
| `hash_timing.py` | `compression.rs::HashTimingSource` | ✅ |

**Additionally, 12 new sources were created in Rust with no Python equivalent:**
- Silicon: `dram_row_buffer`, `cache_contention`, `page_fault_timing`, `speculative_execution`
- Cross-domain: `cpu_io_beat`, `cpu_memory_beat`, `multi_domain_beat`
- Novel: `dispatch_queue`, `dyld_timing`, `vm_page_timing`, `spotlight_timing`
- System: `ioregistry`

**Total: 30 Rust sources (18 ported + 12 new)**

## 2. Legacy Python Cleanup

| Item | Status |
|------|--------|
| Legacy Python source files | Moved to `_python_legacy/` |
| `_python_legacy/` in `.gitignore` | ✅ |
| `esoteric_entropy/__init__.py` | Rust-only PyO3 wrapper (no Python imports) |
| Python infrastructure (pool, CLI, server) | All replaced by Rust crates |

The only remaining Python file in the release tree is `esoteric_entropy/__init__.py`, which is a thin PyO3 wrapper that imports the compiled Rust extension.

## 3. Raw (Unconditioned) Entropy Mode

Raw mode provides XOR-combined source bytes with no conditioning (no Von Neumann, no SHA-256).

| Interface | Implementation | Access |
|-----------|---------------|--------|
| Rust API | `pool.get_raw_bytes(n)` | Direct call |
| CLI stream | `--unconditioned` flag | `esoteric-entropy stream --unconditioned` |
| CLI device | `--unconditioned` flag | `esoteric-entropy device <name> --unconditioned` |
| HTTP server | `?raw=true` query param | Requires `--allow-raw` startup flag |
| Python SDK | `pool.get_raw_bytes(n)` | PyO3 binding |

**Security:** HTTP raw mode requires explicit `--allow-raw` flag to prevent accidental exposure.

## 4. Conditioning Centralization

**Before audit:** SHA-256 was called inside 12+ individual source files, making raw output impossible and causing double-conditioning in the pool.

**After audit:** All conditioning is centralized in `crates/esoteric-core/src/conditioning.rs`:

```rust
pub enum ConditioningMode {
    Raw,         // No processing
    VonNeumann,  // Debiasing only
    Sha256,      // Von Neumann + SHA-256 (default)
}

pub fn condition(data: &[u8], output_len: usize, mode: ConditioningMode) -> Vec<u8>
```

### Sources refactored (SHA-256 removed):

| Source File | What Was Removed |
|-------------|-----------------|
| `timing.rs` | SHA-256 from all 3 sources; Von Neumann from `mach_timing` |
| `sysctl.rs` | SHA-256 conditioning |
| `vmstat.rs` | SHA-256 conditioning |
| `process.rs` | SHA-256 conditioning |
| `ioregistry.rs` | SHA-256 conditioning |
| `disk.rs` | SHA-256 conditioning |
| `bluetooth.rs` | SHA-256 conditioning |
| `wifi.rs` | SHA-256 conditioning |
| `silicon.rs` | SHA-256 from all 4 sources |
| `cross_domain.rs` | SHA-256 from all 3 sources |
| `novel.rs` | SHA-256 from all 4 applicable sources |
| `compression.rs` | SHA-256 from `CompressionTimingSource` |

**Exception:** `compression.rs::HashTimingSource` retains SHA-256 because it is the *workload being timed*, not a conditioning step. The source hashes data and measures timing jitter — the SHA-256 is the entropy-generating operation itself.

## 5. Source Probe Results (Raw Output)

All 26 available sources probed on M4 Mac mini (2026-02-12):

| Source | Grade | Shannon (bits/byte) | Time |
|--------|-------|---------------------|------|
| dns_timing | A | 7.97 | 18.9s |
| tcp_connect_timing | A | 7.96 | 38.8s |
| gpu_timing | A | 7.96 | 37.0s |
| vm_page_timing | A | 7.85 | 0.11s |
| page_fault_timing | A | 7.80 | 0.02s |
| dyld_timing | A | 7.33 | 1.2s |
| spotlight_timing | A | 7.00 | 12.7s |
| memory_timing | A | 6.73 | 0.02s |
| clock_jitter | B | 6.28 | <0.001s |
| dispatch_queue | B | 6.05 | 0.13s |
| compression_timing | B | 5.31 | 0.13s |
| cache_contention | B | 3.96 | 0.03s |
| disk_io | C | 4.73 | 0.006s |
| bluetooth_noise | C | 4.43 | 10.2s |
| cpu_io_beat | C | 4.41 | 0.08s |
| process_table | C | 4.22 | 0.03s |
| dram_row_buffer | C | 3.09 | 0.006s |
| ioregistry | C | 3.08 | 2.0s |
| hash_timing | D | 3.13 | 0.02s |
| cpu_memory_beat | D | 2.77 | 0.01s |
| sleep_jitter | D* | 2.62 | 0.001s |
| multi_domain_beat | D | 2.60 | 0.007s |
| sysctl_deltas | D | 2.49 | 0.19s |
| vmstat_deltas | D | 2.11 | 0.48s |
| speculative_execution | F | 2.00 | 0.001s |
| mach_timing | F | 0.99 | <0.001s |

*Sleep_jitter oscillates between D and F across runs.

**4 sources unavailable on test machine:** audio_noise (ffmpeg), camera_noise (ffmpeg), wifi_noise (WiFi), sensor_noise (CoreMotion)

**Grade distribution:** 8A, 4B, 6C, 6D, 2F, 4 N/A

## 6. Test Suite

```
cargo test --workspace --exclude esoteric-python

  esoteric-core:   58 passed
  integration:      8 passed
  esoteric-tests:  12 passed
  doc-tests:        1 passed
  ─────────────────────────
  Total:           79 passed, 0 failed
```

**Clippy:** 0 warnings (`cargo clippy --workspace --exclude esoteric-python`)

## 7. Crate Structure

```
esoteric-entropy/
├── crates/
│   ├── esoteric-core/      # Library: sources, pool, conditioning
│   ├── esoteric-cli/       # CLI: scan, probe, stream, device, server
│   ├── esoteric-server/    # Axum HTTP server with raw mode
│   ├── esoteric-tests/     # NIST SP 800-22 test battery
│   └── esoteric-python/    # PyO3 bindings (excluded from tests on Python 3.14)
├── esoteric_entropy/
│   └── __init__.py          # Thin PyO3 wrapper
├── docs/
│   ├── OPEN_SOURCE_AUDIT.md # This file
│   ├── SOURCE_CATALOG.md    # All 30 sources with physics & grades
│   ├── CONDITIONING.md      # Conditioning architecture
│   ├── ARCHITECTURE.md      # System architecture
│   ├── API.md               # API reference
│   ├── PYTHON_SDK.md        # Python SDK usage
│   └── SOURCES.md           # Source overview
└── _python_legacy/          # Old Python code (gitignored)
```

## 8. Known Issues & Future Work

1. **PyO3 compatibility:** Python 3.14 > PyO3's max supported 3.13. Requires `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` env var. Builds excluded from default test/clippy runs with `--exclude esoteric-python`.

2. **Low-entropy raw sources:** `mach_timing` (1.0 bits/byte) and `speculative_execution` (2.0 bits/byte) have very low raw Shannon entropy. This is expected — the useful signal is in timing deltas, and raw byte packing doesn't preserve it well. These sources still contribute useful entropy to the XOR-combined pool.

3. **Slow sources:** `gpu_timing` (37s), `tcp_connect_timing` (39s), `dns_timing` (19s), and `spotlight_timing` (13s) are slow due to process spawning or network round-trips. The pool handles this gracefully — fast sources provide bulk entropy while slow sources contribute high-quality samples.

4. **Platform coverage:** 15 of 30 sources are macOS-only. Linux portability would require alternative implementations for: sysctl, vmstat, ioregistry, bluetooth, audio, camera, wifi, sensor, dispatch_queue, spotlight, and gpu sources.

## 9. Conclusion

The codebase is ready for open-source release:
- All Python sources ported, legacy code removed from release tree
- Raw entropy mode available across the full stack
- Conditioning properly centralized with clear separation of concerns
- 79 tests passing, 0 clippy warnings
- Comprehensive documentation (source catalog, conditioning architecture, audit report)
