# OpenEntropy — Source Catalog

All 39 entropy sources, their physics, quality, and operational characteristics.

> **Grades are for raw (unconditioned) output.** After SHA-256 conditioning, all sources produce Grade A output. The raw grades reflect the true hardware entropy density before any whitening.

## Summary Table

| # | Source | Category | Raw Grade | Shannon (bits/byte) | Speed | Platform |
|---|--------|----------|-----------|---------------------|-------|----------|
| 1 | `clock_jitter` | Timing | B | 6.28 | 0.001s | All |
| 2 | `mach_timing` | Timing | F | 0.99 | <0.001s | macOS |
| 3 | `sleep_jitter` | Timing | F | 2.62 | 0.001s | All |
| 4 | `sysctl_deltas` | System | D | 2.49 | 0.19s | macOS |
| 5 | `vmstat_deltas` | System | D | 2.11 | 0.48s | macOS |
| 6 | `process_table` | System | C | 4.22 | 0.03s | macOS |
| 7 | `ioregistry` | System | C | 3.08 | 1.99s | macOS |
| 8 | `dns_timing` | Network | A | 7.97 | 18.9s | All |
| 9 | `tcp_connect_timing` | Network | A | 7.96 | 38.8s | All |
| 10 | `disk_io` | Hardware | C | 4.73 | 0.006s | All |
| 11 | `memory_timing` | Hardware | A | 6.73 | 0.02s | All |
| 12 | `gpu_timing` | Hardware | A | 7.96 | 37.0s | macOS |
| 13 | `bluetooth_noise` | Hardware | C | 4.43 | 10.2s | macOS |
| 14 | `audio_noise` | Hardware | — | — | — | macOS (ffmpeg) |
| 15 | `camera_noise` | Hardware | — | — | — | macOS (ffmpeg) |
| 16 | `wifi_noise` | Hardware | — | — | — | macOS (WiFi) |
| 17 | `sensor_noise` | Hardware | — | — | — | macOS (CoreMotion) |
| 18 | `dram_row_buffer` | Silicon | C | 3.09 | 0.006s | All |
| 19 | `cache_contention` | Silicon | B | 3.96 | 0.03s | All |
| 20 | `page_fault_timing` | Silicon | A | 7.80 | 0.02s | All |
| 21 | `speculative_execution` | Silicon | F | 2.00 | 0.001s | All |
| 22 | `cpu_io_beat` | Cross-Domain | C | 4.41 | 0.08s | All |
| 23 | `cpu_memory_beat` | Cross-Domain | D | 2.77 | 0.01s | All |
| 24 | `multi_domain_beat` | Cross-Domain | D | 2.60 | 0.007s | All |
| 25 | `compression_timing` | Novel | B | 5.31 | 0.13s | All |
| 26 | `hash_timing` | Novel | D | 3.13 | 0.02s | All |
| 27 | `dispatch_queue` | Novel | B | 6.05 | 0.13s | macOS |
| 28 | `dyld_timing` | Novel | A | 7.33 | 1.2s | macOS/Linux |
| 29 | `vm_page_timing` | Novel | A | 7.85 | 0.11s | macOS/Linux |
| 30 | `spotlight_timing` | Novel | A | 7.00 | 12.7s | macOS |
| 31 | `amx_timing` | Frontier | B | 5.19 | 0.05s | macOS (Apple Silicon) |
| 32 | `thread_lifecycle` | Frontier | A | 6.79 | 0.08s | All |
| 33 | `mach_ipc` | Frontier | B | 4.92 | 0.04s | macOS |
| 34 | `tlb_shootdown` | Frontier | A | 6.46 | 0.03s | All |
| 35 | `pipe_buffer` | Frontier | C | 3.22 | 0.01s | All |
| 36 | `kqueue_events` | Frontier | A* | — | 0.05s | macOS/BSD |
| 37 | `dvfs_race` | Frontier | A | 7.96 | <0.1s | All |
| 38 | `cas_contention` | Frontier | D | 3.02 | <0.1s | All |
| 39 | `interleaved_frontier` | Frontier **[C]** | A* | — | 0.2s | All |

**Grade scale:** A ≥ 6.5, B ≥ 5.0, C ≥ 3.5, D ≥ 2.0, F < 2.0 (Shannon entropy bits per byte, max 8.0)

## Source Details

### Timing Sources

