# Entropy Source Catalog

63 sources across 13 mechanism-based categories, each exploiting a different physical phenomenon inside your computer. Every source implements the `EntropySource` trait and produces raw `Vec<u8>` samples that are fed into the entropy pool.

## Source Summary

| # | Source | Category | Mechanism | Est. Rate | Platform |
|---|--------|----------|-----------|-----------|----------|
| 1 | `clock_jitter` | Timing | PLL phase noise between clocks | 0.5 | All |
| 2 | `sleep_jitter` | Scheduling | OS scheduler wake-up jitter | 0.4 | All |
| 3 | `sysctl_deltas` | System | Kernel counter fluctuations | 3.0 | macOS, Linux |
| 4 | `vmstat_deltas` | System | VM subsystem page counters | 2.0 | macOS, Linux |
| 5 | `process_table` | System | Process table snapshot hash | 1.0 | macOS |
| 6 | `dns_timing` | Network | DNS resolution latency jitter | 3.0 | All |
| 7 | `tcp_connect_timing` | Network | TCP handshake timing variance | 2.0 | All |
| 8 | `wifi_rssi` | Network | WiFi signal strength noise floor | 0.5 | macOS |
| 9 | `disk_io` | IO | Block device I/O timing jitter | 3.0 | All |
| 10 | `audio_noise` | Sensor | Microphone ADC thermal noise | 6.0 | Requires mic |
| 11 | `camera_noise` | Sensor | Camera sensor read noise + dark current | 5.0 | Requires camera |
| 12 | `bluetooth_noise` | Sensor | BLE ambient RF environment | 1.0 | macOS |
| 13 | `dram_row_buffer` | Timing | DRAM row buffer hit/miss timing | 3.0 | All |
| 14 | `page_fault_timing` | Timing | mmap/munmap page fault latency | 2.0 | All |
| 15 | `speculative_execution` | Microarch | Branch predictor state timing | 2.5 | All |
| 16 | `ioregistry` | System | IOKit registry value mining | 2.0 | macOS |
| 17 | `compression_timing` | Signal | zlib compression timing oracle | 2.0 | All |
| 18 | `hash_timing` | Signal | SHA-256 timing data-dependency | 2.5 | All |
| 19 | `spotlight_timing` | Signal | Spotlight metadata query timing | 2.0 | macOS |
| 20 | `amx_timing` | Microarch | Apple AMX coprocessor matrix multiply jitter | 1.5 | macOS (ARM) |
| 21 | `thread_lifecycle` | Scheduling | Thread create/join scheduling jitter | 2.0 | All |
| 22 | `mach_ipc` | IPC | Mach port OOL message + VM remapping jitter | 2.0 | macOS |
| 23 | `tlb_shootdown` | Microarch | mprotect() TLB invalidation IPI latency | 2.0 | macOS |
| 24 | `pipe_buffer` | IPC | Kernel zone allocator via pipe lifecycle | 1.5 | macOS |
| 25 | `kqueue_events` | IPC | Kqueue event multiplexing jitter | 2.5 | macOS |
| 26 | `dvfs_race` | Microarch | Cross-core DVFS frequency race | 3.0 | macOS |
| 27 | `keychain_timing` | IPC | Keychain/securityd round-trip timing | 3.0 | macOS |
| 28 | `audio_pll_timing` | Thermal | Audio PLL clock drift from CoreAudio queries | 3.0 | macOS |
| 29 | `mach_continuous_timing` | Timing | mach_continuous_time() kernel sleep-offset path | 2.0 | macOS |
| 30 | `gpu_divergence` | GPU | GPU shader thread divergence timing | 4.0 | macOS (Metal) |
| 31 | `iosurface_crossing` | GPU | IOSurface CPU↔GPU memory domain crossing | 2.5 | macOS |
| 32 | `fsync_journal` | IO | APFS journal commit timing | 2.0 | All |
| 33 | `display_pll` | Thermal | Display PLL phase noise (~533 MHz pixel clock) | 4.0 | macOS (ARM) |
| 34 | `pcie_pll` | Thermal | PCIe PHY PLL jitter from IOKit clock domains | 4.0 | macOS (ARM) |
| 35 | `pe_core_arithmetic` | Scheduling | P-core/E-core migration arithmetic loop jitter | 6.0 | All |
| 36 | `memory_bus_crypto` | Microarch | AES-XTS crypto context switch cache flush timing | 2.0 | macOS (ARM) |
| 37 | `timer_coalescing` | Scheduling | OS timer coalescing wakeup jitter | 2.0 | All |
| 38 | `dispatch_queue_timing` | Scheduling | GCD libdispatch global queue timing | 3.0 | macOS |
| 39 | `nl_inference_timing` | GPU | NaturalLanguage ANE inference timing | 2.0 | macOS |
| 40 | `icc_atomic_contention` | Microarch | Apple Silicon ICC bus atomic contention | 2.5 | macOS (ARM) |
| 41 | `aprr_jit_timing` | Microarch | Apple APRR undocumented register JIT toggle | 1.5 | macOS (ARM) |
| 42 | `preemption_boundary` | Scheduling | Kernel preemption timing via CNTVCT_EL0 | 2.0 | macOS (ARM) |
| 43 | `sev_event_timing` | Microarch | ARM64 SEV/SEVL broadcast event timing | 3.0 | macOS (ARM) |
| 44 | `commpage_clock_timing` | Timing | COMMPAGE seqlock update synchronization | 1.5 | macOS |
| 45 | `smc_highvar_timing` | Sensor | SMC thermistor ADC + fuel gauge I2C bus | 2.5 | macOS |
| 46 | `proc_info_timing` | System | proc_pidinfo kernel proc_lock contention | 1.5 | macOS |
| 47 | `getentropy_timing` | System | getentropy() SEP TRNG reseed timing | 1.0 | macOS |
| 48 | `prefetcher_state` | Microarch | Hardware prefetcher stride-learning state | 2.0 | macOS (ARM) |
| 49 | `usb_enumeration` | IO | IOKit USB device enumeration timing | 1.5 | macOS |
| 50 | `gxf_register_timing` | Microarch | Apple GXF EL0-accessible register trap-path | 0.7 | macOS (ARM) |
| 51 | `cntfrq_cache_timing` | Microarch | CNTFRQ_EL0 system-register cache timing | 1.5 | macOS (ARM) |
| 52 | `commoncrypto_aes_timing` | Microarch | CommonCrypto AES-128-CBC bimodal timing | 2.0 | macOS |
| 53 | `dual_clock_domain` | Microarch | 24 MHz CNTVCT × 41 MHz private timer beat | 6.0 | macOS (ARM) |
| 54 | `sitva` | Microarch | Scheduler-induced timing variance via NEON FMLA | 2.0 | macOS (ARM) |
| 55 | `ane_timing` | Timing | Apple Neural Engine clock domain crossing jitter | 3.0 | macOS |
| 56 | `nvme_iokit_sensors` | IO | NVMe controller sensor polling via IOKit with CNTVCT clock domain crossing timestamps | 3.0 | macOS |
| 57 | `nvme_raw_device` | IO | Direct raw block device reads bypassing filesystem with page-aligned I/O | 2.0 | Any |
| 58 | `nvme_passthrough_linux` | IO | Raw NVMe admin commands via ioctl passthrough on Linux (closest to NAND hardware) | 2.0 | Linux |
| 59 | `mach_timing` | Timing | mach_absolute_time() nanosecond timing jitter | 0.3 | macOS |
| 60 | `qcicada` | Quantum | Crypta Labs QCicada USB QRNG — photonic shot noise | 8.0 | Any (USB) |
| 61 | `counter_beat` | Thermal | Two-oscillator beat frequency: CPU counter vs audio PLL | 3.0 | macOS |
| 62 | `cas_contention` | Microarch | Multi-thread atomic CAS arbitration contention | 2.0 | All |
| 63 | `denormal_timing` | Microarch | Floating-point denormal multiply-accumulate timing | 0.5 | All |

