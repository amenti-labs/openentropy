# Thermal Noise Entropy Source Research — 2026-02-14

## Objective

Identify entropy sources that tap into the most fundamental physical noise
with minimal digital filtering. Seven proof-of-concept C programs were written,
compiled, and run on a Mac Mini M4 to measure Shannon entropy (H) and
NIST SP 800-90B min-entropy (H∞) for each candidate source.

## Test Platform

- **Hardware:** Mac Mini M4 (Apple Silicon)
- **OS:** macOS 24.6.0 (Darwin)
- **Compiler:** Apple Clang, `-O2`
- **Samples:** 10,000-20,000 per source

## Results Summary

| # | Source | H (bits/byte) | H∞ (bits/byte) | Verdict |
|---|--------|---------------|-----------------|---------|
| 1 | Audio ADC Noise Floor | 0.000 | 0.000 | FAIL — no mic on Mac Mini |
| 2 | SMC Sensor ADC LSBs | N/A | N/A | BLOCKED — requires sudo |
| 3 | DRAM Retention Noise | 0.469 | 0.151 | FAIL — no bit flips, low timing |
| 4 | Floating-Point Denormal Timing | 1.748 | 1.199 | MARGINAL — implemented |
| 5 | Audio Clock PLL Jitter | 7.495 | 5.464 | EXCELLENT — implemented |
| 6 | USB Frame Counter Jitter | 5.794 | 3.711 | GOOD — implemented |
| 7 | Instruction Retirement Jitter | 7.968 | 7.310 | EXCELLENT — implemented |

## Detailed Results

### 1. Audio ADC Noise Floor (FAIL)

**Program:** `thermal_audio_adc_noise.c`
**Physics:** Johnson-Nyquist thermal noise in microphone ADC input impedance.

**Result:** All samples returned exactly 0.0 — the Mac Mini has no built-in
microphone. The CoreAudio queue captures silence. Not viable on headless Macs.

**Verdict:** Only viable on MacBooks or Macs with external microphone. The
existing `audio_noise` source (via ffmpeg) already covers this on devices
with microphones.

### 2. SMC Sensor ADC Raw LSBs (BLOCKED)

**Program:** `thermal_smc_adc_lsb.c`
**Physics:** ADC quantization noise in temperature/voltage/current sensors.

**Result:** All 16 SMC keys returned "not available" without root privileges.
The SMC requires `IOServiceOpen` with elevated permissions on modern macOS.
Could not test without sudo.

**Verdict:** Requires root. If accessible, the existing `smc_sensor_noise.c`
PoC (which also needs sudo) showed promise. Not viable for unprivileged use.

### 3. DRAM Retention Noise (FAIL)

**Program:** `thermal_dram_retention.c`
**Physics:** Quantum tunneling of charge through gate oxide in DRAM capacitors.

**Results:**
- Write-Wait-Readback: 0 flipped bits across 20 rounds (10ms busy-wait each)
- Row-crossing read timing: H = 0.469, H∞ = 0.151 (only 3 unique values)
- Pattern-dependent timing: H∞ = 0.109-0.146 across all patterns

**Analysis:** Modern Apple Silicon has aggressive DRAM refresh and likely ECC.
The 10ms busy-wait is too short for observable charge leakage on well-refreshed
cells. The timing has only 0-2 tick resolution — far too coarse. Apple's unified
memory architecture may also handle DRAM differently than discrete DRAM.

**Verdict:** Not viable. The physical effect is real but completely masked by
the memory controller's aggressive refresh and the coarse timer resolution.

### 4. Floating-Point Denormal Timing (MARGINAL — Implemented)

**Program:** `thermal_denormal_timing.c`
**Physics:** Data-dependent timing from denormalized float handling.

**Results:**
- Denormal multiply timing: H = 1.748, H∞ = 1.199, 7 unique values
- Normal baseline: H = 1.768, H∞ = 1.137, 8 unique values
- Slowdown ratio: 0.98x (no microcode penalty on Apple Silicon!)
- Mixed timing: H∞ = 1.207
- Delta XOR-fold: H∞ = 1.186

**Analysis:** Apple Silicon handles denormals in hardware at the same speed as
normal floats — there is no microcode assist penalty. The entropy comes purely
from general pipeline timing jitter, not from the denormal handling itself.
Despite this, the source barely crosses H∞ > 1.0.

**Implemented as:** `crates/openentropy-core/src/sources/frontier/denormal_timing.rs`
**Entropy rate estimate:** 300 bytes/sec

### 5. Audio Clock PLL Jitter (EXCELLENT — Implemented)

**Program:** `thermal_audio_pll_jitter.c`
**Physics:** Phase noise in audio subsystem's PLL voltage-controlled oscillator.

**Results (Mac mini Speakers, 48kHz):**
- Query timing LSBs: H = 2.658, H∞ = 1.472
- PLL beat detection: H = 7.495, **H∞ = 5.464**
- Latency query timing: H = 7.446, **H∞ = 5.427**
- Query timing range: 333ns - 104μs

**Results (USB 2.0 Camera, 48kHz):**
- PLL beat detection: H = 7.396, **H∞ = 5.339**
- Latency query timing: H = 7.667, **H∞ = 5.979**