#### 1. `clock_jitter` — Phase noise between clocks
- **Physics:** Measures phase difference between `Instant` (monotonic) and `SystemTime` (wall clock). These derive from independent oscillators (TSC vs RTC), and their phase relationship drifts due to thermal noise, frequency scaling, and NTP corrections.
- **Raw entropy:** B (6.28 bits/byte)
- **Speed:** <1ms for 5000 samples — extremely fast
- **Platform:** All (uses std::time)

#### 2. `mach_timing` — Mach absolute time LSBs
- **Physics:** Reads `mach_absolute_time()` with micro-workloads and extracts raw LSBs. The LSBs capture CPU cycle-level jitter from pipeline state, branch prediction, and cache effects.
- **Raw entropy:** F (0.99 bits/byte) — LSBs are highly correlated; the useful entropy is in the timing *deltas* between samples, but raw byte density is low.
- **Speed:** <1ms — fastest source
- **Platform:** macOS only

#### 3. `sleep_jitter` — OS scheduler jitter
- **Physics:** Issues zero-duration sleep calls and measures actual wake-up latency. The kernel scheduler introduces non-deterministic delays based on: run queue depth, priority inversions, timer coalescing, and interrupt handling.
- **Raw entropy:** F (2.62 bits/byte) — scheduler quantization limits raw entropy density
- **Speed:** ~1ms
- **Platform:** All

### System Sources

#### 4. `sysctl_deltas` — Kernel counter deltas
- **Physics:** Batch-reads ~1600 kernel counters via `sysctl -a` and extracts deltas from the ~40-60 that change within 200ms. Counters track page faults, context switches, TCP segments, interrupts — each driven by independent processes.
- **Raw entropy:** D (2.49 bits/byte)
- **Speed:** 190ms
- **Platform:** macOS

#### 5. `vmstat_deltas` — VM statistics deltas
- **Physics:** Samples macOS `vm_stat` counters (page faults, pageins, pageouts, compressions, decompressions). Each counter changes when hardware page table walks, TLB misses, or memory pressure triggers compressor/swap.
- **Raw entropy:** D (2.11 bits/byte)
- **Speed:** 480ms
- **Platform:** macOS

#### 6. `process_table` — Process table snapshots
- **Physics:** Snapshots the process table combined with `getpid()` timing jitter. Process creation/destruction, PID recycling, and scheduling state create a high-dimensional chaotic system.
- **Raw entropy:** C (4.22 bits/byte)
- **Speed:** 26ms
- **Platform:** macOS

#### 7. `ioregistry` — IORegistry hardware counters
- **Physics:** Mines the macOS IORegistry for fluctuating hardware counters — GPU utilization, NVMe SMART counters, memory controller stats, Neural Engine buffer allocations, DART IOMMU activity, Mach port counts, and display vsync counters. Each counter is driven by independent hardware subsystems. The LSBs of their deltas capture silicon-level activity across the entire SoC.
- **Raw entropy:** C (3.08 bits/byte)
- **Speed:** 2.0s (takes 4 snapshots with 80ms delays)
- **Platform:** macOS

### Network Sources

#### 8. `dns_timing` — DNS query round-trip timing
- **Physics:** Measures round-trip timing of DNS A-record queries to public resolvers. Timing varies with: network path congestion, router queue depths, DNS server load, TCP/TLS handshake overhead, and WAN link jitter. Uses multiple resolvers (8.8.8.8, 1.1.1.1, etc.) for independence.
- **Raw entropy:** A (7.97 bits/byte) — highest quality
- **Speed:** 18.9s (network-bound)
- **Platform:** All (requires network)

#### 9. `tcp_connect_timing` — TCP handshake timing
- **Physics:** Nanosecond timing of TCP three-way handshakes to remote hosts. Captures: SYN-SYN/ACK round-trip variance, kernel TCP stack scheduling, NIC interrupt coalescing, and network path jitter.
- **Raw entropy:** A (7.96 bits/byte)
- **Speed:** 38.8s (network-bound)
- **Platform:** All (requires network)

### Hardware Sources

#### 10. `disk_io` — NVMe/SSD read latency
- **Physics:** Measures NVMe/SSD read latency for small random reads. Jitter sources: flash translation layer (FTL) remapping, wear leveling, garbage collection, read disturb mitigation, NAND page read latency variation, and NVMe controller queue arbitration.
- **Raw entropy:** C (4.73 bits/byte)
- **Speed:** 6ms — very fast
- **Platform:** All

