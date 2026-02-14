# Critical Validation Results — 2026-02-14

## Methodology
- All tests run on Mac Mini M4 (Apple Silicon)
- validate_dmp.c: 5 tests, up to 100K samples
- validate_keychain.c: 7 tests, up to 10K samples
- cross_correlation.c: 5-way Pearson correlation matrix, 5K samples each

---

## DMP Confusion — HONEST ASSESSMENT

### Verdict: WEAK SOURCE — Needs reclassification or removal

### Problems Found:

**1. H∞ dropped significantly at 100K samples**
- Initial 50K measurement: H∞ = 3.6 (XOR-adj fold)
- 100K validation: H∞ = 2.04 (XOR-fold), 2.16 (delta XOR-fold)
- Stability trials (10 × 10K): Mean H∞ = 1.73, range 1.50–1.90
- The initial H∞ = 3.6 was measured with a specific extraction method (XOR-adjacent)
  that inflates entropy estimates. The more conservative XOR-fold and delta methods
  consistently show H∞ ≈ 1.7–2.1

**2. Significantly correlated with cache_contention (r = 0.17)**
- Cross-correlation matrix shows r = 0.1711 between dmp_confusion and cache_contention
- This is flagged as "weak correlation" — they share some entropy domain
- DMP confusion involves memory loads from a 16MB array, which inherently causes
  cache misses. A substantial fraction of the measured timing variance comes from
  cache behavior, not DMP prediction failures specifically

**3. DMP is NOT the primary entropy driver**
- Test 4 (DMP vs plain cache miss): H∞ difference is only +0.24 bits
  in favor of DMP, with Pearson r = 0.41 (significantly correlated)
- Test 5 (sequential vs random): Random access shows LOWER H∞ (-0.12)
  than sequential with pointer chasing. This is the opposite of what
  we'd expect if DMP confusion were the entropy source
- The entropy comes primarily from **random memory access latency** (cache
  misses, DRAM bank conflicts), NOT from DMP prediction failures

**4. GoFetch paper confirms DMP is DETERMINISTIC**
- The GoFetch attack succeeds precisely because DMP behavior is predictable
- DMP activates deterministically when it sees pointer-like values
- An adversary who controls memory contents can predict DMP activation
- This undermines the claim that DMP prediction failures are nondeterministic

**5. Autocorrelation is borderline**
- lag-1: 0.091 (just under 0.1 threshold, but concerning)
- Samples are not fully independent — adjacent measurements share
  some cache/memory state

### What this source ACTUALLY is:
It's a **memory timing source** that happens to trigger DMP as a side effect.
The entropy comes from random memory access latency across a 16MB array —
cache misses, DRAM bank conflicts, memory controller queuing. This is similar
to what `cache_contention` and `memory_timing` already measure.

### Recommendation:
**RENAME to `memory_random_walk` or REMOVE.** It does not genuinely exploit
DMP as an independent entropy domain. The correlation with cache_contention
(r = 0.17) confirms shared entropy. If kept, it should be classified honestly
as a memory timing source, not as a DMP source, and its entropy_rate_estimate
should be lowered from 3000 to ~1700.

---

## Keychain Timing — HONEST ASSESSMENT

### Verdict: STRONG SOURCE — Genuine and novel, but with caveats

### Strengths:

**1. Exceptionally high entropy that holds up at scale**
- 10K samples: H∞ = 7.533 (near-maximum 8.0)
- Stability across 10 trials: Mean H∞ = 6.91, range 6.57–7.06
- This is the highest-entropy source in the entire project
- Even the lowest trial (H∞ = 6.57) exceeds all other sources

**2. Genuinely independent from existing sources**
- Pearson correlation with mach_ipc: r = 0.051 (independent!)
- Pearson correlation with cache_contention: r = -0.003 (independent!)
- Pearson correlation with tlb_shootdown: r = 0.027 (independent!)
- Pearson correlation with dmp_confusion: r = -0.042 (independent!)
- The keychain path traverses domains (securityd, SEP, APFS) that none
  of our existing sources touch

**3. H∞ advantage over IPC alone: +4.2 bits**
- mach_ipc: H∞ = 2.52
- keychain_timing: H∞ = 6.72
- The keychain adds 4+ bits of entropy beyond what IPC scheduling provides
- This confirms the entropy comes from the full stack (securityd + SEP + disk),
  not just from IPC scheduling noise

