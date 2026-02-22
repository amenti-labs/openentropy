# Entropy Source Catalog

47 sources across 12 mechanism-based categories, each exploiting a different physical phenomenon inside your computer. Every source implements the `EntropySource` trait and produces raw `Vec<u8>` samples that are fed into the entropy pool.

## Source Summary

| # | Source | Category | Physics | Est. Rate | Platform |
|---|--------|----------|---------|-----------|----------|
| 1 | `clock_jitter` | Timing | PLL phase noise between clocks | ~500 b/s | All |
| 2 | `mach_timing` | Timing | ARM system counter LSB jitter | ~300 b/s | macOS |
| 3 | `sleep_jitter` | Scheduling | OS scheduler wake-up jitter | ~400 b/s | All |
| 4 | `sysctl` | System | Kernel counter fluctuations | ~2000 b/s | macOS, Linux |
| 5 | `vmstat` | System | VM subsystem page counters | ~500 b/s | macOS, Linux |
| 6 | `process` | System | Process table snapshot hash | ~300 b/s | All |
| 7 | `dns_timing` | Network | DNS resolution latency jitter | ~400 b/s | All |
| 8 | `tcp_connect` | Network | TCP handshake timing variance | ~300 b/s | All |
| 9 | `wifi_rssi` | Network | WiFi signal strength noise floor | ~200 b/s | macOS |
| 10 | `disk_io` | IO | Block device I/O timing jitter | ~500 b/s | All |
| 11 | `memory_timing` | Timing | DRAM access timing variations | ~800 b/s | All |
| 12 | `gpu_timing` | GPU | GPU compute dispatch jitter | ~600 b/s | macOS (Metal) |
| 13 | `audio_noise` | Sensor | Microphone ADC thermal noise (Johnson-Nyquist) | ~10000 b/s | Requires mic |
| 14 | `camera_noise` | Sensor | Camera sensor noise (read noise + dark current) | ~4000 b/s | Requires camera |
| 15 | `bluetooth_noise` | Sensor | BLE ambient RF environment | ~200 b/s | macOS |
| 17 | `ioregistry` | System | IOKit registry value mining | ~500 b/s | macOS |
| 18 | `dram_row_buffer` | Timing | DRAM row buffer hit/miss timing | ~3000 b/s | All |
| 19 | `cache_contention` | Timing | L1/L2 cache contention timing | ~2500 b/s | All |
| 20 | `page_fault_timing` | Timing | mmap/munmap page fault latency | ~1500 b/s | All |
| 21 | `speculative_execution` | Microarch | Branch predictor state timing | ~2000 b/s | All |
| 22 | `cpu_io_beat` | Composite | CPU vs I/O clock beat frequency | ~300 b/s | All |
| 23 | `cpu_memory_beat` | Composite | CPU vs memory controller beat | ~400 b/s | All |
| 24 | `compression_timing` | Signal | zlib compression timing oracle | ~300 b/s | All |
| 26 | `hash_timing` | Signal | SHA-256 timing data-dependency | ~400 b/s | All |
| 27 | `dispatch_queue` | Scheduling | Thread pool scheduling jitter | ~500 b/s | macOS |
| 28 | `vm_page_timing` | Timing | Mach VM page allocation timing | ~400 b/s | macOS |
| 30 | `spotlight_timing` | Signal | Spotlight metadata query timing | ~200 b/s | macOS |
| 31 | `amx_timing` | Microarch | Apple AMX coprocessor dispatch jitter | ~500 b/s | macOS (ARM) |
| 32 | `thread_lifecycle` | Scheduling | pthread create/join cycle timing | ~400 b/s | All |
| 33 | `mach_ipc` | IPC | Mach port IPC allocation timing | ~300 b/s | macOS |
| 34 | `tlb_shootdown` | Microarch | mprotect() TLB invalidation IPI latency | ~400 b/s | All |
| 35 | `pipe_buffer` | IPC | Kernel zone allocator via pipe lifecycle | ~200 b/s | All |
| 36 | `kqueue_events` | IPC | BSD kqueue event multiplexing jitter | ~300 b/s | macOS |
| 37 | `dvfs_race` | Microarch | Cross-core DVFS frequency race | ~500 b/s | All |
| 38 | `cas_contention` | Microarch | Multi-thread atomic CAS arbitration | ~200 b/s | All |
| 39 | `keychain_timing` | IPC | macOS Keychain Services API timing | ~300 b/s | macOS |
| 40 | `denormal_timing` | Thermal | Denormal FPU micropower thermal noise | ~3000 b/s | All |
| 41 | `audio_pll_timing` | Thermal | Audio PLL clock drift (independent crystal) | ~4000 b/s | macOS |
| 42 | `usb_timing` | IO | USB bus transaction timing jitter | ~2000 b/s | macOS |
| 43 | `counter_beat` | Thermal | Two-oscillator beat: CPU 24 MHz vs audio PLL | ~2000 b/s | macOS (ARM) |
| 44 | `gpu_divergence` | GPU | GPU warp/SIMD divergence timing variance | ~500 b/s | macOS (Metal) |
| 45 | `iosurface_crossing` | GPU | IOSurface CPU↔GPU memory domain crossing | ~500 b/s | macOS |
| 46 | `fsync_journal` | IO | APFS journal commit timing (full storage stack) | ~200 b/s | All |
| 47 | `nvme_latency` | IO | NVMe command submission/completion timing | ~3000 b/s | macOS |
| 48 | `pdn_resonance` | Thermal | Power delivery network LC resonance noise | ~3000 b/s | All (ARM) |
| 49 | `display_pll` | Thermal | Display PLL phase noise (~533 MHz pixel clock) | ~2500 b/s | macOS (ARM) |
| 50 | `pcie_pll` | Thermal | PCIe PHY PLL jitter (Thunderbolt/PCIe clock domains) | ~2000 b/s | macOS (ARM) |

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

