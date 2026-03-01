# Python SDK Reference

Python bindings for `openentropy` via PyO3.

The current package is a Rust-backed extension module exposed as `openentropy`.

## Installation

Install from PyPI:

```bash
pip install openentropy
```

Build from source (development):

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
pip install maturin
maturin develop
```

## Quick Start

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} sources available")

pool = EntropyPool.auto()
data = pool.get_random_bytes(64)
print(data.hex())
```

## Backend and Version

```python
import openentropy

print(openentropy.__version__)       # package version
print(openentropy.version())         # Rust library version
print(openentropy.__rust_backend__)  # always True in current package
```

## Module Exports

```python
import openentropy

# Class
openentropy.EntropyPool

# Discovery / platform
openentropy.detect_available_sources
openentropy.platform_info
openentropy.detect_machine_info

# Statistical test battery
openentropy.run_all_tests
openentropy.calculate_quality_score

# Conditioning and quality helpers
openentropy.condition
openentropy.min_entropy_estimate
openentropy.quick_min_entropy
openentropy.quick_shannon
openentropy.grade_min_entropy
openentropy.quick_quality

# Analysis
openentropy.full_analysis
openentropy.autocorrelation_profile
openentropy.spectral_analysis
openentropy.bit_bias
openentropy.distribution_stats
openentropy.stationarity_test
openentropy.runs_analysis
openentropy.cross_correlation_matrix
openentropy.pearson_correlation

# Comparison
openentropy.compare
openentropy.aggregate_delta
openentropy.two_sample_tests
openentropy.cliffs_delta
openentropy.temporal_analysis
openentropy.digram_analysis
openentropy.markov_analysis
openentropy.multi_lag_analysis
openentropy.run_length_comparison

# Trials
openentropy.trial_analysis
openentropy.stouffer_combine
openentropy.calibration_check
```

## EntropyPool API

Create a pool:

```python
from openentropy import EntropyPool

pool = EntropyPool()
pool = EntropyPool(seed=b"optional-seed")
pool = EntropyPool.auto()  # auto-discover available sources
```

Collection and output:

```python
pool.collect_all()                          # default collection
pool.collect_all(parallel=True, timeout=5) # parallel collection with timeout

pool.get_random_bytes(32)                  # SHA-256 conditioned
pool.get_raw_bytes(32)                     # raw unconditioned bytes
pool.get_bytes(32, conditioning="raw")     # raw / vonneumann|vn / sha256
```

Single-source sampling:

```python
names = pool.source_names()
name = names[0]

data = pool.get_source_bytes(name, 32, conditioning="sha256")
raw = pool.get_source_raw_bytes(name, 64)
```

Health and source metadata:

```python
report = pool.health_report()
print(report.keys())
# healthy, total, raw_bytes, output_bytes, buffer_size, sources

for s in report["sources"]:
    print(s["name"], s["entropy"], s["min_entropy"], s["healthy"])

infos = pool.sources()
for s in infos:
    print(s["name"], s["category"], s["platform"], s["requirements"])
```

Properties:

```python
print(pool.source_count)
```

## Discovery and Platform Helpers

```python
from openentropy import detect_available_sources, platform_info, detect_machine_info

print(detect_available_sources()[0].keys())
# name, description, category, entropy_rate_estimate

print(platform_info())
# { "system": "...", "machine": "...", "family": "..." }

print(detect_machine_info())
# { "os": "...", "arch": "...", "chip": "...", "cores": ... }
```

## Conditioning and Quality Helpers

```python
from openentropy import (
    condition,
    min_entropy_estimate,
    quick_min_entropy,
    quick_shannon,
    grade_min_entropy,
    quick_quality,
)

data = b"\x01\x02\x03" * 1000

out = condition(data, 64, conditioning="sha256")
print(len(out))

mr = min_entropy_estimate(data)
print(mr["min_entropy"], mr["mcv_estimate"], mr["samples"])

print(quick_min_entropy(data))
print(quick_shannon(data))
print(grade_min_entropy(4.2))  # "B"

qr = quick_quality(data)
print(qr["quality_score"], qr["grade"])
```

## Statistical Test Battery

```python
from openentropy import EntropyPool, run_all_tests, calculate_quality_score

pool = EntropyPool.auto()
data = pool.get_random_bytes(10_000)

results = run_all_tests(data)
score = calculate_quality_score(results)

print(f"{len(results)} tests, score={score:.2f}")
print(results[0].keys())
# name, passed, p_value, statistic, details, grade
```

## Analysis

Analyze raw byte data for entropy quality, bias, and structure. All functions
accept `bytes` and return `dict` (except `pearson_correlation` which returns `float`).

```python
import os
from openentropy import (
    full_analysis, autocorrelation_profile, spectral_analysis,
    bit_bias, distribution_stats, stationarity_test, runs_analysis,
    cross_correlation_matrix, pearson_correlation,
)

data = os.urandom(5000)

# Full per-source analysis — returns all sub-analyses in one call
result = full_analysis("my_source", data)
print(result["shannon_entropy"])   # bits/byte, max 8.0
print(result["min_entropy"])       # MCV estimator, max 8.0
print(result["autocorrelation"])   # nested dict: lags, violations, etc.
print(result["spectral"])          # peaks, flatness, dominant frequency
print(result["bit_bias"])          # overall_bias, per_bit_bias, chi_squared, p_value
print(result["distribution"])      # mean, variance, chi_squared, p_value
print(result["stationarity"])      # is_stationary, segment_means, segment_variances
print(result["runs"])              # longest_run, total_runs, expected_runs
```

