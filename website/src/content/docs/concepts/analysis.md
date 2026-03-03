---
title: 'Analysis System'
description: 'Statistical analysis of entropy quality — forensic, entropy, chaos, trials, and cross-correlation'
---

openentropy provides a multi-layered statistical analysis system for evaluating
entropy quality. The system is organized into **five analysis categories**, each
targeting a different aspect of randomness, orchestrated through a unified
dispatcher with preset profiles.

The dispatcher is implemented in `openentropy_core::dispatcher` and serves as
the single entry point for CLI, Python, and Rust.

## Quick Start

```bash
# Quick forensic check (10K samples)
openentropy analyze --profile quick

# Full research analysis (100K + all modules)
openentropy analyze --profile deep

# Security audit (NIST battery + entropy + SHA-256)
openentropy analyze --profile security
```

```python
from openentropy import analyze

report = analyze([("my_source", data)], profile="deep")
for src in report["sources"]:
    print(src["label"], src["verdicts"])
```

```rust
use openentropy_core::dispatcher::{analyze, AnalysisProfile};

let config = AnalysisProfile::Deep.to_config();
let report = analyze(&[("my_source", &data)], &config);
```

## Analysis Profiles

Profiles are convenience presets that configure which analysis modules run.
Choose the profile that matches your use case:

| Profile | Forensic | Entropy | Chaos | Trials | Cross-Corr | Use Case |
|---------|:--------:|:-------:|:-----:|:------:|:----------:|----------|
| `quick` | ✓ | — | — | — | — | Fast sanity check (10K samples) |
| `standard` | ✓ | — | — | — | — | Default analysis (50K samples) |
| `deep` | ✓ | ✓ | ✓ | ✓ | ✓ | Full research characterization (100K samples) |
| `security` | ✓ | ✓ | — | — | — | Cryptographic validation (50K, SHA-256) |

All profiles include **forensic analysis** as the baseline. The `deep` profile
enables every module. The `security` profile adds entropy breakdown for
NIST-style min-entropy assessment and uses SHA-256 conditioning.

You can also build a custom configuration by passing individual flags:

```bash
openentropy analyze --chaos --entropy --cross-correlation
```

```python
report = analyze(
    [("src", data)],
    config={"forensic": True, "chaos": True, "entropy": True}
)
```

```rust
let config = AnalysisConfig {
    forensic: true,
    entropy: true,
    chaos: true,
    trials: None,
    cross_correlation: false,
};
let report = analyze(&[("src", &data)], &config);
```

## Forensic Analysis

Forensic analysis is the core battery of six statistical tests that evaluate
the fundamental properties expected of random data. It runs in every profile.

Implemented in `openentropy_core::analysis`.

### Autocorrelation

**What it measures:** Serial dependence between values at different time lags.

Computes the Pearson correlation between the data and lagged copies of itself
at multiple lag values (default 1–128). True random data should show no
correlation at any lag.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `max_abs_correlation` | Maximum \|r\| across all lags |
| `threshold` | 95% significance threshold: 2/√n |
| `violations` | Number of lags exceeding the threshold |

**Interpretation:** For random data, all correlations should be near zero
and within the threshold band. Non-zero correlation at specific lags
indicates periodic structure or memory in the source.

### Spectral Analysis

**What it measures:** Frequency-domain characteristics via discrete Fourier
transform.

Computes the power spectral density and measures how evenly power is
distributed across frequencies. True random data (white noise) has equal
power at all frequencies.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `flatness` | Spectral flatness (Wiener entropy), 0.0–1.0 |
| `dominant_frequency` | Normalized frequency with highest power (0.0–0.5) |
| `peaks` | Top 10 spectral peaks by power |

**Interpretation:** Flatness near 1.0 indicates white noise (uniform power
spectrum). Low flatness indicates tonal structure — power concentrated at
specific frequencies. A dominant frequency far from 0.0 suggests periodic
patterns.

### Bit Bias

**What it measures:** Deviation of individual bit positions from the expected
50/50 distribution.