**Category:** Scheduling
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

## IO Sources

### 10. `disk_io`

**Category:** IO
**Struct:** `DiskIOSource`
**Platform:** All
**Estimated Rate:** ~500 b/s

**Physics:** Block device I/O latency varies with physical disk arm position (HDD), NAND channel contention (SSD), write-back cache state, wear-leveling decisions, and thermal effects on read thresholds. Each I/O operation traverses a unique path through the storage controller.

**Implementation:** Creates a temporary file via `tempfile`, performs random reads, and measures per-operation latency.

---

### 11. `memory_timing`

**Category:** Timing
**Struct:** `MemoryTimingSource`
**Platform:** All
**Estimated Rate:** ~800 b/s

**Physics:** DRAM access timing varies with row buffer state, refresh timing, and thermal effects. Memory controller scheduling decisions (open-page vs closed-page policy) create measurable latency variation.

**Implementation:** Allocates memory via `mmap`, performs accesses at varying offsets, and measures timing jitter.

**Benchmark (raw):** Shannon entropy H=2.909, 102,500 samples in 0.1s. Grade C individually; pool conditioning lifts to Grade A.

---

### 12. `gpu_timing`

**Category:** GPU
**Struct:** `GPUTimingSource`
**Platform:** macOS (Metal)
**Estimated Rate:** ~600 b/s

**Physics:** GPU compute kernel dispatch timing varies with shader occupancy, memory bandwidth contention, thermal throttling micro-decisions, and warp/SIMD group scheduling. The GPU clock domain is independent from the CPU, adding cross-domain beat frequency effects.

**Implementation:** Times `/usr/bin/sips` image processing operations, which invoke the GPU compute pipeline. The dispatch-to-completion latency captures GPU scheduling jitter.

---

### 13. `audio_noise`