---

## Timing Sources (7)

### `clock_jitter`

**Category:** Timing | **Platform:** All | **Est. Rate:** 0.5

Measures phase noise between two independent clock oscillators (`Instant` vs `SystemTime`). Each clock is driven by a separate PLL on the SoC. Thermal noise in the PLL's VCO causes random frequency drift. The LSBs of their difference are genuine analog entropy.

### `dram_row_buffer`

**Category:** Timing | **Platform:** All | **Est. Rate:** 3.0

DRAM is organized into rows of capacitor cells. Accessing an open row (hit) is fast; accessing a different row requires precharge + activate (miss). Timing depends on physical address mapping, row buffer state from all system activity, memory controller scheduling, and DRAM refresh interference. Uses a 32 MB buffer exceeding L2/L3 cache.

### `page_fault_timing`

**Category:** Timing | **Platform:** All | **Est. Rate:** 2.0

Triggers minor page faults via mmap/munmap cycles. Resolution requires TLB lookup, 4-level page table walk (ARM64), physical page allocation, and zero-fill. Timing depends on memory fragmentation and kernel allocator state.

### `mach_continuous_timing`

**Category:** Timing | **Platform:** macOS | **Est. Rate:** 2.0

mach_continuous_time() kernel sleep-offset path — CV=475% vs mach_absolute_time at 106%. The continuous clock includes sleep time, requiring kernel synchronization that introduces high-variance jitter.

