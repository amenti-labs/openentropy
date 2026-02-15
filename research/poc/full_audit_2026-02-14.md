# Full Redundancy & Correctness Audit — 2026-02-14

## Methodology

Every frontier source file was read line-by-line. For each source, I verified:
1. **Does it do what it claims?** Is the timed operation real or a no-op?
2. **Is the timing correct?** Are `mach_time()` calls placed before AND after the operation?
3. **Could entropy be fake?** Would replacing the operation with a no-op change the output?
4. **Is it independent?** Does it measure a physically distinct phenomenon?

---

## CRITICAL PATTERN: Counter-XOR-Timestamp Masking

Three sources use a dangerous pattern: `output = counter ^ mach_time() ^ measurement`.
The `counter ^ mach_time()` term produces seemingly-random output **even if the measurement
contributes zero entropy**. This makes it impossible to verify entropy quality from output alone.

Sources using this pattern: `gpu_divergence`, `iosurface_crossing`, `quantum_boundary`, `counter_beat`.

---

## Source-by-Source Verdicts

### 1. quantum_boundary — CUT (Fake Entropy)

**File:** `frontier/quantum_boundary.rs`

**Claim:** "Mach scheduler preemption timestamp jitter entropy"

**Reality:** XORs a deterministic counter (1, 2, 3, ...) with `mach_time()`. The "busy
wait" is `black_box(0u64)` repeated 50 times — this is essentially a no-op. The name
"quantum boundary" implies scheduler quantum preemption, but the loop runs in microseconds
— far too short for any preemption to occur.

**Critical test:** Replace the entire loop body with just `mach_time()`. The output entropy
would be IDENTICAL, because the counter XOR is the only thing providing apparent randomness,
and it's fed entirely by clock readings.

**This is `mach_timing` with extra steps.** It measures the exact same physical phenomenon
(clock oscillator noise) already captured by the timing/clock_jitter and timing/mach_timing
sources that have been in the project since v0.1.

**Verdict: CUT** — Redundant with existing clock sources; counter-XOR pattern masks the lack of novel entropy.

---

### 2. counter_beat — CUT (False Independence Claim)

**File:** `frontier/counter_beat.rs`

**Claim:** "XORs the instantaneous values of two independent hardware counters:
ARM64 CNTVCT_EL0 (1 GHz virtual timer) and mach_absolute_time (24 MHz crystal-derived).
The phase relationship between these independent oscillators has thermally-driven jitter."

**Reality:** On Apple Silicon, `mach_absolute_time()` **IS** backed by CNTVCT_EL0. They
are the SAME counter. Apple's implementation of `mach_absolute_time` reads the ARM generic
timer (CNTPCT_EL0 / CNTVCT_EL0) and applies a scaling factor. The "24 MHz crystal" claim
refers to the fact that the commpage timebase info has a `numer/denom` ratio, but both
readings derive from the same physical oscillator.

XORing a counter with itself yields the pipeline stall between the two reads — this is
general CPU timing jitter, identical to what `clock_jitter` already measures.

**The "independent oscillators" claim is FALSE on Apple Silicon.**

**Verdict: CUT** — Based on a false hardware assumption; measures the same phenomenon as clock_jitter.

---

### 3. accelerate_jitter — CUT (Redundant with amx_timing)

**File:** `frontier/accelerate_jitter.rs`

**Claim:** "This is distinct from `amx_timing` which uses raw AMX instructions — here we
go through the Accelerate framework's dispatch layer."

**Reality:** `amx_timing` ALSO calls `cblas_sgemm` via the Accelerate framework. Compare:
- `amx_timing.rs:176`: `cblas_sgemm(101, 111, trans_b, n, n, n, 1.0, ...)`
- `accelerate_jitter.rs:109`: `accelerate::cblas_sgemm(CBLAS_ROW_MAJOR, CBLAS_NO_TRANS, CBLAS_NO_TRANS, m, m, m, 1.0, ...)`