**Category:** Sensor
**Struct:** `AudioNoiseSource`
**Platform:** Requires microphone (built-in or external)
**Estimated Rate:** ~10000 b/s

**Physics:** Records from the microphone ADC with no signal present. The LSBs capture Johnson-Nyquist noise — thermal agitation of electrons in the input impedance. At audio frequencies (up to ~44 kHz), this noise is entirely classical: hf << kT by a factor of ~10^8 at room temperature. Laptop audio codecs use CMOS input stages where channel thermal noise and 1/f flicker noise dominate; shot noise is negligible. V_noise = sqrt(4kTR * bandwidth).

**Implementation:** Captures 0.1s of raw signed 16-bit PCM audio via `ffmpeg` avfoundation, extracts the lower 4 bits of each sample, and packs nibble pairs into bytes.

---

### 14. `camera_noise`

**Category:** Sensor
**Struct:** `CameraNoiseSource`
**Platform:** Requires camera (built-in or USB)
**Estimated Rate:** ~4000 b/s

**Physics:** Captures frames from the camera sensor in darkness. Noise sources: (1) read noise from the amplifier — classical analog noise, dominates at short exposures (~95%+ of variance); (2) dark current from thermal carrier generation in silicon — classical at sensor operating temperatures; (3) dark current shot noise (Poisson counting) — ~1-5% of variance in typical webcams. The LSBs of pixel values mix all three components.

**Implementation:** Captures one grayscale frame via `ffmpeg` avfoundation (tries multiple input selectors), extracts the lower 4 bits of each pixel, and packs nibble pairs into bytes. 900ms timeout keeps the TUI responsive when camera permission is denied.

**Benchmark (raw):** Shannon entropy ~2 bits/byte. Low per-sample entropy but large frame sizes (640x480 = 307,200 pixels) provide substantial total entropy per capture.

---

### 15. `bluetooth_noise`

**Category:** Sensor
**Struct:** `BluetoothNoiseSource`
**Platform:** macOS (CoreBluetooth)
**Estimated Rate:** ~200 b/s

**Physics:** BLE (Bluetooth Low Energy) ambient RF environment scanning. Each advertising device's RSSI fluctuates with multipath fading, movement, and interference. BLE advertising interval jitter reflects each device's independent clock drift. Channel selection across 37 advertising channels adds frequency-domain diversity.

**Implementation:** Uses `system_profiler` to enumerate BLE devices and their signal strengths.

---

### 17. `ioregistry`

**Category:** System
**Struct:** `IORegistryEntropySource`
**Platform:** macOS
**Estimated Rate:** ~500 b/s

**Physics:** The IOKit registry (`ioreg -l -w0`) exposes the entire hardware tree -- thousands of properties including real-time sensor readings, power states, link status counters, and hardware event timestamps. These values change continuously due to hardware activity.

**Implementation:** Reads the IOKit registry tree and hashes the output. Buried counters include AppleARMIODevice sensor readings, IOHIDSystem event timestamps, battery impedance noise, audio PLL lock status, and Thunderbolt link state transitions.

---

## Microarchitecture Sources

These sources exploit physical effects at the CPU and DRAM silicon level. They produce the highest entropy rates because they operate at nanosecond timescales with minimal software overhead.

### 18. `dram_row_buffer`

**Category:** Timing
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

**Category:** Timing
**Struct:** `CacheContentionSource`
**Platform:** All
**Estimated Rate:** ~2500 b/s

**Physics:** The CPU cache hierarchy is a shared resource. Cache timing depends on what every other process and hardware unit is doing. A cache miss requires main memory access (~100+ ns vs ~1 ns for L1 hit). By alternating between sequential (cache-friendly) and random (cache-hostile) access patterns, this source maximizes the observable timing variation.

**Implementation:** Allocates an 8 MB buffer (spans L2 boundary). On even rounds, performs sequential reads (cache-friendly). On odd rounds, performs random reads (cache-hostile). The timing difference between rounds captures the cache state influenced by all concurrent system activity.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

