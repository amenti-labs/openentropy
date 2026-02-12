# Level 3 — Into the Silicon (2026-02-11)

## IOKit: 83 Hardware Properties That Fluctuate

Out of **8,186 total numeric properties** in the IORegistry, **83 change between samples**. These are live hardware counters from every controller on the SoC:

### GPU (AGX) Counters
- `AGXAllocation`, `AGXResource`, `AGXSecureMemoryMap` — GPU memory management
- `AGXUMABlock`, `AGXUMAAsyncGrowRequest` — Unified Memory Architecture activity
- `Device Utilization %` — 97% → 0% → 2% in 0.5s

### Neural Engine (ANE) Counters  
- `ANEDataBuffer` — Neural Engine buffer allocations fluctuate

### Storage Controller
- `Bytes (Read)`, `Bytes (Write)` — NVMe I/O byte counters
- `Container allocation` — APFS allocation counter
- `AuthAPFS: Number of bytes validated` — filesystem integrity checks

### Memory Controller
- `Alloc system memory` — 4.69GB → 4.67GB → 4.63GB in 1 second
- `IOMalloc allocation` — kernel memory allocator: 119M → 119.8M → 119.8M
- `IOMemoryMap`, `IOBufferMemoryDescriptor` — memory mapping counters
- `IODARTVMSpace` — DART (Device Address Resolution Table) IOMMU activity

### System Counters
- `IOMachPort` — Mach port allocation (microkernel IPC channels)
- `IOInterruptEventSource` — Hardware interrupt source count
- `IOCommandGate` — Kernel synchronization primitives
- `GeneratedSyncCounter` — Display sync counter (vsync)
- `HIDIdleTime` — HID subsystem idle timer

## New Entropy Sources (LSB Shannon Entropy)

| Source | Entropy | Unique | Physics |
|--------|---------|--------|---------|
| **DRAM row buffer conflicts** | **5.95/8.0** | 80 | Memory controller row activation/precharge timing |
| **Socket creation (NIC)** | **5.02/8.0** | 60 | Network stack + NIC controller initialization |
| **Speculative execution** | **4.97/8.0** | 50 | Branch predictor + pipeline flush timing |
| **Unified Memory bus** | **4.21/8.0** | 36 | CPU↔GPU shared memory bus arbitration |
| **stat() VFS syscall** | **3.06/8.0** | 16 | VFS + inode cache + APFS B-tree traversal |

## The DRAM Row Buffer Discovery

**5.95 bits/byte — the highest raw entropy of ANY source we've found.**

DRAM is organized into rows and columns. Accessing data in the same row (row hit) is fast. Accessing a different row (row conflict) requires closing the current row and opening a new one — this takes longer. The exact timing depends on:

1. **Physical address mapping** — which DRAM rank/bank/row a virtual address maps to
2. **Row buffer state** — whether the target row is already open (depends on ALL recent memory accesses system-wide)
3. **Memory controller scheduling** — arbitration between CPU cores, GPU, ANE, DMA engines
4. **Refresh interference** — DRAM must be periodically refreshed, which steals bus cycles non-deterministically
5. **Temperature-dependent timing margins** — DRAM timing parameters shift with temperature

This is measuring the **fundamental physics of dynamic RAM** — charge storage in capacitors, sense amplifier thresholds, and bus arbitration logic.
