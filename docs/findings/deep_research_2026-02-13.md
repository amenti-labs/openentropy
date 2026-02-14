# Deep Research: Novel Entropy Source Experiments

**Date:** 2026-02-13
**Platform:** Apple M4 Mac Mini, macOS
**Method:** 6 proof-of-concept C programs measuring previously-untapped hardware entropy domains

## Summary

Six experiments explored unconventional entropy sources on Apple Silicon. Two sources (DVFS race and CAS contention) were promoted to full Rust implementations. The remaining four showed lower entropy yields but provide valuable research data for future work.

| # | Experiment | Best Method | Shannon | H-infinity | Verdict |
|---|-----------|-------------|---------|-----------|---------|
| 1 | DRAM Refresh Interference | Delta analysis | 2.219 | 0.949 | Low — refresh timing too coarse |
| 2 | P/E-core Frequency Drift | Two-thread race | 7.964 | 7.288 | **Excellent — promoted to `dvfs_race`** |
| 3 | Cache Coherence Fabric (ICE) | XOR-fold | 1.005 | 0.991 | Low — too few unique values |
| 4 | Mach Thread QoS Scheduling | Priority change | 1.395 | 0.508 | Low — scheduler quantization |
| 5 | GPU/Accelerate Framework | vDSP convolution | 5.128 | 3.573 | Moderate — overlaps with `amx_timing` |
| 6 | Atomic CAS Contention | 4-thread XOR combine | 3.017 | 2.619 | **Usable — promoted to `cas_contention`** |

## Experiment 1: DRAM Refresh Interference Timing

**File:** `research/poc/poc_dram_decay.c`

**Hypothesis:** DRAM cells leak charge at physically random rates. While we can't observe cell decay directly (the OS handles refresh), we can measure timing spikes caused by DRAM refresh cycles stealing memory bus cycles. The phase relationship between our access pattern and the ~64ms refresh cycle should be physically nondeterministic.

**Method:** Allocate 64MB region spanning many DRAM banks. Perform strided reads at pseudo-random offsets, measuring access latency. Read-modify-write forces row buffer operations.

**Results:**
- Raw XOR-fold: 50/256 unique values, Shannon = 1.557, H-infinity = 0.488
- Delta analysis: 56/256 unique values, Shannon = 2.219, H-infinity = 0.949
- Timing range: 0-67 ticks, avg 2.7 — very low variance
- Many zero-tick measurements indicate cache hits dominate

**Analysis:** Apple Silicon's unified memory architecture with large SLC (System Level Cache) absorbs most DRAM access variability. The refresh interference signal is present (sporadic spikes to 30-67 ticks) but too infrequent to produce good entropy density. The existing `dram_row_buffer` source already captures this domain more effectively.

**Verdict:** Not promoted. Entropy too low (H-infinity < 1.0) and overlaps with existing `dram_row_buffer` source.

---

## Experiment 2: P-core vs E-core Frequency Drift (Software Ring Oscillator)

**File:** `research/poc/poc_frequency_drift.c`

**Hypothesis:** Apple M4's P-cores and E-cores have independent DVFS controllers. A "software ring oscillator" (tight counting loop in a fixed time window) should produce counts that vary with CPU frequency. The difference between two threads' counts captures cross-core frequency jitter — analogous to hardware ring oscillator PUFs.

**Method:** Three approaches tested:
1. Single-thread iteration count variance in 1us window
2. Delta of consecutive iteration counts
3. Two-thread race: spawn threads, let them count for ~2us, compare counts

**Results:**
- Method 1 (single-thread LSB): 19/256 unique, Shannon = 0.330, H-infinity = 0.050 — nearly deterministic
- Method 2 (delta XOR-fold): 18/256 unique, Shannon = 0.523, H-infinity = 0.091 — still very low
- **Method 3 (two-thread race): 256/256 unique, Shannon = 7.964, H-infinity = 7.288** — near-perfect