### 20. `page_fault_timing`

**Category:** Timing
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

**Category:** Microarch
**Struct:** `SpeculativeExecutionSource`
**Platform:** All
**Estimated Rate:** ~2000 b/s

**Physics:** The CPU's branch predictor maintains per-address history tables that depend on ALL previously executed code across all processes on the core. Mispredictions cause pipeline flushes (~15 cycle penalty on Apple M4). By running data-dependent branches with unpredictable outcomes (LCG-generated), we capture the predictor's internal state.

**Implementation:** Executes batches of data-dependent branches using an LCG (seeded from the high-resolution clock). Batch sizes vary with the iteration index to create different branch predictor pressure levels. Three levels of branching per iteration maximize predictor state perturbation.

**Conditioning:** Timing deltas -> XOR whitening -> LSB extraction -> chained SHA-256.

---

## Composite Sources

These sources exploit the interference patterns that arise when independent clock domains interact. Each subsystem (CPU, memory controller, I/O bus) has its own PLL with independent phase noise. When operations cross domain boundaries, the beat frequency of their jitter creates entropy.

### 22. `cpu_io_beat`

**Category:** Composite
**Struct:** `CPUIOBeatSource`
**Platform:** All
**Estimated Rate:** ~300 b/s

**Physics:** The CPU and I/O subsystem run on independent clocks. Their interaction creates beat frequency patterns driven by two independent noise sources. CPU work and file I/O are interleaved, and the timing captures the cross-domain interference.

**Implementation:** Alternates between CPU-bound work and file I/O operations, measuring the total time for each interleaved operation via `mach_absolute_time()`.

---

### 23. `cpu_memory_beat`

**Category:** Composite
**Struct:** `CPUMemoryBeatSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** The CPU clock and memory controller operate at different frequencies with independent PLLs. The beat pattern captures phase noise from both oscillators. Memory-bound operations experience latency that depends on the memory controller's queue state and DRAM timing.

**Implementation:** Alternates between CPU computation and random memory accesses, measuring the timing of each round.

---

## Signal Sources

### 25. `compression_timing`

**Category:** Signal
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

### 28. `vm_page_timing`

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

## Frontier Sources

Experimental sources exploring unconventional entropy extraction mechanisms. These exploit OS kernel internals, cross-core interactions, and hardware coprocessor scheduling.

### 31. `amx_timing`

**Category:** Frontier
**Struct:** `AMXTimingSource`
**Platform:** macOS (Apple Silicon)
**Estimated Rate:** ~500 b/s

**Physics:** The Apple Matrix coprocessor (AMX) shares execution resources with the CPU. Matrix multiplication dispatch timing varies with AMX unit availability, thermal state, and concurrent workload pressure on shared execution ports.

**Implementation:** Dispatches small matrix operations and measures completion timing jitter.

---

### 32. `thread_lifecycle`

**Category:** Frontier
**Struct:** `ThreadLifecycleSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** Thread creation and destruction involves kernel scheduler decisions, stack allocation from the virtual memory subsystem, and TLS (Thread Local Storage) setup. The timing captures kernel allocator state and scheduler queue depth.

**Implementation:** Rapidly creates and joins pthreads, measuring the full lifecycle timing.

---

### 33. `mach_ipc`

**Category:** Frontier
**Struct:** `MachIPCSource`
**Platform:** macOS
**Estimated Rate:** ~300 b/s

**Physics:** Mach port allocation and deallocation involves the kernel IPC subsystem's port namespace management. Timing depends on port table fragmentation, IPC space lock contention, and kernel zone allocator state.

**Implementation:** Allocates and deallocates Mach ports in rapid succession, capturing per-operation timing jitter.

---

### 34. `tlb_shootdown`

