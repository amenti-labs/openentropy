---
title: 'Entropy Sources'
description: 'Catalog hub for all source categories and platform availability'
---

openentropy currently ships 63 entropy sources across 13 categories. This page
is the navigation hub for the split source catalog.

## Source Categories

- [Timing Sources](/openentropy/concepts/sources/timing/)
- [Scheduling Sources](/openentropy/concepts/sources/scheduling/)
- [System Sources](/openentropy/concepts/sources/system/)
- [Network Sources](/openentropy/concepts/sources/network/)
- [IO Sources](/openentropy/concepts/sources/io/)
- [IPC Sources](/openentropy/concepts/sources/ipc/)
- [Microarchitecture Sources](/openentropy/concepts/sources/microarch/)
- [GPU Sources](/openentropy/concepts/sources/gpu/)
- [Thermal Sources](/openentropy/concepts/sources/thermal/)
- [Signal Sources](/openentropy/concepts/sources/signal/)
- [Sensor Sources](/openentropy/concepts/sources/sensor/)
- [Quantum Sources](/openentropy/concepts/sources/quantum/)

## Platform Availability

| Platform | Available Sources | Notes |
|----------|:-----------------:|-------|
| MacBook (M-series) | 63/63 | Full suite |
| Mac Mini/Studio/Pro | 50-55/63 | Some sensor channels unavailable |
| Intel Mac | ~18/63 | ARM-specific sources unavailable |
| Linux | ~14/63 | macOS-specific APIs unavailable |

## Entropy Quality Notes

- Raw source output can be biased and structured.
- Conditioned pool output is the operational path for cryptographic use.
- For deep interpretation, run profile-based analysis and inspect verdicts.

## Adding a New Source

1. Implement `EntropySource` in `crates/openentropy-core/src/sources/<category>/`.
2. Define `SourceInfo` with mechanism and platform requirements.
3. Register source in the category `mod.rs`.
4. Add tests and document the source in the corresponding docs category page.