These call the exact same function. The claim that amx_timing uses "raw AMX instructions"
is incorrect — both go through Accelerate's `cblas_sgemm`. The only differences:
- `amx_timing` uses variable matrix sizes (16-128) and Von Neumann debiasing
- `accelerate_jitter` uses a fixed 64x64 matrix with no debiasing

`amx_timing` is strictly superior in every way.

**Verdict: CUT** — Identical API call to amx_timing; amx_timing has better implementation.

---

### 4. gpu_divergence — FIX (Real entropy, masked by counter XOR)

**File:** `frontier/gpu_divergence.rs`

**Claim:** "Dispatches Metal compute shaders where parallel threads race to atomically
increment a shared counter."

**Reality:** This source ACTUALLY compiles and dispatches a Metal shader with atomic
operations. The GPU thread execution ordering IS physically nondeterministic. This is
genuinely novel.

**Problem:** The output combines `counter ^ now ^ gpu_hash`. The `counter ^ now` term
masks whether `gpu_hash` contributes real entropy. If the GPU dispatch returned all zeros,
the output would still look random.

**Fix needed:** Use `extract_timing_entropy` on the dispatch timings and/or use only
`gpu_hash` values directly. Remove the counter and timestamp from the output combination.

**Verdict: FIX** — Genuine novel entropy source, but remove counter-XOR-timestamp masking.

---

### 5. iosurface_crossing — FIX (Real entropy, masked by counter XOR)

**File:** `frontier/iosurface_crossing.rs`

**Claim:** "IOSurface GPU/CPU memory domain crossing coherence jitter"

**Reality:** Actually creates IOSurfaces via direct framework FFI, performs
lock/write/unlock/read cycles that cross CPU↔GPU memory domains. The write/read cycle
timing (`cycle_timing`) IS genuinely nondeterministic.

**Problem:** Same masking pattern: `combined = counter ^ now ^ cycle_timing`. The
`cycle_timing` from `crossing_cycle()` already captures the round-trip timing (write_timing
XOR read_timing). Just use that directly with `extract_timing_entropy`.

**Fix needed:** Replace XOR-fold output path with `extract_timing_entropy` on the raw
cycle timings.

**Verdict: FIX** — Genuine IOSurface FFI work, but remove counter-XOR-timestamp masking.

---

### 6. nvme_latency — FIX (Wrong physics description)

**File:** `frontier/nvme_latency.rs`

**Claim:** "NVMe flash cell read latency jitter from NAND physics"

**Reality:** Creates a temp file, writes data, then reads back from it with F_NOCACHE.

**Problem:** The file was JUST WRITTEN — the SSD controller has the data in its DRAM write
cache. F_NOCACHE bypasses the OS buffer cache but NOT the SSD's internal DRAM cache.
This is NOT measuring NAND cell physics (charge state, cross-coupling, oxide wear). It
measures filesystem overhead + NVMe command processing + SSD controller DRAM access.

The timing IS still genuinely nondeterministic (real I/O operations), but the entropy
comes from filesystem/NVMe controller scheduling, not NAND cell physics as claimed.

**Fix needed:** Correct the physics description. The source itself is fine — just the
documentation overpromises.

**Verdict: FIX** — Real I/O entropy, but physics description is incorrect.

---

### 7. amx_timing — KEEP (Well-implemented)

**File:** `frontier/amx_timing.rs`

Calls `cblas_sgemm` with variable matrix sizes (16-128), interleaved memory ops to disrupt
pipeline steady-state, and Von Neumann debiasing to correct heavy LSB bias. Timing is
correct (mach_time before/after the operation). Uses `extract_timing_entropy_debiased`.

The AMX coprocessor IS a distinct execution unit on Apple Silicon. Pipeline occupancy,
memory bandwidth, and thermal throttling create genuine nondeterminism.

**Verdict: KEEP** — Genuine, well-implemented, unique execution domain.

---