**Category:** Frontier
**Struct:** `TLBShootdownSource`
**Platform:** All
**Estimated Rate:** ~400 b/s

**Physics:** Calling `mprotect()` on mapped memory triggers TLB (Translation Lookaside Buffer) invalidation via inter-processor interrupts (IPIs). The latency depends on which cores have cached the TLB entries, cross-core interrupt delivery time, and whether other cores are in low-power states.

**Implementation:** Maps memory, then repeatedly changes page protections via `mprotect()`, measuring the system call latency which includes IPI round-trip time.

---

### 35. `pipe_buffer`

**Category:** Frontier
**Struct:** `PipeBufferSource`
**Platform:** All
**Estimated Rate:** ~200 b/s

**Physics:** Creating and destroying pipes exercises the kernel's zone allocator for pipe buffer memory. Allocation timing depends on zone fragmentation, free list state, and memory pressure.

**Implementation:** Rapidly creates and closes pipe file descriptor pairs, measuring per-cycle timing.

---

### 36. `kqueue_events`

**Category:** Frontier
**Struct:** `KqueueEventsSource`
**Platform:** macOS, BSD
**Estimated Rate:** ~300 b/s

**Physics:** BSD kqueue event multiplexing combines timer events, file descriptor readiness, and process notifications. The timing jitter comes from kernel event queue management, timer coalescing, and I/O completion notification delivery.

**Implementation:** Registers mixed event types (timers, file descriptors, sockets) with kqueue and measures event delivery timing.

---

### 37. `dvfs_race`

**Category:** Frontier
**Struct:** `DVFSRaceSource`
**Platform:** All
**Estimated Rate:** ~500 b/s

**Physics:** Dynamic Voltage and Frequency Scaling (DVFS) adjusts CPU frequency based on load. By racing workloads across cores, the source captures the non-deterministic timing of frequency transitions and the interference between cores sharing a voltage domain.

**Implementation:** Spawns concurrent workloads on multiple cores and measures the timing differential as DVFS adjusts frequencies.

---

### 38. `cas_contention`

**Category:** Frontier
**Struct:** `CASContentionSource`
**Platform:** All
**Estimated Rate:** ~200 b/s

**Physics:** Compare-and-swap (CAS) operations on shared atomic variables experience hardware arbitration contention when multiple cores compete. The cache coherency protocol (MOESI/MESI) introduces non-deterministic delays as cache lines bounce between cores.

**Implementation:** Multiple threads perform atomic CAS operations on shared variables, and the contention-induced retry timing captures cache coherency protocol jitter.

---

### 39. `keychain_timing`

**Category:** Frontier
**Struct:** `KeychainTimingSource`
**Platform:** macOS
**Estimated Rate:** ~300 b/s

**Physics:** macOS Keychain Services API calls traverse the Security framework into `securityd`, involving XPC IPC, database access, and cryptographic operations. The timing jitter captures XPC message delivery, SQLite page cache state, and encryption overhead.

**Implementation:** Performs Keychain API queries (searching for non-existent items) and measures the round-trip timing through the Security framework.

### 40. `denormal_timing`

**Category:** Thermal
**Struct:** `DenormalTimingSource`
**Platform:** All (best on ARM)
**Estimated Rate:** ~3000 b/s

**Physics:** Forces the FPU to process denormalized floating-point numbers (values below the normal minimum, ~2⁻¹⁰²² for f64). Denormals require microcode-assisted handling with variable power draw, creating thermal-dependent timing variation. The timing captures transistor-level thermal noise in the FPU execution units — not scheduling or OS noise, but actual silicon-level thermal fluctuation.

**What makes it unique:** Only source that exploits **FPU micropower variation**. The entropy comes from the physical power consumed by the denormal microcode path varying with transistor temperature. Low Shannon H (~1.0) makes it ideal for anomaly detection research — subtle perturbations are easier to detect in a low-entropy signal.

---

### 41. `audio_pll_timing`