### `commpage_clock_timing`

**Category:** Timing | **Platform:** macOS | **Est. Rate:** 1.5

Reads the macOS COMMPAGE seqlock update synchronization timing. Bimodal clock read latency from seqlock contention when the kernel updates the shared COMMPAGE clock values.

### `ane_timing`

**Category:** Timing | **Platform:** macOS | **Est. Rate:** 3.0

Apple Neural Engine clock domain crossing jitter via IOKit property reads. The ANE has its own independent clock domain, separate from the CPU, GPU, audio PLL, display PLL, and PCIe PHY. IOKit property reads from ANE services force clock domain crossings between the CPU's 24 MHz crystal and the ANE's independent PLL. Entropy arises from ANE PLL thermal noise, power state transition latency, DMA setup variance, and memory fabric contention.

### `mach_timing`

**Category:** Timing | **Platform:** macOS | **Est. Rate:** 0.3

Reads the ARM system counter (mach_absolute_time) at sub-nanosecond resolution with variable micro-workloads between samples. Timing jitter comes from CPU pipeline state: instruction reordering, branch prediction, cache state, interrupt coalescing, and power-state transitions.

---

## Scheduling Sources (6)

### `sleep_jitter`

**Category:** Scheduling | **Platform:** All | **Est. Rate:** 0.4

Zero-duration nanosleep() wake-up jitter from OS scheduler non-determinism: timer interrupt granularity, thread priority decisions, runqueue length, and DVFS.

### `thread_lifecycle`

**Category:** Scheduling | **Platform:** All | **Est. Rate:** 2.0

pthread create/join cycle timing. Involves kernel scheduler decisions, stack allocation, and TLS setup.

### `pe_core_arithmetic`

**Category:** Scheduling | **Platform:** All | **Est. Rate:** 6.0

P-core/E-core migration timing entropy from arithmetic loop jitter. Captures the non-deterministic timing of macOS migrating work between performance and efficiency cores. One of the highest entropy rate sources at 6.35 bits/byte.

### `dispatch_queue_timing`

**Category:** Scheduling | **Platform:** macOS | **Est. Rate:** 3.0

GCD libdispatch global queue timing — uses real Grand Central Dispatch via libdispatch FFI to measure system-wide thread pool scheduling entropy.

### `timer_coalescing`

**Category:** Scheduling | **Platform:** All | **Est. Rate:** 2.0

OS timer coalescing wakeup jitter from system-wide timer queue state. macOS coalesces timer events across all processes, creating non-deterministic wakeup timing.

### `preemption_boundary`

**Category:** Scheduling | **Platform:** macOS (ARM) | **Est. Rate:** 2.0

Kernel scheduler preemption timing via consecutive CNTVCT_EL0 reads. Detects preemption events by identifying large gaps in the timer counter.