**Analysis:** Single-thread counts are highly deterministic because one core runs at a stable frequency. But the *difference* between two cores captures physical frequency jitter from independent DVFS controllers, thermal sensors, and scheduler core placement. The race differential hits 256/256 unique byte values — the maximum possible — with H-infinity = 7.288 (the highest of any source we've tested, including network sources).

**Key insight:** It's not the absolute frequency that matters, but the *relative phase* between two independent clock domains. This is the same principle as hardware ring oscillator PUFs, implemented entirely in software.

**Verdict:** Promoted to `dvfs_race` source. Highest H-infinity of any discovered source.

---

## Experiment 3: Cache Coherence Fabric (ICE) Timing

**File:** `research/poc/poc_coherence_fabric.c`

**Hypothesis:** Apple M4's Interconnect Coherence Engine (ICE) handles cache coherency between P-core cluster, E-core cluster, GPU, and ANE. Cache line bouncing between threads on different core types should produce nondeterministic coherence transition timing.

**Method:** Allocate 64 cache-line-aligned structures. Two threads alternate touching all cache lines, forcing MESI state transitions (Modified/Exclusive/Shared/Invalid). Measure each thread's time to acquire all lines.

**Results:**
- Main thread XOR-fold: 4/256 unique, Shannon = 1.005, H-infinity = 0.991
- Delta XOR-fold: 3/256 unique, Shannon = 0.883, H-infinity = 0.515
- Asymmetry (main vs remote): 3/256 unique, Shannon = 0.527, H-infinity = 0.182
- Timing range: 0-3 ticks — extremely narrow

**Analysis:** The coherence fabric on M4 is remarkably fast and deterministic. Cache line bouncing completes in 0-3 ticks, producing only 3-4 distinct timing values. This makes sense — Apple's ICE is designed for low-latency coherency to support their unified memory architecture. The deterministic behavior is a feature, not a bug, of Apple's hardware design.

**Verdict:** Not promoted. Too few unique values (3-4) for meaningful entropy. The coherence fabric is too fast and too deterministic on Apple Silicon.

---

## Experiment 4: Mach Thread QoS Scheduling Entropy

**File:** `research/poc/poc_mach_voucher.c`

**Hypothesis:** macOS's CLUTCH scheduler makes per-thread QoS decisions based on thread priority, process importance, thermal pressure, and CPU time decay. Rapidly changing QoS tiers and measuring scheduling latency should expose the scheduler's internal state.

**Method:** Four approaches:
1. `thread_info()` scheduling statistics with variable work
2. `task_info()` user/system time deltas
3. `pthread_setschedparam()` priority change + `sched_yield()` latency
4. `getrusage()` context switch / page fault delta XOR with timing

**Results:**
- Method 1 (thread_info): 13/256 unique, Shannon = 1.242, H-infinity = 0.567
- Method 2 (task_info): 16/256 unique, Shannon = 1.160, H-infinity = 0.465
- Method 3 (priority change): 14/256 unique, Shannon = 1.395, H-infinity = 0.508
- Method 4 (rusage): 5/256 unique, Shannon = 1.002, H-infinity = 0.540

**Analysis:** All four methods produce very low entropy (H-infinity < 0.6). The Mach scheduling syscalls have highly quantized timing — they complete in a small number of discrete tick values. The QoS propagation mechanism adds minimal observable jitter. This contrasts with our existing `thread_lifecycle` source which gets H-infinity = 6.79 by measuring the *physical* operation of thread creation/destruction rather than scheduling metadata queries.

**Verdict:** Not promoted. All methods < 1.4 bits Shannon. Scheduler timing is too quantized for useful entropy. The existing `thread_lifecycle` and `dispatch_queue` sources already capture scheduling nondeterminism more effectively.

---

## Experiment 5: GPU/Accelerate Framework Timing

**File:** `research/poc/poc_metal_gpu.c`

**Hypothesis:** The M4's GPU shares the unified memory controller and SLC with the CPU. Accelerate framework operations (vDSP FFT, convolution, matrix multiply) dispatch to the AMX coprocessor with timing that depends on thermal state, memory bandwidth contention, and dispatch queue arbitration.

**Method:** Four approaches:
1. vDSP FFT timing with various sizes (64-4096)
2. vDSP convolution with variable filter lengths
3. IOKit AGX service lookup timing (IORegistry traversal)
4. vDSP large matrix multiplication (64x64 to 192x192)

**Results:**
- Method 1 (FFT): 47/256 unique, Shannon = 3.794, H-infinity = 2.520
- **Method 2 (convolution): 68/256 unique, Shannon = 5.128, H-infinity = 3.573**
- Method 3 (IOKit AGX): 106/256 unique, Shannon = 4.066, H-infinity = 2.283
- Method 4 (matrix mul): 81/256 unique, Shannon = 4.914, H-infinity = 3.302

**Analysis:** vDSP convolution and matrix multiply show decent entropy (Shannon > 4.9, H-infinity > 3.3). However, these largely overlap with the existing `amx_timing` source which already measures AMX coprocessor dispatch jitter using `cblas_sgemm`. The IOKit AGX lookup is interesting (106 unique values) but with H-infinity only 2.283 — the timing has high variance but also a dominant mode.

**Verdict:** Not promoted as a separate source. The entropy domain substantially overlaps with `amx_timing`. The IOKit approach could be explored further but isn't compelling enough on its own. Note: the convolution approach could be added as an alternative AMX workload in a future `amx_timing` config option.

---

## Experiment 6: Atomic CAS Contention

**File:** `research/poc/poc_numa_asymmetry.c`

**Hypothesis:** Multiple threads racing on atomic CAS (compare-and-swap) operations create physically nondeterministic arbitration timing. The cache coherence engine must arbitrate concurrent exclusive-access requests, and this arbitration depends on interconnect fabric load, thermal state, and core placement.

**Method:** Three approaches:
1. Single-thread CAS baseline (no contention)
2. 4-thread CAS contention on 64 shared cache-line-aligned targets
3. Memory latency landscape (address-dependent timing)

**Results:**
- Method 1 (single CAS): 6/256 unique, Shannon = 1.131, H-infinity = 0.604 — nearly deterministic
- Method 2 (4-thread CAS): 16/256 unique, Shannon = 2.276, H-infinity = 1.577
- **Method 2 XOR-combined: 32/256 unique, Shannon = 3.017, H-infinity = 2.619**
- Method 3 (memory landscape): 24/256 unique, Shannon = 1.011, H-infinity = 0.388

**Analysis:** Single-thread CAS is nearly deterministic (no contention = no arbitration). Adding 4 threads dramatically increases entropy because the coherence engine must now arbitrate between competing exclusive-access requests. XOR-combining all 4 threads' timings amplifies the effect further (H-infinity 1.577 -> 2.619). The key observation: contention creates entropy, not the CAS operation itself.

The Rust implementation (`cas_contention.rs`) improves on this PoC by:
- Using `compare_exchange_weak` (allows spurious failures, exposing more arbitration decisions)
- Delta-of-deltas extraction (removes systematic bias)
- Configurable thread count
- 128-byte TARGET_SPACING constant for Apple Silicon cache lines

**Verdict:** Promoted to `cas_contention` source. While H-infinity (2.463 estimated in Rust impl) is moderate, the source is extremely fast (<100ms), exercises a unique entropy domain (hardware coherence arbitration), and is independent of all other sources.

---

## Cross-Experiment Insights

1. **Thread racing is the most effective software entropy technique.** Both promoted sources (dvfs_race H-infinity=7.288, cas_contention H-infinity=2.619) derive entropy from multi-thread contention. Single-thread measurements consistently yield low entropy on Apple Silicon.

2. **Apple Silicon is remarkably deterministic for single-threaded operations.** The cache coherence fabric (3 unique values), DRAM access (avg 2.7 ticks), and scheduler syscalls (5-16 unique values) all show extreme timing quantization. The hardware is optimized for predictable single-thread performance.

3. **Cross-core frequency jitter is an unexploited goldmine.** The DVFS race source (H-infinity=7.288) rivals network sources in entropy quality while being 100x faster. This suggests Apple Silicon's independent per-cluster DVFS controllers are a rich entropy domain.

4. **Entropy domains that overlap with existing sources:** GPU/Accelerate timing overlaps with `amx_timing`, DRAM refresh overlaps with `dram_row_buffer`, QoS scheduling overlaps with `thread_lifecycle`. New sources should target genuinely independent physical phenomena.

5. **The XOR-combine technique is consistently effective.** In experiment 6, XOR-combining 4 threads' timings improved H-infinity from 1.577 to 2.619 (66% increase). This validates the approach used throughout OpenEntropy's pool architecture.
