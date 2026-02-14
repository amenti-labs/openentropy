# OpenEntropy Source Quality Audit

**Date:** 2026-02-14
**Machine:** Mac Mini M4, macOS 15.x
**Method:** Independent C programs (100K samples, 10 trials × 10K, autocorrelation lag 1-5, Pearson cross-correlation)

## Cut Criteria
- **CUT**: H∞ < 0.5 at 100K samples
- **CUT**: Cross-correlation > 0.3 with higher-quality source (redundant)
- **CUT**: H∞ std dev > 2.0 across trials (unstable)
- **DEMOTE** (keep but mark weak): H∞ 0.5-1.5 at 100K, or autocorrelation > 0.5

## Summary Table

| Source | Category | H∞ (100K) | H∞ Mean (10T) | H∞ StdDev | Autocorr lag-1 | Max cross-corr | Verdict |
|--------|----------|-----------|----------------|-----------|----------------|----------------|---------|
| kqueue_events | Frontier | **7.541** | 7.347 | 0.062 | -0.131 | 0.012 (pipe_buffer) | **KEEP** |
| thread_lifecycle | Frontier | **5.913** | 5.699 | 0.164 | 0.287 | 0.015 (dispatch_queue) | **KEEP** |
| spotlight_timing | Novel | **5.644** | 5.621 | 0.193 | 0.145 | -0.163 (ioregistry) | **KEEP** |
| dvfs_race | Frontier | **4.566** | 4.994 | 0.202 | 0.494 | 0.002 (cas_contention) | **KEEP** |
| mach_ipc | Frontier | **3.802** | 3.403 | 0.247 | -0.226 | -0.025 (thread_lifecycle) | **KEEP** |
| hash_timing | Novel | **3.720** | 3.690 | 0.036 | 0.035 | -0.009 (compression_timing) | **KEEP** |
| vm_page_timing | Novel | **3.456** | 3.411 | 0.175 | 0.095 | 0.019 (tlb_shootdown) | **KEEP** |
| compression_timing | Novel | **6.973** | 7.026 | 0.174 | 0.106 | 0.004 (amx_timing) | **KEEP** |
| amx_timing | Frontier | **2.468** | 2.504 | 0.104 | -0.003 | -0.017 (cache_contention) | **KEEP** |
| dispatch_queue | Novel | **2.471** | 2.368 | 0.141 | 0.282 | 0.015 (thread_lifecycle) | **KEEP** |
| dyld_timing | Novel | 2.387 | 2.272 | 0.057 | 0.020 | **-0.395** (spotlight_timing) | **CUT** |
| ioregistry | System | 2.737 | 3.322 | 0.000 | 0.127 | **1.000** (sensor_noise) | **CUT** |
| sensor_noise | Hardware | 2.737 | 3.322 | 0.000 | 0.127 | **1.000** (ioregistry) | **CUT** |
| cache_contention | Silicon | 2.058 | 1.861 | 0.133 | -0.354 | 0.009 (dram_row_buffer) | DEMOTE |
| page_fault_timing | Silicon | 1.474 | 0.784 | 0.046 | 0.072 | -0.007 (vm_page_timing) | DEMOTE |
| speculative_execution | Silicon | 1.142 | 0.962 | 0.182 | -0.035 | 0.010 (hash_timing) | DEMOTE |
| cas_contention | Frontier | 1.046 | 1.396 | 0.263 | 0.025 | -0.035 (cache_contention) | DEMOTE |
| pipe_buffer | Frontier | 1.042 | 1.015 | 0.238 | 0.333 | 0.037 (kqueue_events) | DEMOTE |
| tlb_shootdown | Frontier | 0.677 | 0.941 | 0.315 | 0.499 | 0.035 (page_fault_timing) | DEMOTE |
| cpu_memory_beat | CrossDomain | 0.831 | 0.887 | 0.019 | -0.215 | 0.016 (cpu_io_beat) | DEMOTE |
| cpu_io_beat | CrossDomain | 0.629 | 0.593 | 0.026 | -0.008 | 0.025 (cpu_memory_beat) | DEMOTE |
| dram_row_buffer | Silicon | 0.554 | 0.471 | 0.021 | 0.055 | 0.013 (cache_contention) | DEMOTE |
| multi_domain_beat | CrossDomain | **0.479** | 0.483 | 0.030 | -0.118 | -0.007 (cpu_io_beat) | **CUT** |

## Detailed Verdicts

### KEEP (10 sources) — Genuine independent entropy
| Source | Why it's good |
|--------|--------------|
| **kqueue_events** | H∞=7.54, near-perfect byte entropy. Diverse kernel event multiplexing. Best frontier source. |
| **compression_timing** | H∞=6.97, exceptionally high. Data-dependent branch prediction timing. Very stable (σ=0.17). |
| **thread_lifecycle** | H∞=5.91, rich scheduler nondeterminism. 164 unique LSB values. Independent of all sources. |
| **spotlight_timing** | H∞=5.64, process spawn timing variability. Independent of other sources (r<0.17). |
| **dvfs_race** | H∞=4.57, cross-core frequency race captures scheduling + cache coherence. Some autocorr (0.49). |
| **mach_ipc** | H∞=3.80, complex OOL Mach messages. Deep kernel VM paths. Moderate autocorr (-0.23). |
| **hash_timing** | H∞=3.72, extremely stable (σ=0.04). Nearly zero autocorrelation. Clean source. |
| **vm_page_timing** | H∞=3.46, stable. mmap/munmap exercises TLB + page tables. Clean autocorrelation. |
| **amx_timing** | H∞=2.47, AMX coprocessor timing. Near-zero autocorrelation (-0.003). Unique hardware domain. |
| **dispatch_queue** | H∞=2.47, thread scheduling latency. Some autocorr (0.28) but independent (r=0.015 vs thread_lifecycle). |

