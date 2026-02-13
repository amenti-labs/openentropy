# Code Review — 2026-02-12

## Summary

Comprehensive review of the OpenEntropy Rust codebase covering DRY violations, Rust best practices, composability, and code organization. All issues found were fixed.

## Changes Made

### 1. DRY Violations Fixed

#### `mach_absolute_time` FFI + `mach_time()` helper (3 copies → 1)
- **Before:** Duplicated in `timing.rs`, `silicon.rs`, and `cross_domain.rs` — each with its own FFI declaration, `#[cfg]` blocks, and fallback implementations (with slightly different fallback strategies: thread-local vs OnceLock).
- **After:** Single canonical implementation in new `sources/helpers.rs` module. Uses `OnceLock`-based fallback on non-macOS.
- **Bug fixed:** `timing.rs` had a bare `unsafe extern "C"` declaration **without `#[cfg(target_os = "macos")]`**, meaning it would fail to link on non-macOS platforms.

#### `extract_lsbs_u64()` (2 identical copies → 1)
- **Before:** Identical function in `novel.rs` and `cross_domain.rs`, both with `#[allow(dead_code)]`.
- **After:** Single implementation in `sources/helpers.rs`.

#### `extract_lsbs()` for i64 (ioregistry.rs → shared helper)
- **Before:** Separate `extract_lsbs(deltas: &[i64])` in `ioregistry.rs`, functionally identical to the u64 version.
- **After:** `extract_lsbs_i64()` in `sources/helpers.rs`.

### 2. New Module: `sources/helpers.rs`
Shared primitives for entropy source implementations:
- `mach_time()` — cross-platform high-resolution timestamp
- `extract_lsbs_u64()` — LSB extraction from u64 timing deltas
- `extract_lsbs_i64()` — LSB extraction from i64 deltas
- Full test coverage for all helpers

### 3. Clippy Warnings Fixed (4 → 0)
- `openentropy-core`: Removed unnecessary `let` binding in `pool.rs` `get_bytes()` (let_and_return)
- `openentropy-cli`: Replaced `&[name.clone()]` with `std::slice::from_ref(&name)` (cloned_ref_to_slice_refs)
- `openentropy-cli`: Replaced `format!("  watching: ")` with string literal (useless_format)
- `openentropy-cli`: Removed unused `cursor_name()` method (dead_code)

### 4. Test-Only Imports Cleaned Up
Moved `extract_lsbs_*` imports from module scope into `#[cfg(test)] mod tests` blocks in `cross_domain.rs`, `novel.rs`, and `ioregistry.rs` since they were only used in tests.

## Assessment

### What's Good
- **Trait design:** `EntropySource` is clean and minimal — easy to implement custom sources
- **Crate boundaries:** Clean separation (core, cli, server, python, tests)
- **Conditioning architecture:** Single gateway pattern in `conditioning.rs` is excellent
- **Pool composability:** `get_bytes()` with `ConditioningMode` lets users choose their trade-off
- **Documentation:** Physics explanations per source are outstanding
- **Error handling in pool:** `catch_unwind` around source collection is smart for untrusted sources

### Acceptable Trade-offs
- **`unwrap()` on Mutex locks in pool.rs:** These are correct — a poisoned Mutex indicates a panic in another thread, and propagating that panic is the right behavior for an entropy pool
- **`unsafe` blocks:** All necessary for the low-level hardware timing that makes this project unique (mach_absolute_time FFI, volatile reads for cache timing, mmap for page faults). Well-isolated and justified.
- **No `thiserror`/custom error types:** Sources return `Vec<u8>` (empty on failure) rather than `Result`. This is actually a good design choice for entropy sources — partial/empty data is handled gracefully by the pool, and error variants would add complexity without value.

### Future Considerations (not blocking)
- `getrandom` function in pool.rs reads `/dev/urandom` directly — could use the `getrandom` crate for cross-platform support
- `Command::new` patterns across sources (ioreg, sysctl, vm_stat, etc.) could benefit from a shared "run command and get output" helper, but the commands are different enough that abstraction might hurt readability
