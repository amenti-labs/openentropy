---
title: 'Timing Sources'
description: 'Clock-domain and latency-jitter based entropy sources'
---

Timing sources extract entropy from clock drift, scheduling jitter, and memory
latency variability.

## Sources

- `clock_jitter` — PLL phase-noise differences across clocks
- `dram_row_buffer` — row-hit/miss latency variability
- `page_fault_timing` — page-fault handling latency variation
- `mach_continuous_timing` — macOS continuous-time path jitter
- `commpage_clock_timing` — COMMPAGE seqlock update timing
- `ane_timing` — ANE clock-domain crossing timing
- `mach_timing` — high-resolution timer jitter

## Notes

These sources are strong for fine-grained micro-timing behavior but should be
evaluated with forensic + chaos metrics before operational use.
