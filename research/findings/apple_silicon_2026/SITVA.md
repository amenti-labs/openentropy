# SITVA — Scheduler-Induced Timing Variance Amplification

**Date:** 2026-02-24  
**Platform:** Apple M4 Mac mini, macOS 15.3

## Discovery

When a companion thread hammers NEON FMLA (FP multiply-accumulate) on a separate core, the OS scheduler responds by:

1. Detecting the compute-intensive workload → promoting threads to P-cores
2. More aggressively preempting the measurement thread
3. Creating a bimodal timing distribution: fast (post-preemption cache-refill burst) vs slow (steady-state P-core)

**Result: timing variance TRIPLES under load.**

```
                  Baseline    Under NEON load   Delta
ISB+CNTVCT CV:    30.3%       113.3%            +83 pp
AES 2-round CV:   66.4%       189.4%            +123 pp
```

## Mechanism

The scheduler migration creates two distinct timing paths:
- **Fast path** (0-17 ticks): just returned from preemption, L1 cache refilled, pipeline primed  
- **Slow path** (41-59 ticks): steady-state execution, cold path through decode

The boundary between these paths is stochastic — it encodes the exact preemption timing from the OS scheduler, which is in turn driven by the companion thread's compute demands, thermal state, and P/E-core migration decisions.

## Novel Primitive

No existing entropy library deliberately uses a companion computation thread to amplify timing variance of a primary measurement thread. Standard practice is to measure timing in isolation.

SITVA inverts this: **create controlled interference to extract more entropy per sample**, rather than isolating to get cleaner samples.

## Implementation

Implemented as `frontier/sitva.rs` in OpenEntropy. The companion thread runs FMLA in 32-instruction bursts with a yield between bursts (prevents starvation). The primary thread measures AES-2-round timing during companion activity.

## Caveats

- CV increase depends on scheduler decisions, which vary by OS load and thermal state
- On heavily-loaded systems, the amplification may be higher or lower
- The entropy is genuine (scheduler state is physically unpredictable) but source-of-randomness is indirect