**4. No concerning audit/side-effect issues**
- Creates one keychain item on setup, deletes on cleanup
- No Keychain Access prompts (item is created by us, not a third-party credential)
- No disk writes per read (only initial add + final delete)
- Orphan item on crash is harmless

### Problems Found:

**1. SEVERE autocorrelation (lag-1: r = 0.43)**
- lag-1: 0.4279 — this is very high
- lag-2: 0.2455 — still high
- Decays slowly through lag-10 (0.1185)
- Adjacent samples are NOT independent. This means the EFFECTIVE entropy
  per sample is lower than the byte-level H∞ suggests
- The raw timing values contain serial dependency — when one keychain query
  is slow (e.g., due to securityd scheduling), the next one tends to also
  be slow (securityd is still under load)
- This does NOT invalidate the source, but means we must use extraction
  methods that account for serial correlation (variance extraction helps)

**2. Warm-up effect / caching**
- First 2000 samples: Mean = 23,487 ticks
- Remaining samples: Mean ≈ 14,300 ticks (40% faster)
- H∞ drops from 6.97 to 6.64 (still excellent, but there's drift)
- securityd likely caches the database path after the first few queries
- The warm-up effect is a one-time transient; steady-state H∞ ≈ 6.6–7.0

**3. Performance: slow for large collections**
- 0.60 ms per sample
- ~418 entropy bytes/sec after extraction
- 64 bytes requires ~153 ms, 256 bytes requires ~612 ms
- This is the slowest source in the project (2x slower than audio_noise)
- Should NOT be in the fast sources list

**4. The SEP claim may be overstated**
- We can't prove the read path actually touches the Secure Enclave
- securityd may handle reads entirely in userspace with cached keys
- The entropy likely comes from: XPC IPC scheduling + securityd process
  scheduling + SQLite database I/O, not from the SEP hardware itself
- The physics description should be updated to be more conservative

### Recommendation:
**KEEP, but with corrections:**
1. Fix the physics description — don't claim SEP for the read path
2. Lower entropy_rate_estimate from 7000 to ~6500 to reflect autocorrelation
3. Do NOT add to FAST_SOURCES (already correctly excluded)
4. Add a warm-up phase: discard the first ~500 samples in collect()
5. Variance extraction (which we already use) helps with autocorrelation

---

## Cross-Correlation Summary

| Pair | Pearson r | Verdict |
|------|-----------|---------|
| dmp_confusion × cache_contention | **0.171** | Weak correlation — shared cache domain |
| dmp_confusion × keychain | -0.042 | Independent |
| dmp_confusion × mach_ipc | 0.018 | Independent |
| dmp_confusion × tlb_shootdown | 0.015 | Independent |
| keychain × cache_contention | -0.003 | Independent |
| keychain × mach_ipc | -0.003 | Independent |
| keychain × tlb_shootdown | 0.027 | Independent |
| cache_contention × mach_ipc | 0.012 | Independent |
| cache_contention × tlb_shootdown | 0.049 | Independent |
| mach_ipc × tlb_shootdown | 0.005 | Independent |

Key finding: **keychain_timing is genuinely independent from ALL existing sources.**
dmp_confusion shares entropy domain with cache_contention.

---

## Final Honest Recommendation

### keychain_timing: KEEP (with corrections)
- Genuinely novel entropy domain (securityd IPC + database I/O)
- Independent from all existing sources (all r < 0.05)
- Incredibly high H∞ (6.5–7.5) that holds up under scrutiny
- Autocorrelation is the main concern but variance extraction mitigates it
- Performance (0.6ms/sample) is acceptable for a slow source

### dmp_confusion: RENAME OR REMOVE
- Not actually a DMP source — it's a memory random walk timing source
- Correlated with cache_contention (r = 0.17)
- H∞ at scale is 1.7, not 3.6 (initial measurement was inflated)
- DMP is deterministic (GoFetch confirms), so the "prediction failure"
  framing is scientifically inaccurate
- If kept, should be renamed and its entropy_rate_estimate halved
- Questionable whether it adds enough value beyond existing memory sources