#### 11. `memory_timing` — DRAM allocation timing
- **Physics:** Times memory allocation (malloc/mmap) and access patterns. Jitter from: heap fragmentation, page fault handling, kernel memory pressure, DRAM refresh interference (~64ms cycle), cache hierarchy state, and memory controller scheduling.
- **Raw entropy:** A (6.73 bits/byte)
- **Speed:** 21ms
- **Platform:** All

#### 12. `gpu_timing` — GPU dispatch timing
- **Physics:** GPU dispatch timing jitter via `sips` image processing. Captures: GPU command buffer scheduling, shader dispatch latency, PCIe bus arbitration, and GPU memory management. Uses Core Image pipeline internally.
- **Raw entropy:** A (7.96 bits/byte)
- **Speed:** 37.0s (spawns sips process per sample)
- **Platform:** macOS

#### 13. `bluetooth_noise` — BLE RSSI values
- **Physics:** BLE RSSI values and scanning timing jitter. RSSI fluctuates with: multipath fading, constructive/destructive interference, nearby device radio activity, and Bluetooth frequency hopping across 37 advertising channels.
- **Raw entropy:** C (4.43 bits/byte)
- **Speed:** 10.2s
- **Platform:** macOS (Bluetooth hardware)

#### 14-17. `audio_noise`, `camera_noise`, `wifi_noise`, `sensor_noise`
- **Status:** Implemented but unavailable on test machine
- **Requirements:** `audio_noise` and `camera_noise` need ffmpeg; `wifi_noise` needs active WiFi; `sensor_noise` needs CoreMotion data
- **Physics:** Audio noise captures ADC thermal noise; camera captures CCD/CMOS sensor noise (photon shot noise, dark current); WiFi captures RSSI via CoreWLAN; sensor captures MEMS accelerometer/gyroscope Brownian motion

### Silicon Sources

#### 18. `dram_row_buffer` — DRAM row buffer timing
- **Physics:** Measures DRAM row buffer hit/miss timing. DRAM is organized into rows of capacitor cells. Accessing an open row (hit) is fast; accessing a different row requires precharge + activate (miss). Timing depends on: physical address mapping, DRAM refresh state, and competing memory traffic.
- **Raw entropy:** C (3.09 bits/byte)
- **Speed:** 6ms
- **Platform:** All

#### 19. `cache_contention` — L1/L2 cache timing
- **Physics:** Measures L1/L2 cache miss patterns by alternating access patterns. Cache timing depends on what every other process and hardware unit is doing — the cache is a shared resource whose state is fundamentally unpredictable.
- **Raw entropy:** B (3.96 bits/byte)
- **Speed:** 33ms
- **Platform:** All

#### 20. `page_fault_timing` — Page fault timing
- **Physics:** Triggers and times minor page faults via mmap/munmap. Resolution requires: TLB lookup, hardware page table walk (up to 4 levels on ARM64), physical page allocation from kernel free list, and zero-fill for security. Timing depends on physical memory fragmentation.
- **Raw entropy:** A (7.80 bits/byte)
- **Speed:** 20ms
- **Platform:** All

#### 21. `speculative_execution` — Branch predictor timing
- **Physics:** Measures timing variations from the CPU's speculative execution engine. The branch predictor maintains per-address history that depends on ALL previously executed code. Mispredictions cause pipeline flushes (~15 cycle penalty on M4).
- **Raw entropy:** F (2.00 bits/byte) — very fine-grained timing; LSB extraction yields low density
- **Speed:** 1ms
- **Platform:** All

### Cross-Domain Sources

#### 22. `cpu_io_beat` — CPU ↔ I/O beat frequency
- **Physics:** Alternates CPU-bound computation with disk I/O and measures transition timing. The CPU and I/O subsystem run on independent clock domains with separate PLLs. Cross-domain transitions create beat frequency jitter — analogous to acoustic beats between two tuning forks.
- **Raw entropy:** C (4.41 bits/byte)
- **Speed:** 80ms
- **Platform:** All

#### 23. `cpu_memory_beat` — CPU ↔ memory beat frequency
- **Physics:** Interleaves CPU computation with random memory accesses to large arrays (>L2 cache). The memory controller runs on its own clock domain. Cache misses force the CPU to wait for the memory controller's arbitration.
- **Raw entropy:** D (2.77 bits/byte)
- **Speed:** 10ms
- **Platform:** All

