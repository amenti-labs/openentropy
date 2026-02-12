# Entropy Source Catalog

30 sources across 7 categories, each exploiting a different physical phenomenon inside your computer. Every source implements the `EntropySource` trait and produces raw `Vec<u8>` samples that are fed into the entropy pool.

## Source Summary

| # | Source | Category | Physics | Est. Rate | Platform |
|---|--------|----------|---------|-----------|----------|
| 1 | `clock_jitter` | Timing | PLL phase noise between clocks | ~500 b/s | All |
| 2 | `mach_timing` | Timing | ARM system counter LSB jitter | ~300 b/s | macOS |
| 3 | `sleep_jitter` | Timing | OS scheduler wake-up jitter | ~400 b/s | All |
| 4 | `sysctl` | System | Kernel counter fluctuations | ~2000 b/s | macOS, Linux |
| 5 | `vmstat` | System | VM subsystem page counters | ~500 b/s | macOS, Linux |
| 6 | `process` | System | Process table snapshot hash | ~300 b/s | All |
| 7 | `dns_timing` | Network | DNS resolution latency jitter | ~400 b/s | All |
| 8 | `tcp_connect` | Network | TCP handshake timing variance | ~300 b/s | All |
| 9 | `wifi_rssi` | Network | WiFi signal strength noise floor | ~200 b/s | macOS |
| 10 | `disk_io` | Hardware | Block device I/O timing jitter | ~500 b/s | All |
| 11 | `memory_timing` | Hardware | DRAM access timing variations | ~800 b/s | All |
| 12 | `gpu_timing` | Hardware | GPU compute dispatch jitter | ~600 b/s | macOS (Metal) |
| 13 | `audio_noise` | Hardware | Microphone thermal noise floor | ~1000 b/s | Requires mic |
| 14 | `camera_noise` | Hardware | Camera sensor dark current | ~2000 b/s | Requires camera |
| 15 | `sensor_noise` | Hardware | SMC sensor ADC jitter | ~400 b/s | macOS |
| 16 | `bluetooth_noise` | Hardware | BLE ambient RF environment | ~200 b/s | macOS |
| 17 | `ioregistry` | Hardware | IOKit registry value mining | ~500 b/s | macOS |
| 18 | `dram_row_buffer` | Silicon | DRAM row buffer hit/miss timing | ~3000 b/s | All |
| 19 | `cache_contention` | Silicon | L1/L2 cache contention timing | ~2500 b/s | All |
| 20 | `page_fault_timing` | Silicon | mmap/munmap page fault latency | ~1500 b/s | All |
| 21 | `speculative_execution` | Silicon | Branch predictor state timing | ~2000 b/s | All |
| 22 | `cpu_io_beat` | Cross-Domain | CPU vs I/O clock beat frequency | ~300 b/s | All |
| 23 | `cpu_memory_beat` | Cross-Domain | CPU vs memory controller beat | ~400 b/s | All |
| 24 | `multi_domain_beat` | Cross-Domain | Multi-subsystem interference | ~500 b/s | All |
| 25 | `compression_timing` | Novel | zlib compression timing oracle | ~300 b/s | All |
| 26 | `hash_timing` | Novel | SHA-256 timing data-dependency | ~400 b/s | All |
| 27 | `dispatch_queue` | Novel | Thread pool scheduling jitter | ~500 b/s | macOS |
| 28 | `dyld_timing` | Novel | Dynamic linker dlsym() timing | ~300 b/s | macOS, Linux |
| 29 | `vm_page_timing` | Novel | Mach VM page allocation timing | ~400 b/s | macOS |
| 30 | `spotlight_timing` | Novel | Spotlight metadata query timing | ~200 b/s | macOS |

---

## Timing Sources

### 1. `clock_jitter`

**Category:** Timing
**Struct:** `ClockJitterSource`
**Platform:** All
**Estimated Rate:** ~500 b/s