For each of the 8 bit positions in a byte, counts the proportion of 1-bits.
True random data should have each bit at approximately 0.5 probability.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `bit_probabilities` | Probability of 1 for each bit position (0=LSB, 7=MSB) |
| `overall_bias` | Mean deviation from 0.5 across all bits |
| `chi_squared` | Chi-squared statistic for uniformity |
| `p_value` | Approximate p-value for the chi-squared test |
| `has_significant_bias` | Any bit position deviating > 0.01 from 0.5 |

**Interpretation:** Overall bias near 0 and no significant per-bit bias
indicates unbiased output. Bias in specific bit positions may indicate
hardware-level issues (e.g., a stuck bit, ADC linearity problems).

### Distribution Statistics

**What it measures:** How closely the byte-value distribution matches the
expected uniform distribution over \[0, 255\].

Computes a 256-bin histogram and statistical moments (mean, variance,
skewness, kurtosis) of the byte values. Uses a Kolmogorov–Smirnov style
test against the uniform distribution.

**Key metrics:**

| Metric | Expected (uniform) | Description |
|--------|:------------------:|-------------|
| `mean` | 127.5 | Average byte value |
| `variance` | ~5461 | Spread of byte values |
| `skewness` | ~0 | Asymmetry (0 = symmetric) |
| `kurtosis` | ~1.8 | Peakedness (1.8 = platykurtic for uniform) |
| `ks_statistic` | ~0 | KS distance from uniform |
| `ks_p_value` | > 0.01 | Approximate p-value |

**Interpretation:** Values close to the expected moments indicate good
uniformity. High skewness or kurtosis suggests asymmetric or peaked
distributions. Low KS p-value indicates significant departure from
uniformity.

### Stationarity Test

**What it measures:** Whether the statistical properties of the data remain
stable over time.

Divides the data into 10 equal windows and performs an ANOVA-like comparison
of the window means. Non-stationary data has properties that change over
time, which may indicate source degradation or environmental drift.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `is_stationary` | Heuristic flag (F-statistic < 1.88) |
| `f_statistic` | ANOVA F-statistic comparing window means |
| `window_means` | Per-window mean values (10 windows) |
| `window_std_devs` | Per-window standard deviations |

**Interpretation:** A stationary source has consistent statistical properties
throughout the sample. High F-statistic or `is_stationary = false` suggests
the source's behavior changed during collection — possibly due to thermal
drift, frequency scaling, or load changes.

### Runs Analysis

**What it measures:** Patterns in consecutive identical byte values.

Counts the longest run of the same byte value and the total number of runs
(sequences of identical values). Compares both against expected values for
random data.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `longest_run` | Longest streak of the same byte value |
| `expected_longest_run` | Expected longest run for random data |
| `total_runs` | Total number of value transitions + 1 |
| `expected_runs` | Expected total runs for random data |

**Interpretation:** For random data, the longest run and total runs should
be close to their expected values. An abnormally long run may indicate a
stuck value or output buffer issue. Too few runs may indicate insufficient
mixing.

## Entropy Breakdown

The entropy breakdown provides a detailed min-entropy assessment inspired
by NIST SP 800-90B. It runs six independent estimators, each using a
different statistical approach, and reports the most conservative (lowest)
estimate as the primary min-entropy value.

This module is enabled in `deep` and `security` profiles, or with `--entropy`.

Implemented in `openentropy_core::conditioning`.

### Estimators

| Estimator | Method | Description |
|-----------|--------|-------------|
| **Shannon** | Information theory | Classical Shannon entropy H = -Σ p·log₂(p). Maximum 8.0 bits/byte. |
| **MCV** | Most Common Value | NIST 800-90B MCV estimator. H∞ = -log₂(p_max). Conservative and robust. |
| **Collision** | Collision distance | Estimates entropy from average distance between repeated values. |
| **Markov** | Transition model | First-order Markov chain on byte transitions. Detects sequential dependencies. |
| **Compression** | Maurer universal | Entropy estimate based on recurrence times of patterns. |
| **t-Tuple** | Tuple frequency | Entropy from the most frequent t-length byte tuple. Detects repeated patterns. |

The **MCV estimate** is used as the primary min-entropy value (`min_entropy`
field) because it is the most conservative and aligns with NIST
recommendations. The `heuristic_floor` is the minimum across all diagnostic
estimators.