---

## System Sources (6)

### `sysctl_deltas`

**Category:** System | **Platform:** macOS, Linux | **Est. Rate:** 3.0

50+ kernel counters via sysctl that fluctuate due to interrupt handling, context switches, network packets, and I/O completions.

### `vmstat_deltas`

**Category:** System | **Platform:** macOS, Linux | **Est. Rate:** 2.0

Virtual memory subsystem counters — page faults, pageins, swapins — driven by unpredictable memory access patterns from all running processes.

### `process_table`

**Category:** System | **Platform:** macOS | **Est. Rate:** 1.0

Process table snapshot — PIDs, memory usage, CPU times, thread counts. Changes unpredictably with system activity.

### `ioregistry`

**Category:** System | **Platform:** macOS | **Est. Rate:** 2.0

IOKit registry tree — thousands of properties including real-time sensor readings, power states, link status counters, and hardware event timestamps.

### `proc_info_timing`

**Category:** System | **Platform:** macOS | **Est. Rate:** 1.5

proc_pidinfo / proc_pid_rusage syscall — kernel proc_lock contention timing. Lock contention varies with system-wide process activity.

### `getentropy_timing`

**Category:** System | **Platform:** macOS | **Est. Rate:** 1.0

getentropy() SEP TRNG reseed timing — CV=267% bimodal distribution. Captures the timing of the Secure Enclave's hardware TRNG reseed events.

---

## Network Sources (3)

### `dns_timing`

**Category:** Network | **Platform:** All | **Est. Rate:** 3.0

DNS resolution latency includes network propagation, server load, routing jitter, and cache state.

### `tcp_connect_timing`

**Category:** Network | **Platform:** All | **Est. Rate:** 2.0

TCP three-way handshake timing varies with congestion, server load, routing, and kernel networking stack state.

### `wifi_rssi`

**Category:** Network | **Platform:** macOS | **Est. Rate:** 0.5

WiFi RSSI includes multipath fading, co-channel interference, thermal noise floor, and environmental factors.

---

## IO Sources (6)

### `disk_io`

**Category:** IO | **Platform:** All | **Est. Rate:** 3.0

Block device I/O latency varies with NAND channel contention, write-back cache state, wear-leveling decisions, and thermal effects.

### `fsync_journal`

**Category:** IO | **Platform:** All | **Est. Rate:** 2.0

fsync() forces a full flush through the storage stack: APFS journal commit, block layer, NVMe submission, and NAND write with wear-leveling. The deepest I/O path of any source.

### `usb_enumeration`

**Category:** IO | **Platform:** macOS | **Est. Rate:** 1.5

IOKit USB device enumeration timing — CV=116%. Traverses the USB stack crossing clock domains between CPU, XHCI controller, and downstream hubs.

### `nvme_iokit_sensors`

**Category:** IO | **Platform:** macOS | **Est. Rate:** 3.0

NVMe controller sensor polling via IOKit with CNTVCT clock domain crossing timestamps. Reads NVMe controller properties (temperature, SMART counters) via the IOKit C API, forcing clock domain crossings between the CPU's 24 MHz crystal and the NVMe controller's independent PLL. Combines clock domain crossing timing with actual hardware sensor data (temperature ADC noise, SMART counter deltas).

### `nvme_raw_device`

**Category:** IO | **Platform:** Any | **Est. Rate:** 2.0

Direct raw block device reads bypassing filesystem with page-aligned I/O. Reads directly from raw block devices (`/dev/rdiskN` on macOS, `/dev/nvmeXnYpZ` on Linux) with page-aligned buffers and cache bypass (`F_NOCACHE` / `O_DIRECT`). This eliminates the filesystem, buffer cache, and VFS layers from the timing path, so the remaining timing variance comes from NVMe controller firmware and NAND flash page read latency.

### `nvme_passthrough_linux`

**Category:** IO | **Platform:** Linux | **Est. Rate:** 2.0