#### 24. `multi_domain_beat` — 4-domain composite beat
- **Physics:** Rapidly interleaves operations across 4 clock domains: CPU computation, memory access, disk I/O, and kernel syscalls. Each domain has its own PLL and arbitration logic. The composite timing captures interference patterns between all domains simultaneously.
- **Raw entropy:** D (2.60 bits/byte)
- **Speed:** 7ms
- **Platform:** All

### Novel Sources

#### 25. `compression_timing` — Compression algorithm timing
- **Physics:** Compresses varying data with zlib and measures per-operation timing. Compression algorithms have heavily data-dependent branches (Huffman tree traversal, LZ77 match finding). The CPU's branch predictor state from ALL running code affects prediction accuracy.
- **Raw entropy:** B (5.31 bits/byte)
- **Speed:** 130ms
- **Platform:** All

#### 26. `hash_timing` — SHA-256 hashing timing
- **Physics:** SHA-256 hashes data of varying sizes and measures timing. While SHA-256 is algorithmically constant-time, actual execution varies due to: memory access patterns, cache line alignment, TLB state, and CPU frequency scaling. *Note: SHA-256 here is the workload being timed, not a conditioning step.*
- **Raw entropy:** D (3.13 bits/byte)
- **Speed:** 22ms
- **Platform:** All

#### 27. `dispatch_queue` — GCD scheduling latency
- **Physics:** Submits blocks to Grand Central Dispatch queues and measures scheduling latency. macOS dynamically migrates work between P-cores and E-cores based on thermal state and load. Migration decisions, queue priority inversions, and QoS tier scheduling create non-deterministic timing.
- **Raw entropy:** B (6.05 bits/byte)
- **Speed:** 130ms
- **Platform:** macOS

#### 28. `dyld_timing` — Dynamic linker timing
- **Physics:** Times dynamic library loading (dlopen/dlsym): searching the dyld shared cache, resolving symbol tables, rebasing pointers, running initializers. Timing varies with: shared cache page residency, ASLR randomization, and filesystem metadata state.
- **Raw entropy:** A (7.33 bits/byte)
- **Speed:** 1.2s
- **Platform:** macOS/Linux

#### 29. `vm_page_timing` — Mach VM page fault timing
- **Physics:** Times Mach VM operations (mmap/munmap cycles). Each operation requires: VM map entry allocation, page table updates, TLB shootdown across cores (IPI interrupt), and physical page management. Timing depends on: VM map fragmentation, physical memory pressure, and cross-core synchronization.
- **Raw entropy:** A (7.85 bits/byte)
- **Speed:** 110ms
- **Platform:** macOS/Linux

#### 30. `spotlight_timing` — Spotlight index query timing
- **Physics:** Queries Spotlight's metadata index (mdls) and measures response time. The index is a complex B-tree/inverted index structure. Query timing depends on: index size, disk cache residency, concurrent indexing activity, and filesystem metadata state.
- **Raw entropy:** A (7.00 bits/byte)
- **Speed:** 12.7s (spawns mdls process per query)
- **Platform:** macOS

### Frontier Sources (Standalone)

> **Standalone** sources each harvest one independent physical entropy domain.
> **Composite** sources combine multiple standalone sources — see the Composite section below.

#### 31. `amx_timing` — Apple Matrix eXtensions coprocessor dispatch jitter (improved)
- **Physics:** Dispatches SGEMM (single-precision matrix multiply) operations to the AMX coprocessor via the Accelerate framework (`cblas_sgemm`) with varying matrix sizes [16,32,48,64,96,128]. The AMX is a dedicated coprocessor with its own instruction pipeline, shared across all CPU cores. Timing jitter comes from: AMX dispatch queue arbitration, memory controller bandwidth contention during matrix data transfer, thermal throttling affecting coprocessor clock frequency, and cache line eviction patterns for the matrix data.
- **Improvements:** Von Neumann debiasing fixes severe bias (H∞ 0.379 → estimated 2.0+). Alternates between transposed and non-transposed multiplies to exercise different AMX pipeline paths. Interleaves memory operations (64KB cache-thrashing) between AMX dispatches to disrupt pipeline steady-state.
- **Config:** `AMXTimingConfig { matrix_sizes, interleave_memory_ops, von_neumann_debias }`
- **Raw entropy:** B (5.19 bits/byte Shannon; min-entropy improved by debiasing)
- **Speed:** 50ms — very fast
- **Platform:** macOS Apple Silicon only (requires Accelerate framework)