Individual analysis functions:

```python
# Autocorrelation at lags 1..max_lag (default 128)
ac = autocorrelation_profile(data, max_lag=64)
print(ac["max_abs_correlation"], ac["violations"])

# Spectral analysis via DFT
sp = spectral_analysis(data)
print(sp["flatness"], sp["dominant_frequency"])

# Bit-level bias
bb = bit_bias(data)
print(bb["overall_bias"], bb["p_value"])

# Byte distribution statistics
ds = distribution_stats(data)
print(ds["mean"], ds["variance"], ds["chi_squared"])

# Stationarity (segment-based)
st = stationarity_test(data)
print(st["is_stationary"])

# Run-length analysis
ra = runs_analysis(data)
print(ra["longest_run"], ra["total_runs"])
```

Cross-source functions:

```python
# Cross-correlation matrix between multiple sources
sources = [("source_a", os.urandom(1000)), ("source_b", os.urandom(1000))]
matrix = cross_correlation_matrix(sources)
print(matrix["pairs"], matrix["flagged_count"])

# Pearson correlation between two byte streams (returns float, not dict)
r = pearson_correlation(os.urandom(1000), os.urandom(1000))
print(r)  # float in [-1, 1]
```

## Comparison

Compare two byte streams with differential statistical tests. All functions take
two `bytes` arguments and return `dict` (except `cliffs_delta` which returns `float`).

```python
import os
from openentropy import (
    compare, aggregate_delta, two_sample_tests, cliffs_delta,
    temporal_analysis, digram_analysis, markov_analysis,
    multi_lag_analysis, run_length_comparison,
)

data_a = os.urandom(5000)
data_b = os.urandom(5000)

# Full differential report — all sub-analyses in one call
result = compare("session_a", data_a, "session_b", data_b)
print(result["aggregate"])      # Shannon/min-entropy/mean/variance deltas
print(result["two_sample"])     # KS, chi-squared, Cliff's delta, Mann-Whitney
print(result["temporal"])       # sliding-window anomaly detection
print(result["digram"])         # digram chi-squared uniformity
print(result["markov"])         # per-bit transition probabilities
print(result["multi_lag"])      # autocorrelation at multiple lags
print(result["run_lengths"])    # byte run-length distributions
```

Individual comparison functions:

```python
# Aggregate statistics delta
agg = aggregate_delta(data_a, data_b)
print(agg["shannon_a"], agg["shannon_b"], agg["cohens_d"])

# Two-sample tests (KS, chi-squared, Mann-Whitney)
ts = two_sample_tests(data_a, data_b)
print(ts["ks_p_value"], ts["chi2_p_value"], ts["mann_whitney_p_value"])

# Cliff's delta — non-parametric effect size (returns float, not dict)
d = cliffs_delta(data_a, data_b)
print(d)  # float in [-1, 1]

# Temporal analysis with sliding window
ta = temporal_analysis(data_a, data_b, window_size=1024, z_threshold=3.0)
print(ta["anomaly_count_a"], ta["anomaly_count_b"])

# Digram, Markov, multi-lag, run-length comparisons
print(digram_analysis(data_a, data_b)["sufficient_data"])
print(markov_analysis(data_a, data_b)["transitions_a"])
print(multi_lag_analysis(data_a, data_b)["lags"])
print(run_length_comparison(data_a, data_b)["distribution_a"])
```

## Trials

PEAR-style trial analysis for entropy data. Slices byte streams into fixed-length
trials and computes cumulative deviation, terminal Z-scores, and effect sizes.
See [Trial Analysis Methodology](TRIALS.md) for details on the statistical model.

```python
import os
from openentropy import trial_analysis, stouffer_combine, calibration_check

data = os.urandom(5000)

# Trial analysis — 200 bits per trial (default), 5000 bytes = 200 trials
result = trial_analysis(data, bits_per_trial=200)
print(result["num_trials"])                # 200
print(result["terminal_z"])                # terminal Z-score
print(result["effect_size"])               # terminal_z / sqrt(num_trials)
print(result["terminal_p_value"])          # two-tailed p-value
print(result["mean_z"], result["std_z"])   # should be ~0 and ~1 for unbiased data
```

Combine multiple sessions via weighted Stouffer:

```python
# Run trial analysis on multiple sessions
t1 = trial_analysis(os.urandom(2500))  # 100 trials
t2 = trial_analysis(os.urandom(2500))  # 100 trials

# Combine — each session weighted by sqrt(num_trials)
combined = stouffer_combine([t1, t2])
print(combined["num_sessions"])         # 2
print(combined["total_trials"])         # 200
print(combined["stouffer_z"])           # combined Z-score
print(combined["p_value"])             # combined p-value
print(combined["combined_effect_size"]) # stouffer_z / sqrt(total_trials)
```

Calibration check before recording:

```python
# Verify a source is suitable for trial experiments
cal = calibration_check(os.urandom(50_000))
print(cal["is_suitable"])       # bool
print(cal["warnings"])          # list of warning strings
print(cal["shannon_entropy"])   # bits/byte
print(cal["bit_bias"])          # deviation from 0.5
print(cal["analysis"])          # nested TrialAnalysis dict
```

## Notes

- The API is provided by the compiled extension module `openentropy.openentropy`.
- If you run examples from the repository root, Python may import the local package directory first. Use a clean environment and run from outside the repo when validating built wheels.