Raw NVMe admin commands via ioctl passthrough on Linux (closest to NAND hardware). Submits raw NVMe admin commands (Get Log Page for SMART/Health Information) via `ioctl(NVME_IOCTL_ADMIN_CMD)` on `/dev/nvme0`. This bypasses the filesystem, block layer, and I/O scheduler entirely -- the closest to NVMe hardware achievable from userspace on Linux.

---

## IPC Sources (4)

### `mach_ipc`

**Category:** IPC | **Platform:** macOS | **Est. Rate:** 2.0

Mach port complex OOL message and VM remapping timing jitter. Involves kernel IPC subsystem port namespace management.

### `pipe_buffer`

**Category:** IPC | **Platform:** macOS | **Est. Rate:** 1.5

Multi-pipe kernel zone allocator competition. Rapidly creates and closes pipe file descriptor pairs, measuring per-cycle timing.

### `kqueue_events`

**Category:** IPC | **Platform:** macOS | **Est. Rate:** 2.5

BSD kqueue event multiplexing combines timer events, file descriptor readiness, and process notifications.

### `keychain_timing`

**Category:** IPC | **Platform:** macOS | **Est. Rate:** 3.0

Keychain Services API calls traverse the Security framework into securityd, involving XPC IPC, database access, and cryptographic operations.

---

## Microarchitecture Sources (16)

### `speculative_execution`

**Category:** Microarch | **Platform:** All | **Est. Rate:** 2.5

Branch predictor maintains per-address history depending on all previously executed code. Mispredictions cause ~15 cycle pipeline flushes on M4. Data-dependent branches capture predictor internal state.

### `dvfs_race`

**Category:** Microarch | **Platform:** macOS | **Est. Rate:** 3.0

Dynamic Voltage and Frequency Scaling frequency transitions. Races workloads across cores sharing a voltage domain.

### `tlb_shootdown`

**Category:** Microarch | **Platform:** macOS | **Est. Rate:** 2.0

mprotect() triggers TLB invalidation via inter-processor interrupts. Latency depends on which cores have cached entries and cross-core interrupt delivery time.

### `amx_timing`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 1.5

Apple AMX coprocessor matrix multiply timing jitter with Von Neumann debiasing. AMX shares execution resources with the CPU.

### `icc_atomic_contention`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 2.5

Apple Silicon ICC bus arbitration timing via cross-core atomic contention. LDXR/STXR exclusive monitors on the interconnect fabric.

### `prefetcher_state`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 2.0

Hardware prefetcher stride-learning state — 2.25x learned vs random speedup. Precise L2 prime+probe of the prefetcher's internal stride detector.

### `aprr_jit_timing`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 1.5

Apple APRR undocumented register JIT toggle — CV=100%, trimodal 0/42/83. Exercises the JIT memory permission toggling mechanism.

### `sev_event_timing`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 3.0

ARM64 SEV/SEVL broadcast event timing via ICC fabric load. Measures the latency of inter-core event signaling.

### `cntfrq_cache_timing`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 1.5

CNTFRQ_EL0 JIT-read trimodal system-register cache timing. The system register read path hits different caching levels.

### `gxf_register_timing`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 0.7

Apple GXF EL0-accessible register trap-path timing entropy. Exercises the Guarded Execution Feature register access path.

### `dual_clock_domain`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 6.0

24 MHz CNTVCT x 41 MHz Apple private timer beat-frequency entropy. Two independent clock domains create interference patterns with high entropy content.

### `sitva`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 2.0

Scheduler-induced timing variance amplification via NEON FMLA companion thread. CV=189% x AES sample rate. Amplifies scheduler jitter through compute-intensive companion threads.

### `memory_bus_crypto`

**Category:** Microarch | **Platform:** macOS (ARM) | **Est. Rate:** 2.0

AES-XTS crypto context switching timing from cross-page cache flush cycles. Exercises hardware crypto acceleration with cache line eviction patterns.

### `commoncrypto_aes_timing`

**Category:** Microarch | **Platform:** macOS | **Est. Rate:** 2.0

CommonCrypto AES-128-CBC warm/cold key schedule bimodal timing. CCCrypt calls show bimodal distribution: ~50 ticks (warm, key schedule cached) vs ~120 ticks (cold, key reload via system fabric). CV=155.4%. Cross-process sensitivity: FileVault/HTTPS bursts visibly shift distribution toward cold path.

