---
title: 'Trial Analysis'
description: 'PEAR-style 200-bit trial methodology'
---

This document defines the session-trial analysis implemented in
`openentropy-core::trials`.

## Scope

OpenEntropy uses the term "PEAR-style" as a historical reference to the
Princeton Engineering Anomalies Research (PEAR) methodology. This project is not
affiliated with or endorsed by PEAR.

## Core Trial Model

- Trials are fixed-length Bernoulli samples, default `N = 200` bits per trial.
- For each trial:
  - `ones` = number of set bits.
  - `Z_trial = (ones - N/2) / sqrt(N/4)`.
- For a run of `k` trials:
  - cumulative deviation = `sum_i(ones_i - N/2)`.
  - terminal `Z = cumulative_deviation / sqrt(k * N/4)`.
  - effect size = `terminal_z / sqrt(k)`.

This matches the normal/binomial framing used in PEAR-era REG analyses for
200-sample trials.

## Cross-Session Combination

When combining multiple sessions, OpenEntropy uses weighted Stouffer
composition:

`Z_combined = sum_i(w_i * Z_i) / sqrt(sum_i(w_i^2))`, with `w_i = sqrt(n_i)`.

Here `n_i` is the number of trials for session `i`. Zero-trial sessions are
excluded from composition.

## Calibration Gate

The optional `record --calibrate` gate checks per-source baseline suitability
before recording:

- `|terminal_z| < 2.0`
- `bit_bias < 0.005`
- `shannon_entropy > 7.9` bits/byte
- `std_z in [0.85, 1.15]`

These are practical baseline constraints for rejecting strongly biased or
unstable sources before trial-driven experiments.

## Primary Historical References

1. Jahn, Dunne, Nelson, Dobyns, and Bradish (1997), *Correlations of Random
Binary Sequences with Pre-Stated Operator Intention: A Review of a 12-Year
Program*.
https://www.pear-lab.com/pdfs/1997-correlations-random-binary-sequences-12-year-review.pdf

2. Jahn, Dunne, and collaborators (2000), *Mind/Machine Interaction Consortium:
PortREG Replication Experiments*.
https://www.pear-lab.com/pdfs/2000-mmi-consortium-portreg-replication.pdf

3. Dobyns, Dunne, and Nelson (2004), *The MegaREG Experiment*.
https://www.pear-lab.com/pdfs/2004-megareg-replication-interpretation.pdf

4. Jahn and Dunne (1991), *Count Population Profiles in a Random Event
Generator Experiment*.
https://www.pear-lab.com/pdfs/1991-count-population-profiles.pdf

PEAR publications index:
https://www.pear-lab.com/publications