**Physics:** Measures phase noise between two independent clock oscillators (`Instant` vs `SystemTime`). Each clock is driven by a separate PLL (Phase-Locked Loop) on the SoC. Thermal noise in the PLL's voltage-controlled oscillator (VCO) causes random frequency drift. The LSBs of their difference are genuine analog entropy from crystal oscillator physics.

**Implementation:** Reads both `Instant::now()` and `SystemTime::now()` as close together as possible. The monotonic clock is read twice to get a nanos-since-first-read delta. The two clock values are XORed together and the lowest byte is taken as the sample.

**Conditioning:** Raw XOR of clock LSBs (no additional per-source conditioning).

---

### 2. `mach_timing`

**Category:** Timing
**Struct:** `MachTimingSource`
**Platform:** macOS only
**Estimated Rate:** ~300 b/s

**Physics:** Reads the ARM system counter (`mach_absolute_time()`) at sub-nanosecond resolution with variable micro-workloads between samples. The timing jitter comes from CPU pipeline state: instruction reordering, branch prediction, cache state, interrupt coalescing, and power-state transitions.

**Implementation:** Calls `mach_absolute_time()` via FFI before and after a variable-length micro-workload (LCG iterations). The delta between timestamps captures pipeline jitter. Oversamples by 16x to compensate for conditioning losses.

**Conditioning:** Three-stage pipeline:
1. Raw LSBs from timestamp deltas
2. Von Neumann debiasing (discards same-bit pairs, ~75% data loss)
3. Chained SHA-256 in 64-byte blocks

---

### 3. `sleep_jitter`

**Category:** Timing
**Struct:** `SleepJitterSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** Requests zero-duration sleeps (`thread::sleep(Duration::ZERO)`) and measures actual wake-up time. The jitter captures OS scheduler non-determinism: timer interrupt granularity (1-4ms), thread priority decisions, runqueue length, and thermal-dependent clock frequency scaling (DVFS).

**Implementation:** Oversamples 4x. Measures the elapsed time for each zero-length sleep, computes consecutive deltas, XORs adjacent deltas for whitening, then extracts LSBs.

**Conditioning:** XOR whitening of adjacent deltas, then chained SHA-256 block conditioning.

---

## System Sources

### 4. `sysctl`

**Category:** System
**Struct:** `SysctlSource`
**Platform:** macOS, Linux
**Estimated Rate:** ~2000 b/s

**Physics:** Reads 50+ kernel counters via `sysctl` that fluctuate due to interrupt handling, context switches, network packets, and I/O completions. The counters reflect the aggregate behavior of the entire system -- unpredictable at the LSB level.

**Implementation:** Executes `/usr/sbin/sysctl` as a subprocess, parses the key-value output, and hashes the entire counter snapshot.

---

### 5. `vmstat`

**Category:** System
**Struct:** `VmstatSource`
**Platform:** macOS, Linux
**Estimated Rate:** ~500 b/s

**Physics:** Virtual memory subsystem counters -- page faults, pageins, swapins, reactivations -- driven by unpredictable memory access patterns from all running processes.

**Implementation:** Executes `vm_stat` as a subprocess and parses counter values. Delta snapshots between collection rounds capture the change in system activity.

---

### 6. `process`

**Category:** System
**Struct:** `ProcessSource`
**Platform:** All
**Estimated Rate:** ~300 b/s

**Physics:** Process table snapshot -- PIDs, memory usage, CPU times, thread counts. Changes unpredictably with system activity. Each snapshot reflects the combined state of all processes on the machine.

**Implementation:** Executes `ps` as a subprocess and hashes the complete process listing via SHA-256.

**Benchmark (raw, pre-pool conditioning):** Shannon entropy H=7.746 (96.8%), compression ratio 0.985. Passes 11/31 NIST tests individually. The conditioned pool output passes all tests.

---

## Network Sources

### 7. `dns_timing`

**Category:** Network
**Struct:** `DNSTimingSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** DNS resolution latency includes network propagation delay, server load, routing jitter, cache state (cold vs warm), and TCP/UDP retransmission timing. Each query traverses a unique path through the internet.

