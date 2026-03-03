---
title: 'CLI to SDK Mapping'
description: 'Which CLI capabilities are available in Python/Rust SDKs'
---

This page documents which CLI commands have SDK equivalents in Python and Rust.

## CLI ↔ SDK Capability Matrix

| CLI Command | Python SDK | Rust SDK | Notes |
|-------------|-----------|----------|-------|
| `scan` | ✅ `detect_available_sources()` | ✅ `detect_available_sources()` | Full parity |
| `bench` | ✅ `benchmark_sources()` | ✅ `benchmark_sources()` | Full parity |
| `analyze` | ✅ `full_analysis()`, `autocorrelation_profile()`, `spectral_analysis()` | ✅ | Partial — some analysis flags are CLI-only |
| `record` | ✅ `SessionWriter`, `record()` | ✅ `SessionWriter` | Full parity |
| `monitor` | ❌ | ❌ | CLI-only (TUI) — intentional |
| `stream` | ✅ `get_random_bytes()` | ✅ | Full parity |
| `compare` | ✅ `compare()` | ✅ | Full parity |
| `sessions` | ✅ `list_sessions()`, `load_session_meta()`, `load_session_raw_data()` | ✅ `list_sessions()`, `load_session_raw_data()` | Full parity |
| `server` | ❌ | ✅ | HTTP server is Rust-only — intentional |

## Intentionally CLI-Only

### monitor
TUI dashboard. Cannot be embedded in Python/Rust apps.
- **Use case**: Real-time visualization of entropy pool health
- **Alternative**: Use `pool.health_report()` in a loop for custom monitoring

## SDK-Only Capabilities

### server
HTTP entropy server — Rust-only, no Python bindings.
- **Use case**: Serve entropy over HTTP API
- **Why not Python**: Would require async runtime (tokio) in PyO3
