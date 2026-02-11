# Entropy Sources

Detailed descriptions of every source, the physics behind it, and expected entropy rates.

## Timing Sources

### Clock Jitter (`clock_jitter`)
**Physics:** Modern CPUs have multiple independent clock domains driven by separate PLLs (Phase-Locked Loops). The phase noise of each PLL is physically random — dominated by thermal noise in the VCO. By comparing `perf_counter` and `monotonic` clocks, we capture this phase drift.

### Mach Timing (`mach_timing`)
**Physics:** `mach_absolute_time()` on macOS reads the ARM system counter at sub-nanosecond resolution. The LSBs of successive deltas are influenced by interrupt coalescing, power-state transitions, memory controller refresh, and speculative execution pipeline state.

### Sleep Jitter (`sleep_jitter`)
**Physics:** Requesting a zero-duration sleep and measuring actual elapsed time captures OS scheduling jitter, timer interrupt granularity, and thermal-dependent clock drift.

## Kernel Sources

### Sysctl Counters (`sysctl_counters`)
**Physics:** macOS exposes 1600+ kernel counters via sysctl. ~58 change within 0.2s — TCP segment counts, VM page faults, context switches, network packet counts, etc. The deltas between reads are determined by the unpredictable behaviour of every process, network packet, and interrupt on the system.

### VM Statistics (`vmstat`)
**Physics:** `vm_stat` reports page faults, pageins/outs, swap activity, and memory pressure. These counters change with every memory access pattern across all running processes.

## Network Sources

### DNS Timing (`dns_timing`)
**Physics:** Each UDP DNS query traverses physical network links whose latency fluctuates due to queuing delays, routing decisions, congestion, and electromagnetic interference on the wire/air.

### TCP Connect (`tcp_connect`)
**Physics:** TCP three-way handshake adds server processing time and SYN/ACK round-trip through potentially different network paths.

## Storage Sources

### Disk I/O (`disk_io`)
**Physics:** NVMe/SSD read latency varies due to NAND cell voltage margins, wear leveling decisions, garbage collection interrupts, controller queue state, and thermal effects on NAND read thresholds.

## Memory Sources

### Memory Timing (`memory_timing`)
**Physics:** Memory allocation timing varies due to DRAM refresh cycles (~64ms intervals), cache misses (L1→L2→L3→DRAM), TLB misses, memory controller scheduling, and row buffer hits/misses.

## Compute Sources

### GPU Timing (`gpu_timing`)
**Physics:** GPU shader execution is non-deterministic due to thermal throttling micro-decisions, memory controller arbitration, and warp/SIMD group scheduling.

## Hardware Sensor Sources (Optional)

### Audio Thermal Noise (`audio_thermal`)
**Physics:** Johnson-Nyquist noise — thermal agitation of electrons in the microphone/ADC input impedance. This is a well-characterised quantum-origin noise source (Physical Review, 1928).

### Camera Shot Noise (`camera_shot_noise`)
**Physics:** Photon arrival at each pixel follows a Poisson process. In low-light/dark conditions, the LSBs are dominated by quantum shot noise and thermal dark current.

### Bluetooth BLE (`bluetooth_ble`)
**Physics:** BLE advertisement RSSI fluctuates due to multipath fading, device movement, and frequency-hop timing across 40 channels.
