---
title: 'Python API Reference'
description: 'Complete Python API reference for the openentropy package'
---

Python bindings for `openentropy` via PyO3.

The current package is a Rust-backed extension module exposed as `openentropy`.

For most workflows, start with:

- [Python Quick Reference](/openentropy/python-sdk/quick-reference/)
- [Python Analysis Workflows](/openentropy/python-sdk/analysis/)

Use this page as the exhaustive API surface.

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

# Chaos
openentropy.chaos_analysis
openentropy.hurst_exponent
openentropy.lyapunov_exponent
openentropy.correlation_dimension
openentropy.bientropy
openentropy.epiplexity

# Dispatcher
openentropy.analyze
openentropy.analysis_config

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

For detailed explanations of each analysis category, interpretation guides,
and verdict thresholds, see [Analysis System](/openentropy/concepts/analysis/).

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

## Chaos Theory Analysis

Chaos theory metrics distinguish genuine quantum randomness from deterministic
or structured behavior. See [Chaos Theory Analysis](/openentropy/concepts/analysis-chaos/)
for interpretation guides and verdict thresholds.

```python
from openentropy import (
    chaos_analysis, hurst_exponent, lyapunov_exponent,
    correlation_dimension, bientropy, epiplexity,
)

data = os.urandom(5000)

# Full chaos analysis — all metrics in one call
result = chaos_analysis(data)
print(result["hurst"]["hurst_exponent"])                # H ≈ 0.5 = random walk
print(result["lyapunov"]["lyapunov_exponent"])           # λ ≈ 0 = no chaos
print(result["correlation_dimension"]["dimension"])      # high D₂ = random
print(result["bientropy"]["bien"])                       # high = maximal entropy
print(result["epiplexity"]["compression_ratio"])         # ≈ 1.0 = incompressible

# Individual metrics
hurst = hurst_exponent(data)
lyapunov = lyapunov_exponent(data)
corrdim = correlation_dimension(data)
bien = bientropy(data)
epi = epiplexity(data)
```

| Function | Returns | Description |
|----------|---------|-------------|
| `chaos_analysis(data)` | `dict` | All chaos metrics in one call |
| `hurst_exponent(data)` | `dict` | Hurst exponent (H≈0.5 = random walk) |
| `lyapunov_exponent(data)` | `dict` | Lyapunov exponent (λ≈0 = no chaos) |
| `correlation_dimension(data)` | `dict` | Correlation dimension (high D₂ = random) |
| `bientropy(data)` | `dict` | BiEntropy and TBiEntropy metrics |
| `epiplexity(data)` | `dict` | Compression-based complexity |

## Unified Analysis Dispatcher

The `analyze()` function runs multiple analysis modules in one call with
configurable profiles. See [Analysis System](/openentropy/concepts/analysis/)
for profile details, analysis categories, and the verdict system.

```python
from openentropy import analyze, analysis_config

data = os.urandom(5000)

# Run with a profile preset
report = analyze([("my_source", data)], profile="deep")
for src in report["sources"]:
    print(f"{src['label']}: {src['verdicts']}")

# Get profile defaults
config = analysis_config("deep")
# {'forensic': True, 'entropy': True, 'chaos': True,
#  'trials': {'bits_per_trial': 200}, 'cross_correlation': True}

# Custom config dict
report = analyze(
    [("my_source", data)],
    config={"forensic": True, "chaos": True, "entropy": True}
)
```

**Profiles:**

| Profile | forensic | entropy | chaos | trials | cross_correlation |
|---------|:--------:|:-------:|:-----:|:------:|:-----------------:|
| `quick` | ✓ | | | | |
| `standard` | ✓ | | | | |
| `deep` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `security` | ✓ | ✓ | | | |

| Function | Returns | Description |
|----------|---------|-------------|
| `analyze(sources, config=None, profile=None)` | `dict` | Run selected analyses on source data |
| `analysis_config(profile=None)` | `dict` | Get default config for a profile |

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
See [Trial Analysis Methodology](/openentropy/concepts/trials/) for details on the statistical model.

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

## Benchmarking