### `cas_contention`

**Category:** Microarch | **Platform:** All | **Est. Rate:** 2.0

Multi-thread atomic CAS arbitration contention jitter. Spawns multiple threads performing atomic compare-and-swap operations on shared targets spread across cache lines. The hardware coherence engine must arbitrate concurrent exclusive-access requests, producing physically nondeterministic timing from interconnect fabric latency variations, thermal state, and traffic from other cores.

### `denormal_timing`

**Category:** Microarch | **Platform:** All | **Est. Rate:** 0.5

Floating-point denormal multiply-accumulate timing jitter. Times blocks of floating-point operations on denormalized values (magnitudes between 0 and f64::MIN_POSITIVE). Even on Apple Silicon where denormal handling is fast in hardware, residual timing jitter comes from FPU pipeline state, cache line alignment, and memory controller arbitration.

---

## GPU Sources (3)

### `gpu_divergence`

**Category:** GPU | **Platform:** macOS (Metal) | **Est. Rate:** 4.0

Metal compute shaders where threads within a SIMD group take different execution paths. GPU must serialize divergent paths; timing depends on scheduler state and thermal throttling.

### `iosurface_crossing`

**Category:** GPU | **Platform:** macOS | **Est. Rate:** 2.5

IOSurface CPU-GPU memory domain crossing coherence jitter. Forces cache coherence protocol operations between CPU and GPU memory controllers.

### `nl_inference_timing`

**Category:** GPU | **Platform:** macOS | **Est. Rate:** 2.0

NaturalLanguage ANE inference timing via system-wide NLP cache state. Exercises the Apple Neural Engine through the NaturalLanguage framework.

---

## Thermal Sources (4)

### `audio_pll_timing`

**Category:** Thermal | **Platform:** macOS | **Est. Rate:** 3.0

Audio PLL clock jitter from CoreAudio device property queries. The audio subsystem's independent PLL generates sample clocks from a separate crystal. Phase noise from VCO Johnson-Nyquist noise and charge pump shot noise.

### `counter_beat`

**Category:** Thermal | **Platform:** macOS | **Est. Rate:** 3.0

Two-oscillator beat frequency: CPU counter (CNTVCT_EL0) vs audio PLL crystal. Reads the ARM generic timer counter immediately before and after a CoreAudio property query that forces synchronization with the audio PLL clock domain. The query duration in raw counter ticks is modulated by the instantaneous phase relationship between the CPU crystal and the independent audio PLL crystal.

### `display_pll`

**Category:** Thermal | **Platform:** macOS (ARM) | **Est. Rate:** 4.0

Display PLL phase noise from pixel clock (~533 MHz) domain crossing. The third independent oscillator on the SoC — separate from both CPU crystal and audio PLL.

### `pcie_pll`

**Category:** Thermal | **Platform:** macOS (ARM) | **Est. Rate:** 4.0

PCIe PHY PLL jitter from IOKit property reads across PCIe/Thunderbolt clock domains. The fourth independent oscillator domain. May include spread-spectrum clocking modulation.

---

## Signal Sources (3)

### `compression_timing`

**Category:** Signal | **Platform:** All | **Est. Rate:** 2.0

zlib compression time is data-dependent. Different byte patterns trigger different Lempel-Ziv code paths.

### `hash_timing`

**Category:** Signal | **Platform:** All | **Est. Rate:** 2.5

SHA-256 hashing time varies subtly with input data due to cache state and microarchitectural effects.

### `spotlight_timing`

**Category:** Signal | **Platform:** macOS | **Est. Rate:** 2.0

Spotlight metadata query timing via mdls. Depends on index size, disk cache residency, and concurrent indexing activity.

---

## Sensor Sources (4)

### `audio_noise`

**Category:** Sensor | **Platform:** Requires microphone | **Est. Rate:** 6.0

Microphone ADC Johnson-Nyquist noise — thermal agitation of electrons in the input impedance. At audio frequencies, entirely classical. V_noise = sqrt(4kTR x bandwidth).

