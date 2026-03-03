# openentropy (Python)

Hardware entropy for Python, backed by Rust (PyO3 + maturin build).

OpenEntropy harvests randomness from physical noise sources on your machine (timing jitter, thermal effects, scheduler variance, I/O timing, and more), then exposes:

- `detect_available_sources()` for source discovery
- `get_source_bytes()` and `get_source_raw_bytes()` for single-source sampling
- `get_random_bytes()` for conditioned output
- `get_bytes(..., conditioning="raw|vonneumann|sha256")` for research/analysis workflows
- `run_all_tests()` + `calculate_quality_score()` for statistical checks

## Install

```bash
pip install openentropy
```

## Quick start

```python
from openentropy import EntropyPool, detect_available_sources

sources = detect_available_sources()
print(f"{len(sources)} sources available")

pool = EntropyPool.auto()
source = sources[0]["name"]
data = pool.get_source_bytes(source, 64, conditioning="sha256")
print(data.hex())
```

## Docs

- Project: https://github.com/amenti-labs/openentropy
- Python SDK docs: https://github.com/amenti-labs/openentropy/blob/main/docs/PYTHON_SDK.md
