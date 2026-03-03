---
title: 'Forensic Analysis'
description: 'Core six-test statistical battery for entropy source quality'
---

Forensic analysis is the baseline battery in openentropy. It evaluates six
properties expected in random data and runs in every profile.

Implemented in `openentropy_core::analysis`.

## Autocorrelation

Measures serial dependence across lags.

| Metric | Description |
|--------|-------------|
| `max_abs_correlation` | Maximum `|r|` across lags |
| `threshold` | Approximate significance threshold (`2/sqrt(n)`) |
| `violations` | Number of lags exceeding threshold |

Low correlation and few violations indicate independence over time.

## Spectral Analysis

Measures frequency-domain structure via DFT.

| Metric | Description |
|--------|-------------|
| `flatness` | Spectral flatness (0..1), higher is whiter |
| `dominant_frequency` | Strongest normalized frequency component |
| `peaks` | Top power-spectrum peaks |

Flatness near 1.0 and weak dominant peaks are expected for white-noise-like
sources.

## Bit Bias

Measures per-bit deviation from 50/50.

| Metric | Description |
|--------|-------------|
| `bit_probabilities` | P(1) for each bit position |
| `overall_bias` | Mean deviation from 0.5 |
| `chi_squared` | Uniformity statistic |
| `p_value` | Approximate p-value |
| `has_significant_bias` | Any bit with meaningful bias |

Low overall bias and no significant per-bit bias indicate healthy bit-level
behavior.

## Distribution Statistics

Compares byte-value distribution to uniform `[0, 255]`.

| Metric | Expected (uniform) |
|--------|--------------------|
| `mean` | `127.5` |
| `skewness` | `~0` |
| `kurtosis` | `~1.8` |
| `ks_p_value` | Prefer `>= 0.01` |

Large skew/kurtosis drift or very low KS p-values indicate non-uniform output.

## Stationarity Test

Tests whether statistical behavior remains stable over time using 10 windows.

| Metric | Description |
|--------|-------------|
| `is_stationary` | Heuristic stationarity flag |
| `f_statistic` | ANOVA-like F statistic |
| `window_means` | Mean per window |
| `window_std_devs` | Standard deviation per window |

Non-stationary behavior can indicate drift from temperature, scheduler load,
or source-state transitions.

## Runs Analysis

Measures run structure in repeated values.

| Metric | Description |
|--------|-------------|
| `longest_run` | Longest identical-value streak |
| `expected_longest_run` | Expected longest streak |
| `total_runs` | Total run count |
| `expected_runs` | Expected total runs |

Run metrics far from expectation can indicate stickiness or insufficient mixing.

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Verdict System](/openentropy/concepts/analysis-verdicts/)
