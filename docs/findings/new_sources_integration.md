# New Entropy Sources — Integration & NIST Results

**Date:** 2026-02-11  
**Machine:** Mac Mini M4, macOS, Apple Silicon  
**Total new sources:** 14 (10 from breakthrough integration + 4 novel discoveries)

## Summary

| Source | Category | Quick Grade | NIST Score | NIST Passed | Shannon |
|--------|----------|:-----------:|:----------:|:-----------:|:-------:|
| spotlight_timing | Novel | A | 81.5 | 26/31 | 7.287 |
| compression_timing | Compression | A | 56.5 | 18/31 | 7.660 |
| cpu_io_beat | Cross-domain | A | 36.3 | 11/31 | 7.816 |
| speculative_execution | Silicon | C | 30.6 | 8/31 | 4.342 |
| vm_page_timing | Novel | B | 21.8 | 5/31 | 5.437 |
| dram_row_buffer | Silicon | D | 21.0 | 4/31 | 3.246 |
| cpu_memory_beat | Cross-domain | D | 21.0 | 6/31 | 3.086 |
| multi_domain_beat | Cross-domain | D | 16.9 | 5/31 | 3.385 |
| dispatch_queue | Novel | B | 16.1 | 3/31 | 5.620 |
| dyld_timing | Novel | B | 15.3 | 3/31 | 5.370 |
| cache_contention | Silicon | F | 10.5 | 3/31 | 1.800 |
| page_fault_timing | Silicon | D | 9.7 | 3/31 | 1.909 |
| hash_timing | Compression | D | 8.9 | 2/31 | 2.563 |
| ioregistry_deep | IORegistry | C | 8.1 | 2/31 | 3.583 |

## Key Findings

### 🏆 Star Performer: Spotlight Timing (81.5/100 NIST)
Spotlight metadata queries (`mdls`) produce the highest-quality entropy of any new source. The timing jitter comes from:
- Disk I/O for index lookups
- Background Spotlight indexer interference
- Filesystem cache state
- Subprocess launch overhead

**Trade-off:** Slow (~48s for 5000 samples due to subprocess per query).

### 🥈 Compression Timing (56.5/100 NIST)
zlib compression timing oracle is the best high-throughput source. Benefits from:
- Data-dependent branch prediction outcomes
- Hash table cache behavior
- Pipeline state variations

### Novel Source Discovery (Phase 2)

Probed 9 novel macOS subsystems. Results (Shannon bits/byte):

| Source | Shannon | Status |
|--------|:-------:|--------|
| Spotlight (mdls) | 6.97 | ✅ Integrated |
| Dispatch Queue (GCD) | 5.51 | ✅ Integrated |
| dyld/loader | 4.99 | ✅ Integrated |
| VM page (mmap) | 4.95 | ✅ Integrated |
| Pipe buffer | 1.98 | ❌ Too weak |
| kqueue/kevent | 1.48 | ❌ Too weak |
| Mach port | 1.45 | ❌ Too weak |
| xattr | 1.29 | ❌ Too weak |
| CoreGraphics | 0.00 | ❌ Unavailable |

## Architecture Notes

Raw LSB extraction alone isn't enough for NIST compliance — most sources need:
1. **XOR decorrelation** of consecutive deltas (implemented)
2. **Von Neumann debiasing** or SHA-256 conditioning (via EntropyPool)
3. **Mixing multiple sources** in the pool for defense in depth

These sources are designed as **raw entropy ingredients** for the EntropyPool mixer, not standalone RNGs.

## Source Files Created

- `openentropy/sources/silicon.py` — 4 classes (DRAM, cache, page fault, speculative)
- `openentropy/sources/ioregistry.py` — 1 class (deep IORegistry mining)
- `openentropy/sources/cross_domain.py` — 3 classes (CPU↔IO, CPU↔memory, multi-domain)
- `openentropy/sources/compression.py` — 2 classes (zlib timing, SHA-256 timing)
- `openentropy/sources/novel.py` — 4 classes (dispatch queue, dyld, VM page, Spotlight)

**Total package sources: 30** (up from 16)