**Implementation:** Sends raw UDP DNS queries via `std::net::UdpSocket` and measures round-trip time at nanosecond resolution.

---

### 8. `tcp_connect`

**Category:** Network
**Struct:** `TCPConnectSource`
**Platform:** All
**Estimated Rate:** ~300 b/s

**Physics:** TCP three-way handshake timing varies with network congestion, server load, routing decisions, and kernel networking stack state. The SYN-SYNACK-ACK round-trip captures physical network conditions.

**Implementation:** Times `TcpStream::connect()` calls to well-known hosts and extracts jitter from the connection latency.

---

### 9. `wifi_rssi`

**Category:** Network
**Struct:** `WiFiRSSISource`
**Platform:** macOS (CoreWLAN)
**Estimated Rate:** ~200 b/s

**Physics:** WiFi received signal strength indicator (RSSI) includes multipath fading, co-channel interference from other networks, thermal noise floor of the radio, and environmental factors (people moving, doors opening). The noise floor fluctuation is genuine RF thermal noise.

**Implementation:** Uses `/usr/sbin/networksetup` or the airport CLI utility to read RSSI values from the WiFi interface.

---

## Hardware Sources

### 10. `disk_io`

**Category:** Hardware
**Struct:** `DiskIOSource`
**Platform:** All
**Estimated Rate:** ~500 b/s

**Physics:** Block device I/O latency varies with physical disk arm position (HDD), NAND channel contention (SSD), write-back cache state, wear-leveling decisions, and thermal effects on read thresholds. Each I/O operation traverses a unique path through the storage controller.

**Implementation:** Creates a temporary file via `tempfile`, performs random reads, and measures per-operation latency.

---

### 11. `memory_timing`

**Category:** Hardware
**Struct:** `MemoryTimingSource`
**Platform:** All
**Estimated Rate:** ~800 b/s

**Physics:** DRAM access timing varies with row buffer state, refresh timing, and thermal effects. Memory controller scheduling decisions (open-page vs closed-page policy) create measurable latency variation.

**Implementation:** Allocates memory via `mmap`, performs accesses at varying offsets, and measures timing jitter.

**Benchmark (raw):** Shannon entropy H=2.909, 102,500 samples in 0.1s. Grade C individually; pool conditioning lifts to Grade A.

---

### 12. `gpu_timing`

**Category:** Hardware
**Struct:** `GPUTimingSource`
**Platform:** macOS (Metal)
**Estimated Rate:** ~600 b/s

**Physics:** GPU compute kernel dispatch timing varies with shader occupancy, memory bandwidth contention, thermal throttling micro-decisions, and warp/SIMD group scheduling. The GPU clock domain is independent from the CPU, adding cross-domain beat frequency effects.

**Implementation:** Times `/usr/bin/sips` image processing operations, which invoke the GPU compute pipeline. The dispatch-to-completion latency captures GPU scheduling jitter.

---

### 13. `audio_noise`

**Category:** Hardware
**Struct:** `AudioNoiseSource`
**Platform:** Requires microphone (built-in or external)
**Estimated Rate:** ~1000 b/s

**Physics:** Microphone preamp thermal noise (Johnson-Nyquist noise). Even with no audio input, the ADC captures genuine thermal fluctuations from the preamplifier's input resistance: V_noise = sqrt(4kTR * bandwidth). This is fundamental thermodynamic noise.

**Implementation:** Captures audio via `ffmpeg` or CoreAudio, extracting the noise floor from silent recordings.

---

### 14. `camera_noise`

**Category:** Hardware
**Struct:** `CameraNoiseSource`
**Platform:** Requires camera (built-in or USB)
**Estimated Rate:** ~2000 b/s

**Physics:** Image sensor dark current -- thermally generated electron-hole pairs in silicon photodiodes. At the quantum level, each pixel independently generates charge carriers through thermal excitation across the silicon bandgap. The process is fundamentally quantum mechanical (Poisson-distributed photon/electron events).