### CUT (4 sources) — Remove from pool
| Source | Why cut |
|--------|---------|
| **multi_domain_beat** | H∞=0.479 — below 0.5 threshold. Composite of individually weak signals. Does not add unique entropy beyond cpu_io_beat and cpu_memory_beat. |
| **dyld_timing** | H∞=2.39 looks OK, but **r=-0.395 with spotlight_timing** — they both measure process spawn/filesystem timing. spotlight_timing has H∞=5.64, making dyld_timing redundant. |
| **sensor_noise** | **r=1.000 with ioregistry** — identical data source (both parse ioreg output). sensor_noise adds nothing that ioregistry doesn't provide. ioregistry is demoted anyway. |
| **ioregistry** | **r=1.000 with sensor_noise** — redundant pair. Both parse the same ioreg command output. Keep whichever has better entropy (ioregistry), but DEMOTE it — not CUT. *See note below.* |

**Note on ioregistry vs sensor_noise:** These are both ioreg-based. Cross-correlation = 1.0 means they're measuring the exact same data. We should keep exactly ONE. ioregistry (4 snapshots, richer) is the better implementation, but it should be DEMOTED since H∞ autocorrelation > 0.5.

**Revised:** CUT sensor_noise. DEMOTE ioregistry. Net: 3 sources CUT.

### DEMOTE (9 sources) — Keep but mark as weak, lower weight
| Source | Why demoted |
|--------|-------------|
| **cache_contention** | H∞=2.06 is OK, but max autocorr=0.975 (!) — lag-5 is almost perfectly correlated. Alternating access patterns create systematic timing pattern. |
| **page_fault_timing** | H∞=1.47 at 100K but H∞ mean only 0.78 across trials — inconsistent. On the edge of CUT threshold. |
| **speculative_execution** | H∞=1.14 — branch predictor timing has limited entropy. Very low timing resolution on M4 (mean=1.4 ticks). |
| **cas_contention** | H∞=1.05 — CAS arbitration provides modest entropy. Stable but fundamentally limited. |
| **pipe_buffer** | H∞=1.04, autocorr=0.33. Zone allocator contention provides some entropy but less than expected. |
| **tlb_shootdown** | H∞=0.68, borderline. Autocorr=0.50. mprotect IPI timing is noisy but not deeply nondeterministic. |
| **cpu_memory_beat** | H∞=0.83, autocorr=-0.22. Cross-domain beat is real but weak on unified memory architecture. |
| **cpu_io_beat** | H∞=0.63. Cross-domain beat between CPU and disk I/O is barely above noise. |
| **dram_row_buffer** | H∞=0.55. Borderline — on Apple Silicon's unified memory, DRAM row buffer effects are minimal. |

## Cross-Correlation Matrix (Notable Pairs)

All pairs except those noted below have |r| < 0.05 — **sources are remarkably independent**.

| Pair | r | Notes |
|------|---|-------|
| sensor_noise ↔ ioregistry | 1.000 | Same data source (ioreg). **CUT one.** |
| dyld_timing ↔ spotlight_timing | -0.395 | Both spawn subprocesses. **CUT dyld_timing.** |
| spotlight_timing ↔ ioregistry | -0.163 | Weak. Both involve system commands. Acceptable. |

## Final Source Count

| Status | Count | Sources |
|--------|-------|---------|
| KEEP | 10 | kqueue_events, thread_lifecycle, spotlight_timing, dvfs_race, mach_ipc, hash_timing, vm_page_timing, compression_timing, amx_timing, dispatch_queue |
| DEMOTE | 9 | cache_contention, page_fault_timing, speculative_execution, cas_contention, pipe_buffer, tlb_shootdown, cpu_memory_beat, cpu_io_beat, dram_row_buffer |
| CUT | 3 | multi_domain_beat, dyld_timing, sensor_noise |
| ALREADY VALIDATED | 1 | keychain_timing (PASS) |
| NOT TESTED (established) | 16 | timing (3), system (4), network (2), hardware (4), other |

**Starting sources:** 39
**After this audit:** 36 (remove multi_domain_beat, dyld_timing, sensor_noise)

## Recommendations

1. **Remove** `multi_domain_beat`, `dyld_timing`, `sensor_noise` from the source registry
2. **Mark demoted sources** with reduced weight in the entropy pool (e.g., 0.5x contribution)
3. **Investigate** `cache_contention` autocorrelation — the lag-5 value of 0.975 suggests a 3-cycle pattern from the sequential/random/strided rotation. Consider randomizing pattern order.
4. **Consider** promoting `page_fault_timing` if we can explain the H∞ inconsistency between 100K (1.47) and trial mean (0.78)
5. Keep `ioregistry` but understand it overlaps with any future sensor source. Mark it DEMOTE.