#### 32. `thread_lifecycle` — pthread create/join cycle timing
- **Physics:** Spawns and joins threads with variable workloads per iteration. The kernel must: allocate a thread structure from the zone allocator, assign a TID, create a kernel stack, choose a CPU core (P-core vs E-core scheduling decision), set up the Mach thread port, and reverse all of this on join. Timing captures: zone allocator fragmentation, scheduler run-queue depth, P/E core migration latency, stack page zeroing, and thread port namespace operations. Richest source by unique LSBs (89/256). NIST: 89.5/100 (Grade A).
- **Raw entropy:** A (6.79 bits/byte)
- **Speed:** 80ms
- **Platform:** All (uses pthreads)

#### 33. `mach_ipc` — Mach port complex OOL message timing (reworked)
- **Physics:** Sends complex Mach messages with out-of-line (OOL) memory descriptors via `mach_msg()`, round-robining across multiple ports. OOL descriptors force kernel VM remapping (`vm_map_copyin`/`vm_map_copyout`) which exercises page table operations, physical page allocation, and TLB updates. Round-robin across ports with varied queue depths creates namespace contention. A receiver thread drains messages to prevent queue backup.
- **Improvements:** Complete rework from simple port allocate/deallocate to complex OOL messages forcing VM remapping. Round-robin across 8 ports (configurable) for namespace contention. Receiver thread creates cross-thread scheduling interference. Falls back to simple mode on port allocation failure.
- **Config:** `MachIPCConfig { num_ports, ool_size, use_complex_messages }`
- **Raw entropy:** B (4.92 bits/byte; expected improvement from OOL VM remapping)
- **Speed:** 40ms
- **Platform:** macOS only

#### 34. `tlb_shootdown` — TLB invalidation via variable-count mprotect (improved)
- **Physics:** Allocates a large memory region (256 pages, configurable) and toggles page protection using `mprotect()` on varying subsets. Each protection change sends TLB shootdown IPIs to ALL cores. Varying the page count (8-128, configurable) creates different IPI patterns. Using different memory region offsets each time prevents TLB prefetch patterns. Delta-of-deltas extraction captures timing variance between consecutive shootdowns.
- **Improvements:** Variable page counts per measurement create different IPI patterns. Random region offsets prevent systematic TLB caching. Variance-based extraction (delta-of-deltas) removes systematic bias and amplifies nondeterministic components.
- **Config:** `TLBShootdownConfig { page_count_range, region_pages, measure_variance }`
- **Raw entropy:** A (6.46 bits/byte; min-entropy improved by variance extraction)
- **Speed:** 30ms
- **Platform:** All

#### 35. `pipe_buffer` — Multi-pipe kernel zone allocator competition (improved)
- **Physics:** Creates multiple pipes simultaneously (4, configurable), writes variable-size data (1-4096 bytes) through them in round-robin, reads it back, measuring contention. Multiple pipes compete for kernel pipe zone and mbuf allocations, creating cross-CPU magazine transfer contention. Non-blocking mode captures EAGAIN failure path timing. Periodically creates/destroys extra pipes for zone allocator churn.
- **Improvements:** Multi-pipe pool (4 pipes, configurable) for zone allocator contention. Extended buffer size range (1-4096 vs 1-256). Non-blocking mode with EAGAIN timing. Periodic pipe create/destroy for zone churn. Falls back to single-pipe mode on allocation failure.
- **Config:** `PipeBufferConfig { num_pipes, min_write_size, max_write_size, non_blocking }`
- **Raw entropy:** C (3.22 bits/byte; expected improvement from contention)
- **Speed:** 10ms — fastest frontier source
- **Platform:** All (uses POSIX pipes)

#### 36. `kqueue_events` — Kqueue event multiplexing timing (new)
- **Physics:** Registers diverse kqueue event types simultaneously — timers (EVFILT_TIMER, 8 at different intervals), file watchers (EVFILT_VNODE on temp files), and socket monitors (EVFILT_READ/EVFILT_WRITE on socket pairs) — then measures `kevent()` notification timing. Timer events capture kernel timer coalescing and interrupt jitter. File watchers exercise VFS/APFS notification paths. Socket events capture mbuf allocator timing. A background thread periodically pokes watched files and sockets to generate asynchronous events. Multiple simultaneous watchers create knote lock contention and dispatch queue interference.
- **Config:** `KqueueEventsConfig { num_file_watchers, num_timers, num_sockets, timeout_ms }`
- **Raw entropy:** Estimated A (high min-entropy from diverse event interference)
- **Speed:** ~50ms
- **Platform:** macOS/BSD (uses kqueue)