### `camera_noise`

**Category:** Sensor | **Platform:** Requires camera | **Est. Rate:** 5.0

Camera sensor noise in darkness: read noise (~95%), dark current, and shot noise. The LSBs of pixel values mix all three components.

### `bluetooth_noise`

**Category:** Sensor | **Platform:** macOS | **Est. Rate:** 1.0

BLE ambient RF environment scanning. RSSI fluctuates with multipath fading, movement, and interference across 37 advertising channels.

### `smc_highvar_timing`

**Category:** Sensor | **Platform:** macOS | **Est. Rate:** 2.5

SMC thermistor ADC + fuel gauge I2C bus — CV=64-66%, 8x outliers. Targets two SMC keys (TC0P: CPU proximity NTC thermistor via analog ADC, B0RM: battery fuel gauge IC over I2C) that show 8x higher variance than all other SMC keys. On MacBook reads live Li-ion electrochemical noise; on Mac mini captures I2C bus timeout randomness.

---

## Quantum Sources (1)

### `qcicada`

**Category:** Quantum | **Platform:** Any (USB) | **Est. Rate:** 8.0

Crypta Labs QCicada USB QRNG — photonic shot noise from an LED/photodiode pair. Photon emission and detection are inherently quantum processes governed by Poisson statistics. The device digitises photodiode current fluctuations to produce true quantum random numbers at full entropy (8 bits/byte).

**Requires:** QCicada USB hardware.

**CLI mode flag:**
```bash
openentropy bench qcicada --qcicada-mode sha256     # NIST conditioned
openentropy stream qcicada --qcicada-mode raw        # Raw noise (default)
openentropy stream qcicada --qcicada-mode samples    # Direct QOM samples
```

**TUI:** Press `m` while qcicada is selected to cycle modes live (raw → sha256 → samples).

**Environment variables:**
| Variable | Default | Description |
|----------|---------|-------------|
| `QCICADA_MODE` | `raw` | Post-processing mode: `raw`, `sha256`, or `samples` |
| `QCICADA_POST_PROCESS` | `raw` | Legacy alias for `QCICADA_MODE` |
| `QCICADA_PORT` | auto-detect | Serial port path (e.g. `/dev/tty.usbmodem*`) |
| `QCICADA_TIMEOUT` | `5000` | Connection timeout in ms |

---

## Platform Availability

| Platform | Available Sources | Notes |
|----------|:-----------------:|-------|
| **MacBook (M-series)** | **63/63** | Full suite — WiFi, BLE, camera, mic, all sensors and oscillators |
| **Mac Mini/Studio/Pro** | 50-55/63 | Most sources — no built-in camera or mic on some models |
| **Intel Mac** | ~18/63 | Timing, system, network, disk sources; ARM-specific sources unavailable |
| **Linux** | ~14/63 | Timing, network, disk, process sources; no macOS/ARM-specific sources |

## Entropy Quality Notes

Individual source quality varies. Raw (unconditioned) source output often has bias and correlation:

- **Shannon entropy** of raw sources ranges from 0.7-8 bits/byte depending on the source
- **Raw NIST test pass rates** for individual sources range from 11/31 to 28/31
- **After pool conditioning** (SHA-256 + mixing + os.urandom), output passes 28-31/31 NIST tests

The conditioning pipeline extracts genuine entropy from biased sources and produces cryptographic-quality output. The general approach follows NIST SP 800-90B guidance: measure min-entropy of the raw source, then apply approved conditioning (SHA-256). Note: OpenEntropy has not been formally evaluated against SP 800-90B by an accredited lab.

## Adding a New Source

1. Create a struct implementing `EntropySource` in `crates/openentropy-core/src/sources/<category>/`
2. Define a static `SourceInfo` with physics explanation, category, platform requirements, and `is_fast` flag
3. Register in `sources()` in the category's `mod.rs` (e.g., `sources/timing/mod.rs`)
4. Add unit tests in the same file
5. Document the physics in this file

Python bindings automatically expose all Rust sources via PyO3.
