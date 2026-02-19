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
openentropy.entropy_measurements
openentropy.quantum_assess_batch
openentropy.quick_min_entropy
openentropy.quick_shannon
openentropy.grade_min_entropy
openentropy.quick_quality
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

q = pool.quantum_report(sample_bytes=1024, min_pair_samples=64, telemetry=True)
report = q["experimental"]["quantum_proxy_v3"]["report"]
print(report["aggregate"]["quantum_to_classical"])
print(report["aggregate"]["quantum_fraction_ci_low"], report["aggregate"]["quantum_fraction_ci_high"])
print(report["calibration"]["global_prior"])
print(report["config"]["coupling_fdr_alpha"], report["config"]["coupling_use_fdr_gate"])
print(report["sources"][0]["coupling_significant_pair_fraction_any"])
print(report["sources"][0]["stress_sensitivity_effective"], report["sources"][0]["telemetry_confound_penalty"])
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
    entropy_measurements,
    quantum_assess_batch,
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

m = entropy_measurements(data, elapsed_seconds=0.25)
print(m["shannon_entropy"], m["min_entropy"], m["throughput_bps"])

batch = quantum_assess_batch([
    {"name": "audio_noise", "category": "sensor", "min_entropy_bits": 6.5, "quality_factor": 0.9},
    {"name": "sysctl_deltas", "category": "system", "min_entropy_bits": 2.2, "quality_factor": 0.7},
], telemetry=True)
print(batch["aggregate"]["quantum_fraction"])
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

## Notes

- The API is provided by the compiled extension module `openentropy.openentropy`.
- If you run examples from the repository root, Python may import the local package directory first. Use a clean environment and run from outside the repo when validating built wheels.