#### 37. `dvfs_race` — Cross-core DVFS frequency race (new)
- **Physics:** Spawns two threads running tight counting loops on different CPU cores. After a ~2μs race window, the absolute difference in iteration counts captures physical frequency jitter from independent DVFS (Dynamic Voltage and Frequency Scaling) controllers. Apple Silicon P-core and E-core clusters have separate voltage/frequency domains that adjust asynchronously based on thermal state, power budget, and workload. The scheduler's core placement, cache coherence latency for the stop signal, and per-core frequency transitions all contribute independent nondeterminism.
- **Raw entropy:** A (7.96 Shannon, H∞ = 7.288 bits/byte from PoC — highest of any discovered source)
- **Speed:** <100ms — very fast
- **Platform:** All (uses std::thread)

#### 38. `cas_contention` — Multi-thread atomic CAS arbitration contention (new)
- **Physics:** Spawns 4 threads performing atomic compare-and-swap operations on shared targets spread across 128-byte-aligned cache lines. The hardware coherence engine (MOESI protocol on Apple Silicon) must arbitrate concurrent exclusive-access requests. This arbitration is physically nondeterministic due to interconnect fabric latency variations, thermal state, and traffic from other cores/devices. XOR-combining timing measurements from all threads amplifies the arbitration entropy.
- **Config:** `CASContentionConfig { num_threads }`
- **Raw entropy:** D (3.02 Shannon, H∞ = 2.619 bits/byte from PoC with 4-thread XOR combine)
- **Speed:** <100ms — very fast
- **Platform:** All (uses std::thread + atomics)

### Composite Sources

> Composite sources do not measure a single independent entropy domain.
> They combine multiple standalone sources for higher-quality output.
> In CLI output, composite sources are marked with `[COMPOSITE]`.

#### 39. `interleaved_frontier` — **[COMPOSITE]** Cross-source interference entropy (new)
- **Physics:** Rapidly alternates between all 8 frontier sources in round-robin, collecting small 4-byte batches from each. Each source's system perturbations affect the next source's measurements: AMX dispatch affects memory controller state which affects TLB shootdown timing; pipe zone allocations affect kernel magazine state which affects Mach port timing; thread scheduling decisions affect kqueue timer delivery. Measures both the transition timing between sources and XORs it with collected source bytes.
- **Raw entropy:** Estimated A (cross-source interference is independent entropy)
- **Speed:** ~200ms (sum of individual source costs)
- **Platform:** All (requires ≥2 available frontier sources)
- **Note:** This is a composite source — it combines all standalone frontier sources.

## Grade Distribution (Raw Output)

| Grade | Count | Sources |
|-------|-------|---------|
| A | 11+ | dns_timing, tcp_connect_timing, memory_timing, gpu_timing, page_fault_timing, dyld_timing, vm_page_timing, spotlight_timing, **thread_lifecycle**, **tlb_shootdown**, **kqueue_events**\*, **dvfs_race**, **interleaved_frontier**\* |
| B | 6 | clock_jitter, cache_contention, compression_timing, dispatch_queue, **amx_timing**, **mach_ipc** |
| C | 7 | process_table, ioregistry, disk_io, bluetooth_noise, dram_row_buffer, cpu_io_beat, **pipe_buffer** |
| D | 7 | sysctl_deltas, vmstat_deltas, cpu_memory_beat, multi_domain_beat, hash_timing, sleep_jitter*, **cas_contention** |
| F | 2 | mach_timing, speculative_execution |
| N/A | 4 | audio_noise, camera_noise, wifi_noise, sensor_noise |

\*Estimated grade — requires benchmarking. Sleep_jitter oscillates between D and F across runs.

## Notes

- **All measurements are raw (unconditioned).** After SHA-256 conditioning, every source produces 8.0 bits/byte (Grade A). The raw grades show actual hardware entropy density.
- **Speed** is wall-clock time for 5000 samples on an M4 Mac mini.
- **Low-grade sources are still valuable** in the pool — even 1 bit/byte of true hardware entropy per source, XOR-combined across 20+ sources, produces strong composite output.
- **Network sources** (dns_timing, tcp_connect_timing, gpu_timing) are slow but highest quality. Fast sources (clock_jitter, mach_timing, silicon sources) provide rapid bulk entropy at lower density.
- **Configurable sources** — frontier sources accept config structs with sensible defaults. Use custom configs to tune for specific hardware or entropy requirements.