**Category:** Thermal
**Struct:** `AudioPLLTimingSource`
**Platform:** macOS (requires audio output device)
**Estimated Rate:** ~4000 b/s

**Physics:** The audio subsystem has its own Phase-Locked Loop generating sample clocks from an independent crystal oscillator. CoreAudio property queries (sample rate, latency) cross from the CPU clock domain into the audio PLL domain. Phase noise arises from VCO transistor Johnson-Nyquist noise, charge pump shot noise, and reference oscillator crystal jitter.

**What makes it unique:** Taps the **audio PLL crystal** — a physically separate oscillator from the CPU's 24 MHz crystal. The audio crystal runs at 48 kHz base frequency. Unlike `counter_beat` which measures the beat *between* CPU and audio clocks, this source measures the timing *within* the audio PLL domain crossing itself.

---

### 42. `usb_timing`

**Category:** IO
**Struct:** `USBTimingSource`
**Platform:** macOS
**Estimated Rate:** ~2000 b/s

**Physics:** USB bus transactions involve the XHCI host controller (which has its own clock domain), PHY signaling with bit-level timing recovery, and kernel driver overhead. IOKit lookups of USB device properties traverse the full USB stack, crossing clock domains between CPU, USB controller, and potentially downstream hubs.

**What makes it unique:** Exercises the **USB XHCI clock domain** — a separate clock recovery PLL from CPU, audio, display, and PCIe. The USB PHY has its own oscillator for high-speed signaling.

---

### 43. `counter_beat`

**Category:** Thermal
**Struct:** `CounterBeatSource`
**Platform:** macOS (Apple Silicon)
**Estimated Rate:** ~2000 b/s