### Entropy Grade

The min-entropy value is automatically graded:

| Grade | Min-Entropy | Interpretation |
|:-----:|:-----------:|----------------|
| **A** | ≥ 6.0 bits/byte | Excellent — suitable for cryptographic seeding |
| **B** | ≥ 4.0 bits/byte | Good — adequate entropy density |
| **C** | ≥ 2.0 bits/byte | Fair — significant redundancy present |
| **D** | ≥ 1.0 bits/byte | Poor — highly predictable |
| **F** | < 1.0 bits/byte | Failing — insufficient entropy |

## Chaos Theory Analysis

Chaos theory analysis distinguishes **true randomness** from **deterministic
chaos**. A chaotic system can appear random but is actually generated by
deterministic equations with sensitive dependence on initial conditions.
These five metrics test whether the data exhibits signatures of deterministic
dynamics or genuine unpredictability.

This module is enabled in the `deep` profile, or with `--chaos`.

Implemented in `openentropy_core::chaos`.

### Hurst Exponent

**What it measures:** Long-range dependence via rescaled range (R/S) analysis.

**Method:** Computes the slope of log(R/S) vs log(window\_size) across
multiple window sizes.

| H value | Interpretation |
|:-------:|----------------|
| H ≈ 0.5 | Random walk — no long-range dependence |
| H > 0.5 | Persistent — trends tend to continue |
| H < 0.5 | Anti-persistent — trends tend to reverse |

### Lyapunov Exponent

**What it measures:** Sensitivity to initial conditions (Rosenstein method).

**Method:** Measures the average rate of divergence between nearby
trajectories in reconstructed phase space (embedding dimension 3, delay 1).

| λ value | Interpretation |
|:-------:|----------------|
| λ ≈ 0 | No deterministic chaos detected |
| λ > 0 | Chaotic — nearby trajectories diverge exponentially |
| λ < 0 | Convergent — system attracted to fixed point |

### Correlation Dimension

**What it measures:** Fractal dimensionality of the attractor
(Grassberger–Procaccia algorithm).

**Method:** Estimates D₂ by computing the correlation integral at multiple
length scales and finding the scaling exponent.

| D₂ value | Interpretation |
|:--------:|----------------|
| D₂ > 3.0 | High-dimensional — consistent with randomness |
| D₂ 2.0–3.0 | Moderate — possible low-dimensional structure |
| D₂ ≤ 2.0 | Low-dimensional — likely deterministic attractor |

### BiEntropy

**What it measures:** Binary entropy across successive derivatives of the
bit stream.

**Method:** Computes Shannon entropy of the binary data and its successive
XOR derivatives. True random data maintains maximal entropy through all
derivative levels.

**Expected for random data:** BiEn > 0.95

**Metrics:**

| Metric | Description |
|--------|-------------|
| `bien` | BiEntropy value (0.0–1.0) |
| `tbien` | Truncated BiEntropy (alternative weighting) |
| `derivative_entropies` | Shannon entropy at each derivative level |

### Epiplexity (Compression Ratio)

**What it measures:** Compressibility of the data using deflate compression.

**Method:** Compresses the data and compares compressed size to original size.
Also compresses the first-difference series to detect structural patterns
that survive differencing.

**Expected for random data:** compression ratio ≈ 1.0 (incompressible)

**Metrics:**

| Metric | Description |
|--------|-------------|
| `compression_ratio` | compressed\_size / raw\_size |
| `structural_info` | 1.0 − compression\_ratio (structure found) |
| `delta_compression_ratio` | Compressibility of the first-difference series |

## Trial Analysis

PEAR-style 200-bit trial analysis evaluates entropy data as a series of
Bernoulli experiments. Each trial examines a fixed-length bit window
(default 200 bits) and computes a Z-score measuring deviation from the
expected 50/50 bit distribution.

This module is enabled in the `deep` profile, or with `--trials`.

Implemented in `openentropy_core::trials`.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `num_trials` | Number of 200-bit trials extracted |
| `terminal_z` | Cumulative Z-score across all trials |
| `effect_size` | terminal\_z / √(num\_trials) |
| `terminal_p_value` | Two-tailed p-value from terminal Z |
| `mean_z`, `std_z` | Should be ≈ 0 and ≈ 1 for unbiased data |