**Implementation:** Captures frames via `ffmpeg` with the lens covered or in darkness. Pixel-to-pixel variation in dark frames is genuine quantum noise.

**Benchmark (raw):** Shannon entropy H=1.976 (4 unique values), but 921,600 samples per frame. Low per-sample entropy compensated by massive parallelism.

---

### 15. `sensor_noise`

**Category:** Hardware
**Struct:** `SensorNoiseSource`
**Platform:** macOS
**Estimated Rate:** ~400 b/s

**Physics:** Apple SMC (System Management Controller) sensor readout jitter. Every ADC (analog-to-digital converter) reading contains quantization noise plus thermal noise from the sensor element itself. With dozens of independent sensors (voltage rails, current sense, thermal diodes) sampled rapidly, you get parallel independent entropy streams.

**Implementation:** Reads sensor values via `ioreg` and extracts ADC quantization noise from the LSBs.

---

### 16. `bluetooth_noise`

**Category:** Hardware
**Struct:** `BluetoothNoiseSource`
**Platform:** macOS (CoreBluetooth)
**Estimated Rate:** ~200 b/s

**Physics:** BLE (Bluetooth Low Energy) ambient RF environment scanning. Each advertising device's RSSI fluctuates with multipath fading, movement, and interference. BLE advertising interval jitter reflects each device's independent clock drift. Channel selection across 37 advertising channels adds frequency-domain diversity.

**Implementation:** Uses `system_profiler` to enumerate BLE devices and their signal strengths.

---

### 17. `ioregistry`

**Category:** Hardware
**Struct:** `IORegistryEntropySource`
**Platform:** macOS
**Estimated Rate:** ~500 b/s

**Physics:** The IOKit registry (`ioreg -l -w0`) exposes the entire hardware tree -- thousands of properties including real-time sensor readings, power states, link status counters, and hardware event timestamps. These values change continuously due to hardware activity.

**Implementation:** Reads the IOKit registry tree and hashes the output. Buried counters include AppleARMIODevice sensor readings, IOHIDSystem event timestamps, battery impedance noise, audio PLL lock status, and Thunderbolt link state transitions.

---

## Silicon Microarchitecture Sources

These four sources exploit physical effects at the CPU and DRAM silicon level. They produce the highest entropy rates because they operate at nanosecond timescales with minimal software overhead.

### 18. `dram_row_buffer`

**Category:** Silicon
**Struct:** `DRAMRowBufferSource`
**Platform:** All
**Estimated Rate:** ~3000 b/s

**Physics:** DRAM is organized into rows of capacitor cells within banks. Accessing an already-open row (hit) is fast; accessing a different row requires a precharge cycle followed by activation (miss), which takes significantly longer. The exact timing depends on:

- Physical address mapping (which bank and row the virtual address maps to)
- Row buffer state from ALL other system activity (shared resource)
- Memory controller scheduling policy and queue depth
- DRAM refresh interference (periodic refresh steals bandwidth)
- Temperature effects on charge retention and sense amplifier timing

**Implementation:** Allocates a 32 MB buffer (exceeds L2/L3 cache capacity), touches all pages to ensure residency, then performs random volatile reads timed via `mach_absolute_time()`. The access pattern deliberately crosses row boundaries.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

### 19. `cache_contention`

**Category:** Silicon
**Struct:** `CacheContentionSource`
**Platform:** All
**Estimated Rate:** ~2500 b/s

**Physics:** The CPU cache hierarchy is a shared resource. Cache timing depends on what every other process and hardware unit is doing. A cache miss requires main memory access (~100+ ns vs ~1 ns for L1 hit). By alternating between sequential (cache-friendly) and random (cache-hostile) access patterns, this source maximizes the observable timing variation.

**Implementation:** Allocates an 8 MB buffer (spans L2 boundary). On even rounds, performs sequential reads (cache-friendly). On odd rounds, performs random reads (cache-hostile). The timing difference between rounds captures the cache state influenced by all concurrent system activity.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