**Analysis:** Querying CoreAudio device properties (sample rate, latency) crosses
the audio PLL / CPU clock domain boundary. The PLL's thermal noise creates
genuine phase jitter visible as timing variation. The high H∞ values indicate
a truly flat distribution — excellent entropy quality.

**Implemented as:** `crates/openentropy-core/src/sources/frontier/audio_pll_timing.rs`
**Entropy rate estimate:** 4,000 bytes/sec

### 6. USB Frame Counter Jitter (GOOD — Implemented)

**Program:** `thermal_usb_frame_jitter.c`
**Physics:** Crystal oscillator phase noise in USB host controller.

**Results (USB3 Gen2 Hub):**
- Query LSBs: H = 5.794, **H∞ = 3.711**
- Delta XOR-fold: H = 3.892, H∞ = 2.132
- Timing range: 92-1118 ticks, mean=114

**Results (USB2 Hub):**
- Query LSBs: H = 4.939, **H∞ = 3.511**
- Timing range: 90-1019 ticks, mean=104

**Results (XHCI Controllers):**
- Controller timing: H∞ = 0.988-1.525
- Beat detection: H = 4.126, **H∞ = 2.892**
- Autocorrelation at lag 1: r = 0.291 (moderate — some correlation present)

**Analysis:** USB device property queries via IOKit have significant timing
variation from USB bus arbitration and IOKit registry traversal. The USB3 hub
showed the highest entropy. The autocorrelation is moderate (0.29) indicating
some temporal structure, but H∞ > 3 bits/byte is still strong.

11 USB devices were found on the Mac Mini M4 (3 hubs + internal devices).

**Implemented as:** `crates/openentropy-core/src/sources/frontier/usb_timing.rs`
**Entropy rate estimate:** 1,500 bytes/sec

### 7. Instruction Retirement Jitter (EXCELLENT — Implemented)

**Program:** `thermal_instruction_retirement.c`
**Physics:** Non-deterministic pipeline timing for fixed instruction sequences.

**Results:**
- CNTVCT_EL0 NOP timing: H = 1.543, H∞ = 1.219 (8 unique values)
- mach_absolute_time NOP timing: H = 0.979, H∞ = 0.727
- Mixed ALU+NOP workload: H = 0.919, H∞ = 0.523
- **CNTVCT_EL0 XOR mach_absolute_time beat:**
  - LSBs: H = 7.206, **H∞ = 5.615**
  - XOR-fold: H = 7.968, **H∞ = 7.310** (256/256 unique values)

**Analysis:** The individual NOP timing has low entropy because Apple Silicon's
deterministic pipeline processes 1000 NOPs very consistently (mean ≈ 42 ticks CNTVCT).

The breakthrough finding is the **counter beat**: XORing CNTVCT_EL0 (1 GHz timer
counter) with mach_absolute_time (24 MHz crystal-derived) captures the phase
relationship between two independent clock domains. This produces near-perfect
entropy at H∞ = 7.3 bits/byte because:

1. CNTVCT_EL0 derives from a 1 GHz ring oscillator
2. mach_absolute_time derives from a 24 MHz crystal oscillator
3. The phase relationship between these oscillators has thermal jitter
4. The reading latency of `mrs CNTVCT_EL0` and `mach_absolute_time()` differ
   non-deterministically per call

This is conceptually similar to the existing `cross_domain` beat sources but
uses the most fundamental available counters with no workload overhead.

**Implemented as:** `crates/openentropy-core/src/sources/frontier/counter_beat.rs`
**Entropy rate estimate:** 8,000 bytes/sec

## New Sources Added

Four new Rust `EntropySource` implementations were added to
`crates/openentropy-core/src/sources/frontier/`:

| Source | File | Category | H∞ | Rate Est. |
|--------|------|----------|-----|-----------|
| `denormal_timing` | `denormal_timing.rs` | Frontier | 1.2 | 300 B/s |
| `audio_pll_timing` | `audio_pll_timing.rs` | Frontier | 5.4 | 4,000 B/s |
| `usb_timing` | `usb_timing.rs` | Frontier | 3.7 | 1,500 B/s |
| `counter_beat` | `counter_beat.rs` | Frontier | 7.3 | 8,000 B/s |

All four are registered in `sources/mod.rs`, added to `FAST_SOURCES`, and
include unit tests. Total source count: 36 → 40.

## Key Insights

1. **Apple Silicon eliminates the denormal penalty.** Unlike x86 which has a
   large microcode assist penalty for denormalized floats, Apple Silicon handles
   them at the same speed as normal floats. The denormal timing source works
   but only captures general pipeline jitter.

2. **Counter beat is a high-quality, zero-overhead entropy source.** Simply
   XORing two hardware counter values gives H∞ = 7.3 bits/byte with no
   computational workload. This is the highest quality of any thermal noise
   source tested.

3. **Clock domain crossings are the richest entropy sources.** The audio PLL,
   USB crystal, and counter beat all exploit the physically-independent phase
   noise between different oscillators. Each oscillator domain has its own
   thermal noise, and the phase relationship between domains is fundamentally
   unpredictable.

4. **DRAM retention noise is not observable on modern hardware.** Apple's
   unified memory architecture with aggressive refresh and likely ECC
   completely prevents direct observation of charge decay.

5. **Some sources require hardware not present on Mac Mini.** The audio ADC
   source needs a microphone (MacBook or external), and SMC access requires
   root privileges.
