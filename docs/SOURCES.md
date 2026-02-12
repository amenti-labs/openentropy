# Entropy Source Catalog

30 sources across 7 categories, each exploiting a different physical phenomenon.

## ⏱ Timing Sources

### `clock_jitter`
**Physics:** Phase noise between independent oscillators driving `perf_counter` and `monotonic` clocks. PLL jitter in the LSBs is genuine thermal noise.
**Rate:** ~500 b/s | **Platform:** All

### `mach_timing`
**Physics:** Apple Silicon Mach absolute time counter LSBs. The counter runs at a frequency with inherent crystal oscillator phase noise.
**Rate:** ~1000 b/s | **Platform:** macOS

### `sleep_jitter`
**Physics:** OS scheduler non-determinism. `nanosleep()` wake-up times vary due to interrupt load, cache state, and thermal throttling.
**Rate:** ~200 b/s | **Platform:** All

## 🖥 System Sources

### `sysctl`
**Physics:** Kernel counters (50+ keys) that fluctuate due to interrupt handling, context switches, network packets, and I/O completions.
**Rate:** ~2000 b/s | **Platform:** macOS, Linux

### `vmstat`
**Physics:** Virtual memory subsystem counters — page faults, pageins, swapins — driven by unpredictable memory access patterns.
**Rate:** ~500 b/s | **Platform:** macOS, Linux

### `process`
**Physics:** Process table snapshot — PIDs, memory usage, CPU times. Changes unpredictably with system activity.
**Rate:** ~300 b/s | **Platform:** All

## 🌐 Network Sources

### `dns_timing`
**Physics:** DNS resolution latency includes network propagation, server load, and routing jitter.
**Rate:** ~400 b/s | **Platform:** All

### `tcp_connect`
**Physics:** TCP three-way handshake timing varies with network congestion, server load, and routing.
**Rate:** ~300 b/s | **Platform:** All

### `wifi_rssi`
**Physics:** WiFi received signal strength includes multipath fading, interference, and thermal noise floor.
**Rate:** ~200 b/s | **Platform:** macOS (CoreWLAN)

## 🔧 Hardware Sources

### `disk_io`
**Physics:** Block device I/O latency varies with disk arm position, write-back cache, and wear-leveling (SSD).
**Rate:** ~500 b/s | **Platform:** All

### `memory_timing`
**Physics:** DRAM access timing varies with row buffer state, refresh timing, and thermal effects.
**Rate:** ~800 b/s | **Platform:** All

### `gpu_timing`
**Physics:** GPU compute kernel dispatch timing varies with shader occupancy, memory bandwidth, and thermal throttling.
**Rate:** ~600 b/s | **Platform:** macOS (Metal)

### `audio_noise`
**Physics:** Microphone preamp thermal noise (Johnson-Nyquist noise). Even with no audio input, the ADC captures genuine thermal fluctuations.
**Rate:** ~1000 b/s | **Platform:** Requires microphone + sounddevice

### `camera_noise`
**Physics:** Image sensor dark current — thermally generated electron-hole pairs in silicon photodiodes. Fundamentally quantum.
**Rate:** ~2000 b/s | **Platform:** Requires camera + opencv

### `sensor_noise`
**Physics:** Apple SMC sensor readout jitter from ADC quantization noise and thermal fluctuations.
**Rate:** ~400 b/s | **Platform:** macOS

### `bluetooth_noise`
**Physics:** BLE ambient RF environment scanning. Signal strengths reflect multipath interference and thermal noise.
**Rate:** ~200 b/s | **Platform:** macOS (CoreBluetooth)

### `ioregistry`
**Physics:** IOKit registry deep mining — hardware counters, power states, and sensor readings from the IOService tree.
**Rate:** ~500 b/s | **Platform:** macOS

## 🧬 Silicon Microarchitecture

### `dram_row_buffer`
**Physics:** DRAM row buffer conflicts. Accessing different rows in the same bank forces a precharge cycle whose latency depends on the row's charge state.
**Rate:** ~600 b/s | **Platform:** All

### `cache_contention`
**Physics:** CPU cache line eviction timing. L1/L2 conflicts create measurable timing variations based on the cache replacement policy's internal state.
**Rate:** ~800 b/s | **Platform:** All

### `page_fault_timing`
**Physics:** Virtual memory page fault handling latency depends on TLB state, page table walk depth, and physical memory pressure.
**Rate:** ~400 b/s | **Platform:** All

### `speculative_exec`
**Physics:** Branch prediction and speculative execution create timing side-channels. The predictor's internal state depends on all prior branches — deeply unpredictable.
**Rate:** ~500 b/s | **Platform:** All

## 🔀 Cross-Domain Beat Frequencies

### `cpu_io_beat`
**Physics:** The CPU and I/O subsystem run on independent clocks. Their interaction creates beat frequency patterns driven by two independent noise sources.
**Rate:** ~300 b/s | **Platform:** All

### `cpu_memory_beat`
**Physics:** CPU clock and memory controller operate at different frequencies. The beat pattern captures independent PLL phase noise.
**Rate:** ~400 b/s | **Platform:** All

### `multi_domain_beat`
**Physics:** Interference pattern from 3+ independent subsystems (CPU, memory, I/O). Multi-source beats have higher entropy density than pairwise.
**Rate:** ~500 b/s | **Platform:** All

## 🆕 Novel Sources

### `compression_timing`
**Physics:** zlib compression time is data-dependent. Different byte patterns trigger different code paths, creating measurable timing variation.
**Rate:** ~300 b/s | **Platform:** All

### `hash_timing`
**Physics:** SHA-256 hashing time varies subtly with input data due to memory access patterns and microarchitectural state.
**Rate:** ~400 b/s | **Platform:** All

### `dispatch_queue`
**Physics:** macOS Grand Central Dispatch queue scheduling jitter. Work items experience non-deterministic queueing delays.
**Rate:** ~500 b/s | **Platform:** macOS

### `dyld_timing`
**Physics:** Dynamic linker `dlsym()` lookup timing varies with symbol table size, cache state, and hash collisions.
**Rate:** ~300 b/s | **Platform:** macOS, Linux

### `vm_page_timing`
**Physics:** Mach VM page allocation latency depends on the kernel's free page list state and memory pressure.
**Rate:** ~400 b/s | **Platform:** macOS

### `spotlight_timing`
**Physics:** Spotlight metadata query timing reflects index size, disk cache state, and concurrent indexing activity.
**Rate:** ~200 b/s | **Platform:** macOS