### `benchmark_sources(pool, config=None) -> dict`

Run a multi-round benchmark across all sources in a pool. Returns a `BenchReport` dict.

**Parameters:**
- `pool` — `EntropyPool` instance
- `config` — optional dict with keys: `samples_per_round` (int), `rounds` (int), `warmup_rounds` (int), `timeout_sec` (float), `rank_by` (str: `"balanced"` | `"min_entropy"` | `"throughput"`), `include_pool_quality` (bool), `pool_quality_bytes` (int), `conditioning` (str)

**Returns:** dict with keys:
- `generated_unix` — Unix timestamp
- `config` — the config used
- `sources` — list of source report dicts (name, composite, healthy, success_rounds, failures, avg_shannon, avg_min_entropy, avg_throughput_bps, avg_autocorrelation, p99_latency_ms, stability, grade, score)
- `pool` — optional pool quality dict (bytes, shannon_entropy, min_entropy, healthy_sources, total_sources)

```python
from openentropy import EntropyPool, benchmark_sources

pool = EntropyPool.auto()
report = benchmark_sources(pool, {"rounds": 3, "rank_by": "balanced"})
for src in report["sources"]:
    print(f"{src['name']}: grade={src['grade']} score={src['score']:.3f}")
```

### `bench_config_defaults() -> dict`

Return the default `BenchConfig` as a dict. Useful for inspecting defaults before overriding.

## Recording

### `class SessionWriter`

Low-level session writer for recording entropy samples to disk.

**Constructor:** `SessionWriter(sources, output_dir, conditioning="raw", tags=None, note=None, analyze=False)`
- `sources` — list of source names to record
- `output_dir` — directory where session folder will be created
- `conditioning` — `"raw"` | `"vonneumann"` | `"sha256"`
- `tags` — optional dict of string key-value metadata
- `note` — optional string note
- `analyze` — if True, embed statistical analysis in session.json

**Methods:**
- `write_sample(source_name, raw: bytes, conditioned: bytes)` — write one sample
- `finish() -> str` — finalize session, return session directory path
- `total_samples() -> int` — samples written so far
- `elapsed_secs() -> float` — seconds since recording started
- `session_dir() -> str` — path to session directory

```python
from openentropy import EntropyPool, SessionWriter

pool = EntropyPool.auto()
writer = SessionWriter(["clock_jitter"], "sessions", conditioning="raw")
for _ in range(100):
    raw = pool.get_source_raw_bytes("clock_jitter", 1000)
    writer.write_sample("clock_jitter", raw, raw)
path = writer.finish()
print(f"Session saved to: {path}")
```

### `record(pool, sources, duration_secs, conditioning="raw", output_dir="sessions", analyze=False) -> dict`

Convenience function: record entropy from a pool for a fixed duration. Returns session metadata dict.

```python
from openentropy import EntropyPool, record

pool = EntropyPool.auto()
meta = record(pool, ["clock_jitter", "thermal_noise"], duration_secs=30.0)
print(f"Recorded {meta['total_samples']} samples to {meta['id']}")
```

## Sessions

### `list_sessions(dir) -> list[dict]`

List all recorded sessions in a directory. Returns list of session metadata dicts, sorted newest-first. Each dict includes a `path` key with the session directory path.

```python
from openentropy import list_sessions

sessions = list_sessions("sessions")
for s in sessions:
    print(f"{s['id']} — {s['total_samples']} samples — {s['path']}")
```

### `load_session_meta(session_dir) -> dict`

Load session metadata from a session directory. Returns the `session.json` contents as a dict.

### `load_session_raw_data(session_dir) -> dict[str, bytes]`

Load raw entropy data from a session directory. Returns a dict mapping source name → raw bytes.

```python
from openentropy import load_session_meta, load_session_raw_data, full_analysis

meta = load_session_meta("sessions/my-session")
raw = load_session_raw_data("sessions/my-session")
for source, data in raw.items():
    analysis = full_analysis(source, data)
    print(f"{source}: H∞={analysis['min_entropy']:.4f}")
```

## Notes

- The API is provided by the compiled extension module `openentropy.openentropy`.