### 20. `page_fault_timing`

**Category:** Silicon
**Struct:** `PageFaultTimingSource`
**Platform:** All
**Estimated Rate:** ~1500 b/s

**Physics:** Triggers and times minor page faults via `mmap`/`munmap` cycles. Page fault resolution requires:

1. TLB (Translation Lookaside Buffer) lookup and miss
2. Hardware page table walk (up to 4 levels on ARM64)
3. Physical page allocation from the kernel free list
4. Zero-fill of the page for security
5. TLB entry installation

The timing depends on physical memory fragmentation, the kernel's page allocator state, and memory pressure from other processes.

**Implementation:** In each cycle, maps 4 anonymous pages via `mmap`, touches each page to trigger a fault (timed individually via `Instant`), then unmaps. Fresh pages are allocated each cycle.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

### 21. `speculative_execution`

**Category:** Silicon
**Struct:** `SpeculativeExecutionSource`
**Platform:** All
**Estimated Rate:** ~2000 b/s

**Physics:** The CPU's branch predictor maintains per-address history tables that depend on ALL previously executed code across all processes on the core. Mispredictions cause pipeline flushes (~15 cycle penalty on Apple M4). By running data-dependent branches with unpredictable outcomes (LCG-generated), we capture the predictor's internal state.

**Implementation:** Executes batches of data-dependent branches using an LCG (seeded from the high-resolution clock). Batch sizes vary with the iteration index to create different branch predictor pressure levels. Three levels of branching per iteration maximize predictor state perturbation.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

## Cross-Domain Beat Frequency Sources

These sources exploit the interference patterns that arise when independent clock domains interact. Each subsystem (CPU, memory controller, I/O bus) has its own PLL with independent phase noise. When operations cross domain boundaries, the beat frequency of their jitter creates entropy.

### 22. `cpu_io_beat`

**Category:** Cross-Domain
**Struct:** `CPUIOBeatSource`
**Platform:** All
**Estimated Rate:** ~300 b/s

**Physics:** The CPU and I/O subsystem run on independent clocks. Their interaction creates beat frequency patterns driven by two independent noise sources. CPU work and file I/O are interleaved, and the timing captures the cross-domain interference.

**Implementation:** Alternates between CPU-bound work and file I/O operations, measuring the total time for each interleaved operation via `mach_absolute_time()`.

---

### 23. `cpu_memory_beat`

**Category:** Cross-Domain
**Struct:** `CPUMemoryBeatSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** The CPU clock and memory controller operate at different frequencies with independent PLLs. The beat pattern captures phase noise from both oscillators. Memory-bound operations experience latency that depends on the memory controller's queue state and DRAM timing.

**Implementation:** Alternates between CPU computation and random memory accesses, measuring the timing of each round.

---

### 24. `multi_domain_beat`

**Category:** Cross-Domain
**Struct:** `MultiDomainBeatSource`
**Platform:** All
**Estimated Rate:** ~500 b/s

**Physics:** Interference pattern from 3+ independent subsystems (CPU, memory, I/O, and syscall). Multi-source beats have higher entropy density than pairwise interactions because they combine noise from more independent oscillators.

**Implementation:** Interleaves CPU work, memory accesses, file I/O, and system calls in rapid succession, capturing the composite timing.

---

## Novel Sources

### 25. `compression_timing`

**Category:** Novel
**Struct:** `CompressionTimingSource`
**Platform:** All
**Estimated Rate:** ~300 b/s

**Physics:** zlib compression time is data-dependent. Different byte patterns trigger different code paths in the Lempel-Ziv algorithm, creating measurable timing variation. The Huffman encoding step and hash table lookups are particularly sensitive to input data.

**Implementation:** Compresses varying data via `flate2` and measures per-operation latency.

---

### 26. `hash_timing`

**Category:** Novel
**Struct:** `HashTimingSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** SHA-256 hashing time varies subtly with input data due to memory access patterns, cache state, and microarchitectural effects. While SHA-256 is designed to be constant-time, hardware-level effects (cache line fills, TLB misses) create measurable jitter.

