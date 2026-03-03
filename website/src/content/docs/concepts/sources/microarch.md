---
title: 'Microarchitecture Sources'
description: 'CPU pipeline, coherence, and hardware state timing sources'
---

Microarchitecture sources extract entropy from nondeterministic CPU and
interconnect behavior at low levels.

## Sources

- `speculative_execution`
- `dvfs_race`
- `tlb_shootdown`
- `amx_timing`
- `icc_atomic_contention`
- `prefetcher_state`
- `aprr_jit_timing`
- `sev_event_timing`
- `cntfrq_cache_timing`
- `gxf_register_timing`
- `dual_clock_domain`
- `sitva`
- `memory_bus_crypto`
- `commoncrypto_aes_timing`
- `cas_contention`
- `denormal_timing`

## Notes

This category is broad and platform-sensitive. Validate with `deep` profile and
cross-run comparisons before drawing conclusions from a single metric.