### 8. thread_lifecycle — KEEP

**File:** `frontier/thread_lifecycle.rs`

Actually spawns and joins real threads with variable workloads. Each cycle exercises kernel
thread port allocation, stack page allocation, TLS setup, and scheduler core selection.
Timing is correct. Uses `extract_timing_entropy`.

**Verdict: KEEP** — Genuine kernel scheduling entropy from a unique code path.

---

### 9. mach_ipc — KEEP

**File:** `frontier/mach_ipc.rs`

Sends complex Mach messages with OOL descriptors through real Mach ports. Exercises
`vm_map_copyin`/`vm_map_copyout`, port namespace operations, and cross-thread scheduling.
Has a receiver thread for bidirectional timing. Configurable. Well-implemented.

**Verdict: KEEP** — Deep XNU kernel path, genuinely unique.

---

### 10. tlb_shootdown — KEEP

**File:** `frontier/tlb_shootdown.rs`

Real `mprotect()` calls on `mmap`'d memory with variable page counts and regions. Forces
TLB invalidation IPIs across all cores. Uses variance extraction (delta-of-deltas) for
better min-entropy. Properly touches all pages before measurement.

**Verdict: KEEP** — Real microarchitectural side-channel, unique mechanism.

---

### 11. pipe_buffer — KEEP

**File:** `frontier/pipe_buffer.rs`

Multiple simultaneous pipes with variable write sizes, non-blocking I/O mode, and periodic
pipe creation/destruction for zone allocator churn. Real kernel I/O operations. Configurable.

**Verdict: KEEP** — Unique kernel zone allocator contention path.

---

### 12. kqueue_events — KEEP

**File:** `frontier/kqueue_events.rs`

Registers multiple event types (timers, file watchers, socket pairs) and measures kevent()
notification timing. Background poker thread creates asynchronous events. Complex
interaction pattern with rich interference.

**Verdict: KEEP** — Rich multi-event-type interaction, unique.

---

### 13. dvfs_race — KEEP

**File:** `frontier/dvfs_race.rs`

Two threads race in tight counting loops; the absolute difference in iteration counts is
the entropy. The physics description honestly acknowledges that the 2μs window is too short
for actual DVFS transitions and that the primary entropy comes from scheduling and
cache-coherence nondeterminism. The mechanism (thread race counting) is unique.

**Verdict: KEEP** — Unique mechanism, honest about physics.

---

### 14. cas_contention — KEEP

**File:** `frontier/cas_contention.rs`

4 threads performing `compare_exchange_weak` on shared targets spread across 128-byte
aligned cache lines. XOR-combines timings from all threads. Cache coherence arbitration
(MOESI) is genuinely nondeterministic.

**Verdict: KEEP** — Real multi-thread hardware coherence contention.

---

### 15. keychain_timing — KEEP

**File:** `frontier/keychain_timing.rs`

Real Security.framework keychain operations (SecItemCopyMatching or SecItemAdd/Delete).
Discards first 100 warm-up samples. Uses variance extraction to remove serial correlation.
Both read and write paths are well-implemented.

**Verdict: KEEP** — Multi-domain IPC round-trip, well-implemented.

---

### 16. denormal_timing — KEEP (Marginal)

**File:** `frontier/denormal_timing.rs`

Times blocks of denormal float multiply-accumulate operations. Physics description honestly
acknowledges that Apple Silicon handles denormals in hardware (no microcode penalty). H∞ of
1.2 bits/byte is low but honest. Provides independence from integer-only timing sources.

**Verdict: KEEP** — Low entropy but honest, unique FPU code path.

---

### 17. audio_pll_timing — KEEP

**File:** `frontier/audio_pll_timing.rs`

Queries CoreAudio device properties (sample rate, latency) via real framework FFI. Cycles
through different property selectors. Hardware-dependent (requires audio device).

The "PLL phase noise" claim is likely overstated (timing mostly captures function call
overhead + IPC to coreaudiod), but it IS a distinct code path.