**Implementation:** Hashes varying data via `sha2::Sha256` and extracts timing jitter.

---

### 27. `dispatch_queue`

**Category:** Novel
**Struct:** `DispatchQueueSource`
**Platform:** macOS
**Estimated Rate:** ~500 b/s

**Physics:** Grand Central Dispatch (GCD) queue scheduling jitter. Work items submitted to dispatch queues experience non-deterministic queueing delays that depend on thread pool state, priority inversion, and system load.

**Implementation:** Spawns thread pool tasks and measures the scheduling latency -- the time between submission and execution start.

---

### 28. `dyld_timing`

**Category:** Novel
**Struct:** `DyldTimingSource`
**Platform:** macOS, Linux
**Estimated Rate:** ~300 b/s

**Physics:** Dynamic linker `dlsym()` lookup timing varies with symbol table size, hash table collisions, shared library cache state, and address space layout. ASLR (Address Space Layout Randomization) adds per-process variation.

**Implementation:** Times `libloading::Library::new()` operations for dynamic library loading.

---

### 29. `vm_page_timing`

**Category:** Novel
**Struct:** `VMPageTimingSource`
**Platform:** macOS
**Estimated Rate:** ~400 b/s

**Physics:** Mach VM page allocation latency depends on the kernel's free page list state, physical memory fragmentation, memory pressure from other processes, and page zeroing overhead.

**Implementation:** Performs `mmap`/`munmap` cycles and times the allocation path.

---

### 30. `spotlight_timing`

**Category:** Novel
**Struct:** `SpotlightTimingSource`
**Platform:** macOS
**Estimated Rate:** ~200 b/s

**Physics:** Spotlight metadata query timing reflects the Spotlight index size, disk cache state, concurrent indexing activity, and file system metadata access patterns.

**Implementation:** Times `mdls` (metadata listing) operations against system files.

---

## Platform Availability

| Platform | Available Sources | Notes |
|----------|:-----------------:|-------|
| **MacBook (M-series)** | **30/30** | Full suite -- WiFi, BLE, camera, mic, all sensors |
| **Mac Mini/Studio/Pro** | 27-28/30 | Most sources -- no built-in camera or mic on some models |
| **Intel Mac** | ~20/30 | Timing, system, network, disk sources work; some silicon sources are ARM-specific |
| **Linux** | 10-15/30 | Timing, network, disk, process, silicon sources work; no macOS-specific sources |

The package gracefully detects available hardware via `detect_available_sources()` and only activates sources that pass `is_available()`. MacBooks provide the richest entropy because they pack the most sensors into one device.

## Entropy Quality Notes

Individual source quality varies. Raw (unconditioned) source output often has bias and correlation:

- **Shannon entropy** of raw sources typically ranges from 2-8 bits/byte depending on the source
- **Raw NIST test pass rates** for individual sources range from 11/31 to 28/31
- **After pool conditioning** (SHA-256 + mixing + os.urandom), output consistently passes 28-31/31 NIST tests

The conditioning pipeline is designed to extract the genuine entropy from biased sources and produce cryptographic-quality output regardless of individual source weakness. This is consistent with the NIST SP 800-90B approach: measure the min-entropy of the raw source, then apply an approved conditioning function (SHA-256) to concentrate it.

## Adding a New Source

To add a new entropy source to the Rust codebase:

1. Create a struct implementing `EntropySource` in the appropriate file under `crates/esoteric-core/src/sources/`
2. Define a static `SourceInfo` with the physics explanation, category, and platform requirements
3. Register the source in `all_sources()` in `crates/esoteric-core/src/sources/mod.rs`
4. Add unit tests in the same file
5. Document the physics in this file

See [ADDING_SOURCES.md](ADDING_SOURCES.md) for the Python-side equivalent.