**Physics:** Reads CNTVCT_EL0 (ARM generic timer, driven by the CPU's **24 MHz crystal**) immediately before and after a CoreAudio property query that forces synchronization with the **audio PLL crystal**. The query duration in counter ticks is modulated by the instantaneous phase relationship between these two independent oscillators. Entropy arises from uncorrelated Johnson-Nyquist thermal noise in each crystal's sustaining amplifier.

**What makes it unique:** The **primary two-oscillator beat** — directly measures the phase difference between two independent crystal oscillators. This is the closest consumer-hardware analog to oscillator-based hardware random number generators. Low min-entropy (H∞ ~1–3) is intentional: preserves the raw physical beat signal for research.

**Key distinction from `audio_pll_timing`:** `audio_pll_timing` measures timing jitter *within* CoreAudio calls. `counter_beat` measures the *beat frequency* between the CPU crystal and audio crystal — a fundamentally different signal that encodes the instantaneous phase relationship.

---

### 44. `gpu_divergence`

**Category:** GPU
**Struct:** `GPUDivergenceSource`
**Platform:** macOS (Metal)
**Estimated Rate:** ~500 b/s

**Physics:** Dispatches Metal compute shaders where threads within a SIMD group (warp) take different execution paths based on thread index. When threads diverge, the GPU must serialize execution across the divergent paths. The timing of this serialization depends on GPU scheduler state, thermal throttling, and power management — all nondeterministic.

**What makes it unique:** Only source exploiting **GPU SIMD lane divergence**. Unlike `gpu_timing` (measures dispatch scheduling overhead), this source forces intra-warp thread divergence and measures the timing cost. The GPU's internal scheduling decisions for divergent warps are not deterministic.

---

### 45. `iosurface_crossing`

**Category:** GPU
**Struct:** `IOSurfaceCrossingSource`
**Platform:** macOS
**Estimated Rate:** ~500 b/s

**Physics:** IOSurface is Apple's cross-domain shared memory primitive for GPU↔CPU coherence. Creating, mapping, and destroying IOSurfaces forces cache coherence protocol operations between the CPU and GPU memory controllers. The timing of these coherence operations depends on cache state, memory controller arbitration, and bus contention — all nondeterministic.

**What makes it unique:** Only source exploiting **GPU↔CPU memory coherence protocol timing**. Unlike `gpu_timing`/`gpu_divergence` (compute dispatch), this source measures the cost of crossing the CPU-GPU memory boundary itself.

---

### 46. `fsync_journal`

**Category:** IO
**Struct:** `FsyncJournalSource`
**Platform:** All
**Estimated Rate:** ~200 b/s

**Physics:** `fsync()` forces a full flush through the storage stack: filesystem journal commit (APFS on macOS), block layer queueing, NVMe command submission, and NAND flash write with wear-leveling. The timing traverses the entire I/O path from userspace to physical media.

**What makes it unique:** The **deepest I/O path** of any source — touches every layer from VFS to NAND flash. Slower than other IO sources but captures noise from the full storage stack, including APFS journal commit decisions and NVMe controller firmware timing.

---

### 47. `nvme_latency`

**Category:** IO
**Struct:** `NVMeLatencySource`
**Platform:** macOS
**Estimated Rate:** ~3000 b/s

**Physics:** NVMe controllers have their own firmware and clock domain. IOKit property reads traverse the NVMe driver stack, crossing from the CPU domain into the NVMe controller's firmware. Timing depends on NVMe queue depth, command scheduling, NAND cell access patterns, and wear-leveling state.

**What makes it unique:** Targets the **NVMe controller's independent clock domain** specifically, without going through the full filesystem stack (unlike `fsync_journal`). Faster and more direct.

---

### 48. `pdn_resonance`

**Category:** Thermal
**Struct:** `PDNResonanceSource`
**Platform:** All (ARM, best on Apple Silicon)
**Estimated Rate:** ~3000 b/s

**Physics:** The power delivery network (PDN) is an LC circuit (inductors + capacitors) that filters voltage to the CPU cores. When computation patterns change rapidly, the PDN's LC network rings at its resonant frequency. By alternating between heavy computation (AMX matrix ops or FPU-intensive work) and idle, the source excites PDN resonance and captures the resulting voltage fluctuation through timing variation.

**What makes it unique:** Only source exploiting **power delivery network analog resonance**. The PDN is a passive analog circuit — its ringing behavior depends on physical component values (inductor ESR, capacitor ESR, PCB trace impedance) that vary with temperature. Very low Shannon H (~0.86) makes it excellent for anomaly detection.

---

### 49. `display_pll`

**Category:** Thermal
**Struct:** `DisplayPllSource`
**Platform:** macOS (Apple Silicon)
**Estimated Rate:** ~2500 b/s

**Physics:** The display subsystem uses an independent PLL to generate the pixel clock (~533 MHz on Mac Mini M4). CoreGraphics display property queries (mode, refresh rate, dimensions) cross from the CPU clock domain into the display PLL domain. Reading CNTVCT_EL0 before and after captures the beat between the CPU crystal and display PLL.

**What makes it unique:** Taps the **third independent oscillator** on the SoC — separate from both the CPU crystal (24 MHz) and audio PLL (48 kHz). The display PLL runs at ~533 MHz, providing a different frequency ratio and noise characteristic. No audio hardware required.

**Key distinction from other oscillator sources:**
- `counter_beat` → CPU crystal vs **audio PLL** (48 kHz base)
- `display_pll` → CPU crystal vs **display PLL** (~533 MHz)
- `pcie_pll` → CPU crystal vs **PCIe PHY PLLs** (various)

---

### 50. `pcie_pll`

**Category:** Thermal
**Struct:** `PciePllSource`
**Platform:** macOS (Apple Silicon)
**Estimated Rate:** ~2000 b/s

**Physics:** Apple Silicon has multiple independent PLLs for the PCIe/Thunderbolt physical layer (CIO3PLL, AUSPLL, ACIOPHY_PLL visible in IORegistry). IOKit property reads from PCIe/Thunderbolt services cross into these PLL clock domains. The timing captures the beat between the CPU crystal and PCIe PHY oscillators. PCIe may also use spread-spectrum clocking (SSC), which intentionally modulates the clock frequency — adding an extra noise dimension.

**What makes it unique:** Taps the **fourth independent oscillator domain** — PCIe PHY PLLs are electrically separate from CPU, audio, and display oscillators. Cycles through 5 IOKit service classes (ThunderboltHAL, IOPCIDevice, IOThunderboltController, IONVMeController, USBHostController) to exercise different PCIe clock recovery PLLs.

**Key distinction:** While `nvme_latency` times NVMe-specific operations, `pcie_pll` targets the underlying PCIe PHY clock domain that *all* PCIe devices share — a lower-level, more fundamental oscillator source.

---

## Oscillator Independence Map

The thermal/oscillator sources each tap a **physically independent** noise source. This table clarifies what makes each one unique:

| Source | Oscillator Probed | Frequency | How Probed | Independent From |
|--------|-------------------|-----------|------------|------------------|
| `counter_beat` | Audio PLL crystal | ~48 kHz | CoreAudio property query | CPU, Display, PCIe |
| `audio_pll_timing` | Audio PLL crystal | ~48 kHz | CoreAudio property timing | CPU, Display, PCIe |
| `display_pll` | Display PLL | ~533 MHz | CoreGraphics mode query | CPU, Audio, PCIe |
| `pcie_pll` | PCIe PHY PLLs | Various | IOKit service property reads | CPU, Audio, Display |
| `denormal_timing` | *(none — FPU thermal)* | N/A | Denormal FPU computation | All oscillators |
| `pdn_resonance` | *(none — PDN analog)* | LC resonant | Computation burst/idle | All oscillators |

All oscillator sources use **CNTVCT_EL0** (CPU's 24 MHz crystal) as the timing reference. The entropy comes from the *other* oscillator's independent thermal noise — not from the CPU crystal itself.

---

## Platform Availability

| Platform | Available Sources | Notes |
|----------|:-----------------:|-------|
| **MacBook (M-series)** | **47/47** | Full suite — WiFi, BLE, camera, mic, all sensors and oscillators |
| **Mac Mini/Studio/Pro** | 44-45/47 | Most sources — no built-in camera or mic on some models |
| **Intel Mac** | ~20/47 | Timing, system, network, disk sources work; ARM-specific sources unavailable |
| **Linux** | ~12/47 | Timing, network, disk, process sources; no macOS/ARM-specific sources |

The package gracefully detects available hardware via `detect_available_sources()` and only activates sources that pass `is_available()`. MacBooks provide the richest entropy because they pack the most sensors into one device.

## Entropy Quality Notes

Individual source quality varies. Raw (unconditioned) source output often has bias and correlation:

- **Shannon entropy** of raw sources typically ranges from 2-8 bits/byte depending on the source
- **Raw NIST test pass rates** for individual sources range from 11/31 to 28/31
- **After pool conditioning** (SHA-256 + mixing + os.urandom), output consistently passes 28-31/31 NIST tests

The conditioning pipeline is designed to extract the genuine entropy from biased sources and produce cryptographic-quality output regardless of individual source weakness. This is consistent with the NIST SP 800-90B approach: measure the min-entropy of the raw source, then apply an approved conditioning function (SHA-256) to concentrate it.

## Adding a New Source

To add a new entropy source to the Rust codebase:

1. Create a struct implementing `EntropySource` in the appropriate file under `crates/openentropy-core/src/sources/`
2. Define a static `SourceInfo` with the physics explanation, category, and platform requirements
3. Register the source in `all_sources()` in `crates/openentropy-core/src/sources/mod.rs`
4. Add unit tests in the same file
5. Document the physics in this file

See [ADDING_SOURCES.md](ADDING_SOURCES.md) for the Python-side equivalent.