**Verdict: KEEP** — Different subsystem, real FFI operations.

---

### 18. usb_timing — KEEP

**File:** `frontier/usb_timing.rs`

Queries USB device properties via IOKit/IORegistry. Cycles through different property keys
and devices. Hardware-dependent (requires USB devices connected).

**Verdict: KEEP** — Unique subsystem, real IOKit operations.

---

### 19. fsync_journal — KEEP

**File:** `frontier/fsync_journal.rs`

Creates new temp files each iteration, writes data, calls `sync_all()` (fsync). Exercises
full APFS allocation + B-tree insert + journal commit path. Different from `disk_io` which
does reads. Uses `extract_timing_entropy`.

**Verdict: KEEP** — Different I/O path (write+fsync vs read), exercises journal commit.

---

### 20. pdn_resonance — KEEP (Marginal)

**File:** `frontier/pdn_resonance.rs`

Spawns stress threads (memory + ALU workloads) while measuring timing on the main thread.
Uses `extract_timing_entropy`. The "PDN resonance" claim is a stretch — the timing
perturbation is more likely from cache contention and scheduler interference than PCB
power plane resonance. H∞ of 1.6 bits/byte is honest.

Similar mechanism to `cas_contention`, but different: CAS measures atomic operation
arbitration timing, while PDN measures background-workload-induced timing perturbation.

**Verdict: KEEP** — Marginal but distinct mechanism from CAS contention.

---

## Redundancy Matrix (Potential Overlaps)

| Source A | Source B | Same Physical Phenomenon? | Verdict |
|----------|----------|--------------------------|---------|
| accelerate_jitter | amx_timing | YES — identical cblas_sgemm call | CUT accelerate_jitter |
| quantum_boundary | clock_jitter | YES — both just read mach_time() | CUT quantum_boundary |
| counter_beat | clock_jitter | YES — CNTVCT_EL0 = mach_absolute_time on Apple Silicon | CUT counter_beat |
| pdn_resonance | cas_contention | Partially — both measure cross-core effects | KEEP both (different mechanisms) |
| iosurface_crossing | keychain_timing | NO — different APIs entirely | KEEP both |
| nvme_latency | fsync_journal | Partially — both do I/O | KEEP both (read vs write+fsync) |
| nvme_latency | disk_io | Partially — both do file I/O | KEEP both (F_NOCACHE vs normal) |
| audio_pll_timing | usb_timing | NO — different subsystems | KEEP both |

---

## Summary

| Verdict | Count | Sources |
|---------|-------|---------|
| **KEEP** | 14 | amx_timing, thread_lifecycle, mach_ipc, tlb_shootdown, pipe_buffer, kqueue_events, dvfs_race, cas_contention, keychain_timing, denormal_timing, audio_pll_timing, usb_timing, fsync_journal, pdn_resonance |
| **FIX** | 3 | gpu_divergence (remove counter-XOR mask), iosurface_crossing (remove counter-XOR mask), nvme_latency (correct physics description) |
| **CUT** | 3 | quantum_boundary (fake entropy), counter_beat (false independence), accelerate_jitter (redundant with amx_timing) |

**Final source count: 47 → 44** (removing quantum_boundary, counter_beat, accelerate_jitter)

---

## Fixes Required

### gpu_divergence
Replace `counter ^ now ^ gpu_hash` combination with `extract_timing_entropy` on dispatch
timings. The GPU atomic execution order (`gpu_hash`) should be used as the primary entropy
signal, combined with dispatch timing via the standard extraction pipeline.

### iosurface_crossing
Replace `counter ^ now ^ cycle_timing` combination with `extract_timing_entropy` on raw
cycle timings. The `crossing_cycle()` function already returns honest timing data.

### nvme_latency
Update physics description: source measures filesystem + NVMe controller scheduling jitter,
not NAND cell physics. The file was just written and sits in the SSD's DRAM cache.