For the full statistical model, cross-session combination via weighted
Stouffer composition, and calibration gating, see
[Trial Analysis Methodology](/openentropy/concepts/trials/).

## Cross-Correlation

Cross-correlation analysis computes the Pearson correlation coefficient
between every pair of entropy sources and flags pairs with unusually high
correlation.

This module is enabled in the `deep` profile, or with `--cross-correlation`.
It requires **two or more sources** to produce results.

**Key metrics:**

| Metric | Description |
|--------|-------------|
| `pairs` | Correlation coefficient for each source pair |
| `flagged_count` | Number of pairs with \|r\| > 0.3 |

**Why it matters:** Independent entropy sources should be uncorrelated. If
two sources show significant correlation (\|r\| > 0.3), they may share a
common physical mechanism or timing dependency, which reduces the effective
entropy of the combined pool.

Implemented in `openentropy_core::analysis`.

## Verdict System

Every forensic and chaos metric is automatically graded with a verdict:
**PASS**, **WARN**, **FAIL**, or **N/A**.

Verdicts provide at-a-glance quality assessment without requiring manual
threshold interpretation. The `VerdictSummary` in each source report
contains up to 11 verdict fields (6 forensic + 5 chaos).

Implemented in `openentropy_core::verdict`.

### Forensic Verdicts

| Metric | PASS | WARN | FAIL |
|--------|------|------|------|
| Autocorrelation | max \|r\| ≤ 0.05 | max \|r\| ≤ 0.15 | max \|r\| > 0.15 |
| Spectral flatness | ≥ 0.75 | ≥ 0.50 | < 0.50 |
| Bit bias | overall ≤ 0.02, no significant | any significant bit | overall > 0.02 |
| Distribution (KS p) | ≥ 0.01 | ≥ 0.001 | < 0.001 |
| Stationarity | stationary, F ≤ 3.0 | not stationary | F > 3.0 |
| Runs | ratio ≤ 2.0, dev ≤ 0.2 | ratio ≤ 3.0, dev ≤ 0.4 | otherwise |

### Chaos Verdicts

| Metric | PASS | WARN | FAIL | N/A |
|--------|------|------|------|-----|
| Hurst | 0.4 ≤ H ≤ 0.6 | 0.3–0.4 or 0.6–0.7 | H < 0.3 or H > 0.7 | Non-finite |
| Lyapunov | \|λ\| < 0.1 | \|λ\| < 0.2 | \|λ\| ≥ 0.2 | Non-finite |
| Correlation dim | D₂ > 3.0 | D₂ > 2.0 | D₂ ≤ 2.0 | Non-finite |
| BiEntropy | > 0.95 | > 0.90 | ≤ 0.90 | Non-finite |
| Compression | > 0.99 | > 0.95 | ≤ 0.95 | Non-finite |

### Reading a Verdict Summary

A source with all PASS verdicts has strong randomness properties across
every measured dimension. Any WARN or FAIL verdicts indicate areas for
investigation:

- **PASS** — metric within expected range for random data
- **WARN** — metric approaching concerning levels; may warrant further testing with larger samples
- **FAIL** — metric outside acceptable range; source may have structural issues
- **N/A** — metric could not be computed (e.g., insufficient data or non-finite result)

A single FAIL does not necessarily mean the source is unusable — some metrics
are more sensitive than others, and sample size affects reliability. Use the
`deep` profile with large samples for definitive assessment.

### Accessing Verdicts

Verdicts are included in the dispatcher output for every source:

```bash
openentropy analyze --profile deep
# Verdicts appear in the per-source output
```

```python
report = analyze([("src", data)], profile="deep")
verdicts = report["sources"][0]["verdicts"]
# {'autocorrelation': 'PASS', 'spectral': 'PASS', 'bias': 'PASS', ...}
```

```rust
let report = analyze(&[("src", &data)], &config);
let verdicts = &report.sources[0].verdicts;
// verdicts.autocorrelation => Some(Verdict::Pass)
// verdicts.hurst => Some(Verdict::Pass)
```
