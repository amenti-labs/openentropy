---
title: 'Entropy Breakdown'
description: 'NIST-style min-entropy estimators and entropy grade interpretation'
---

The entropy module provides a detailed min-entropy assessment inspired by
NIST SP 800-90B. It runs multiple estimators and reports conservative values.

Implemented in `openentropy_core::conditioning`.

## Estimators

| Estimator | Method | Notes |
|-----------|--------|-------|
| Shannon | Information entropy | Classical `H = -sum p log2 p`, max 8 bits/byte |
| MCV | Most common value | Conservative min-entropy estimate |
| Collision | Collision spacing | Repetition-distance based estimate |
| Markov | Transition model | Captures sequential dependence |
| Compression | Universal compression | Pattern recurrence estimate |
| t-Tuple | Tuple frequency | Repeated tuple dominance estimate |

`min_entropy` uses the conservative estimate used by the core report; a
diagnostic floor is also available for additional caution.

## Entropy Grade

| Grade | Min-Entropy | Interpretation |
|:-----:|:-----------:|----------------|
| A | `>= 6.0` | Excellent density for cryptographic seeding |
| B | `>= 4.0` | Good entropy density |
| C | `>= 2.0` | Moderate redundancy present |
| D | `>= 1.0` | High predictability risk |
| F | `< 1.0` | Insufficient entropy |

## When To Use

- `--profile security` for security-focused validation
- `--profile deep` for broad research characterization
- `--entropy` to enable entropy breakdown in custom runs

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Forensic Analysis](/openentropy/concepts/analysis-forensic/)
- [Verdict System](/openentropy/concepts/analysis-verdicts/)
