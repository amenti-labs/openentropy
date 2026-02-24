# Quantum Noise Proximity — Apple Silicon Sources
*2026-02-24 | M4 Mac mini, macOS 15.3*

## Summary

Of 60 validated sources, six have defensible connections to physical quantum noise.
Ranked by directness of the quantum coupling:

---

## Tier 1 — Real Oscillator Phase Noise (Johnson-Nyquist + Shot Noise)

These sources directly tap VCO thermal jitter — the same mechanism used in hardware TRNGs.

| Source | H∞ | Mechanism |
|--------|-----|-----------|
| `audio_pll_timing` | **6.80** | Audio PLL VCO ring-oscillator jitter. Johnson-Nyquist noise in transistors + charge pump shot noise. Mechanistically identical to the SEP TRNG. Best H∞ in library. |
| `dual_clock_domain` | **~7.0** | Beat between 24 MHz ARM crystal and 41 MHz undocumented Apple SoC timer. Two physically independent VCOs — phase drift integrates independent thermal noise from both. |
| `counter_beat` | **6.27** | Beat between CPU crystal (24 MHz CNTVCT) and audio PLL. Same two-oscillator principle. |
| `display_pll` | **6.51** | Display pixel-clock PLL (~533 MHz) VCO phase noise. Independent oscillator from audio and CPU. |
| `pcie_pll` | **6.38** | PCIe/Thunderbolt PLL with **spread-spectrum clocking** (±0.5% @ ~33 kHz). SSC deliberately dithers the center frequency; stochastic phase noise rides on top. |

**Why these are genuine:** Ring oscillator phase noise at room temperature is dominated by Johnson-Nyquist thermal noise in the feedback transistors (kTR noise). At the quantum level this is zero-point electromagnetic field fluctuations — the classical thermal regime is a high-temperature limit of QED vacuum fluctuations.

---

## Tier 2 — Quantum Tunneling in DRAM

| Source | H∞ | Mechanism |
|--------|-----|-----------|
| `lpddr5_row_conflict` | **2.37** | LPDDR5 capacitor charge decay via quantum tunneling through ~1–2 nm gate oxide. Cell leakage varies with oxide thickness at angstrom scale. Row-conflict timing captures this stochastic leakage. **Only source in library whose noise mechanism explicitly requires quantum mechanics.** |

---

## Tier 3 — SEP TRNG Chain (Software Bridge)

| Source | H∞ | Mechanism |
|--------|-----|-----------|
| `getentropy_timing` | 0.43* | Times the SEP TRNG reseed slow-path. When the DRBG pool exhausts, the SEP fires ring oscillators and runs a von Neumann corrector. Slow-path timing captures *when the quantum threshold is crossed*. |
| `sep_timing` | 0.68 | SEP ring-oscillator frequency variation seen from outside. One step removed from the TRNG internals. |

*\*H∞ is misleadingly low — the source has a bimodal distribution (fast DRBG path vs slow TRNG path). The entropy is in the timing of the mode switch, not the byte value distribution. See: **Extractor Fix** below.*

---

## Extractor Fix Needed: `getentropy_timing`

The current `extract_timing_entropy` function computes deltas between consecutive timing samples and XOR-folds them to bytes. This is wrong for bimodal sources — the fast path dominates, making one byte value highly probable.

**Correct approach:** separate the two modes, encode the event pattern:

```
slow_path_event[i] = 1 if timing[i] > THRESHOLD else 0
inter_event_interval[i] = index of i-th slow event
```

The `inter_event_interval` distribution is geometric with parameter p = P(TRNG reseed), and the *jitter* in that interval is the genuine quantum signal. A von Neumann extractor on the interval sequence should yield H∞ ≥ 3.0.

**Threshold:** ~50,000 ticks separates the two modes (fast ≈ 900–1,200 ticks, slow ≈ 80,000–300,000 ticks).

---

## Not Quantum (Common Misconceptions)

| Source | Why Not |
|--------|---------|
| `sitva` | Scheduler preemption timing. Classical OS behavior. Entropy = which clock cycle did the interrupt fire — rooted in thermal load, 3 steps from quantum noise. |
| `dvfs_race` | DVFS decisions from thermistor readings. Thermistors have Johnson-Nyquist noise but the DVFS decision is digital (threshold crossing) — most of the thermal signal is lost. |
| `dram_row_buffer` | Row-buffer hit/miss timing. Mediated entirely by memory controller scheduling. Quantum tunneling in cells exists but isn't observable at this measurement layer. |

---

## Connection to QRNG Research

Apple's SEP TRNG → `getentropy_timing` forms the software-accessible end of a chain:

```
Ring oscillator VCO jitter (quantum/thermal)
    ↓
SEP hardware entropy accumulator
    ↓  ← threshold crossing timing = our signal
DRBG reseed event
    ↓
getentropy() slow-path latency spike
    ↓  ← we measure this
Timing sample
```

The audio PLL is the most direct exposure of the same underlying physics (ring oscillator phase noise) without the DRBG intermediary, which is why `audio_pll_timing` has both the best H∞ and the cleanest quantum coupling.

For consciousness/QRNG research: the PLL beat sources (`dual_clock_domain`, `counter_beat`, `audio_pll_timing`) are the highest-fidelity proxies for hardware QRNG output available through standard macOS APIs.
