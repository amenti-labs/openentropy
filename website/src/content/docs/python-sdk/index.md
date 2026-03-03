---
title: 'Python SDK'
description: 'Python bindings for openentropy via PyO3'
---

The openentropy Python package provides PyO3 bindings to the Rust core library, enabling you to harvest entropy from hardware noise and analyze it programmatically.

## Installation

Install from PyPI:

```bash
pip install openentropy
```

Or build from source:

```bash
git clone https://github.com/amenti-labs/openentropy.git
cd openentropy
pip install maturin
maturin develop
```

## Quick Start

```python
from openentropy import EntropyPool, detect_available_sources, full_analysis

# Discover available entropy sources on your machine
sources = detect_available_sources()
print(f"{len(sources)} entropy sources available")

# Pick one source for focused sampling
source = sources[0]["name"]

# Create pool and sample that single source
pool = EntropyPool.auto()
raw = pool.get_source_raw_bytes(source, 4096)
conditioned = pool.get_source_bytes(source, 64, conditioning="sha256")

print(f"Using source: {source}")
print(conditioned.hex())

result = full_analysis(source, raw)
print(f"Shannon entropy: {result['shannon_entropy']:.4f} bits/byte")
```

## What You Can Do

- **Harvest entropy** from 63 hardware noise sources (thermal, timing, microarchitecture, I/O, GPU, network, sensors)
- **Analyze entropy quality** with statistical tests, min-entropy estimation, autocorrelation, spectral analysis
- **Compare streams** with differential statistical tests and effect sizes
- **Run trials** using PEAR-style methodology for entropy validation
- **Condition output** with SHA-256, Von Neumann debiasing, or raw passthrough

## Next Steps

- **[Python Quick Reference](/openentropy/python-sdk/quick-reference/)** — Most-used calls and workflows
- **[Full API Reference](/openentropy/python-sdk/reference/)** — Complete API documentation with examples for every function
- **[Python Analysis Workflows](/openentropy/python-sdk/analysis/)** — Dispatcher, forensic, chaos, trials, and comparison patterns
- **[Source Catalog](/openentropy/concepts/sources/)** — All 63 entropy sources with physics explanations
- **[Trial Analysis Methodology](/openentropy/concepts/trials/)** — PEAR-style trial analysis and statistical model
