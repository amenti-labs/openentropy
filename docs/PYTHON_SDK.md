# Python SDK Reference

Python bindings for the openentropy Rust library. The package provides the same API whether backed by the native Rust extension (via PyO3) or the pure-Python fallback.

## Installation

### From PyPI (pure Python)

```bash
pip install openentropy
```

This installs the pure-Python package. All features work, but collection and conditioning run in Python.

### With optional hardware sources

```bash
# Audio source (microphone thermal noise)
pip install openentropy[audio]

# Camera source (sensor dark current)
pip install openentropy[camera]

# macOS-specific sources (WiFi RSSI, Bluetooth)
pip install openentropy[macos]

# TUI monitor dependencies
pip install openentropy[tui]

# Everything
pip install openentropy[all]
```

### From source with Rust extension (recommended)

Building from source compiles the Rust extension via maturin, providing native performance:

```bash
git clone https://github.com/amenti-labs/openentropy
cd openentropy

# Install Rust toolchain if not present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build and install with Rust extension
pip install maturin
maturin develop --release

# Or install in editable mode for development
pip install -e ".[dev]"
```

### Checking the backend

```python
import openentropy

# True if the Rust extension is loaded, False for pure Python
print(openentropy.__rust_backend__)

# Version string (from Rust if available, else Python)
print(openentropy.__version__)
```

---

## Core API

### EntropyPool

The central class. Manages multiple entropy sources, collects raw entropy, applies SHA-256 conditioning, and produces high-quality random output.

```python
from openentropy import EntropyPool
```

#### Creating a pool

```python
# Auto-discover all available sources on this machine
pool = EntropyPool.auto()

# Create an empty pool with optional seed
pool = EntropyPool()
pool = EntropyPool(seed=b"optional-seed-bytes")
```

#### Generating random bytes

```python
# Get 256 bytes of SHA-256 conditioned output
data = pool.get_random_bytes(256)
assert isinstance(data, bytes)
assert len(data) == 256

# Get a large block
block = pool.get_random_bytes(1_048_576)  # 1 MB
```

The pool automatically collects from sources when the internal buffer runs low. No manual collection is required for normal use.

#### Manual collection

```python
# Collect from all sources (serial)
bytes_collected = pool.collect_all()

# Collect in parallel with a timeout (Rust backend only)
bytes_collected = pool.collect_all(parallel=True, timeout=10.0)
```

#### Health monitoring

```python
# Get health report as a dictionary
report = pool.health_report()
print(f"Healthy sources: {report['healthy']}/{report['total']}")
print(f"Raw bytes collected: {report['raw_bytes']}")
print(f"Output bytes generated: {report['output_bytes']}")
print(f"Buffer size: {report['buffer_size']}")

# Per-source details
for source in report['sources']:
    status = "OK" if source['healthy'] else "FAIL"
    print(f"  {source['name']:25s} [{status}] H={source['entropy']:.2f} "
          f"bytes={source['bytes']} time={source['time']:.3f}s "
          f"failures={source['failures']}")

# Pretty-print to stdout
pool.print_health()
```

#### Source information

```python
# Get metadata for all registered sources
sources = pool.sources()
for s in sources:
    print(f"{s['name']:25s} [{s['category']}] {s['description']}")
    print(f"  Physics: {s['physics']}")
    print(f"  Estimated rate: {s['entropy_rate_estimate']} b/s")

# Number of registered sources
print(f"Source count: {pool.source_count}")
```

---

### Platform Detection

```python
from openentropy import detect_available_sources

# Returns a list of dicts with source metadata
sources = detect_available_sources()
for s in sources:
    print(f"{s['name']:25s} [{s['category']}] rate={s['entropy_rate_estimate']}")

print(f"\n{len(sources)} sources available on this machine")
```

This function is only available when the Rust backend is loaded.

---

## NumPy Integration

openentropy provides a NumPy-compatible random number generator backed by hardware entropy.

### EsotericRandom

A factory function that returns a `numpy.random.Generator` backed by hardware entropy.

```python
from openentropy import EsotericRandom

rng = EsotericRandom()
```

#### Generating random data

```python
# Uniform floats in [0, 1)
floats = rng.random(10)

# Random integers
ints = rng.integers(0, 256, size=100)
ints = rng.integers(low=0, high=1000000, size=(10, 10))

# Gaussian (normal) samples
normal = rng.standard_normal(1000)
normal = rng.normal(loc=5.0, scale=2.0, size=500)

# Raw bytes
raw = rng.bytes(32)

# Random choice from an array
choices = rng.choice([1, 2, 3, 4, 5], size=10, replace=True)

# In-place shuffle
array = list(range(100))
rng.shuffle(array)

# Random permutation
perm = rng.permutation(52)  # Shuffle a deck of cards

# Exponential distribution
exp_samples = rng.exponential(scale=1.0, size=100)

# Binomial distribution
binom = rng.binomial(n=10, p=0.5, size=1000)

# Uniform distribution with bounds
uniform = rng.uniform(low=-1.0, high=1.0, size=50)
```

#### Full NumPy Generator compatibility

`EsotericRandom()` returns a standard `numpy.random.Generator` object. All methods documented in the [NumPy Generator API](https://numpy.org/doc/stable/reference/random/generator.html) are available.

### EsotericBitGenerator

The underlying `numpy.random.BitGenerator` subclass. Use directly if you need lower-level control or want to construct a Generator manually.

```python
from openentropy import EsotericBitGenerator
import numpy as np

bg = EsotericBitGenerator()
rng = np.random.Generator(bg)

# Now use rng as any NumPy Generator
values = rng.random(100)
```

---

## NIST Test Battery

Run the complete 31-test statistical battery on arbitrary byte data.

### Running tests

```python
from openentropy import run_all_tests, calculate_quality_score

# Test any bytes object
data = pool.get_random_bytes(10000)
results = run_all_tests(data)

# Each result is a dict
for r in results:
    status = "PASS" if r['passed'] else "FAIL"
    p_str = f"p={r['p_value']:.4f}" if r['p_value'] is not None else "N/A"
    print(f"  [{status}] {r['grade']} {r['name']:30s} {p_str} -- {r['details']}")
```

### Calculating quality score

```python
score = calculate_quality_score(results)
print(f"Quality score: {score:.1f}/100")

# Determine grade
if score >= 80:
    grade = "A"
elif score >= 60:
    grade = "B"
elif score >= 40:
    grade = "C"
elif score >= 20:
    grade = "D"
else:
    grade = "F"
print(f"Grade: {grade}")
```

### Test result format

Each test result is a dictionary with the following keys:

| Key | Type | Description |
|-----|------|-------------|
| `name` | `str` | Test name (e.g., "Monobit Frequency") |
| `passed` | `bool` | Whether the test passed (p >= 0.01 or metric threshold) |
| `p_value` | `float` or `None` | p-value where applicable |
| `statistic` | `float` | Test statistic value |
| `details` | `str` | Human-readable details |
| `grade` | `str` | Letter grade: A (p >= 0.1), B (>= 0.01), C (>= 0.001), D (>= 0.0001), F |

### List of all 31 tests

| # | Test | Category | Has p-value |
|---|------|----------|:-----------:|
| 1 | Monobit Frequency | Frequency | Yes |
| 2 | Block Frequency | Frequency | Yes |
| 3 | Byte Frequency | Frequency | Yes |
| 4 | Runs Test | Runs | Yes |
| 5 | Longest Run of Ones | Runs | Yes |
| 6 | Serial Test | Serial | Yes |
| 7 | Approximate Entropy | Serial | Yes |
| 8 | DFT Spectral | Spectral | Yes |
| 9 | Spectral Flatness | Spectral | No |
| 10 | Shannon Entropy | Entropy | No |
| 11 | Min-Entropy | Entropy | No |
| 12 | Permutation Entropy | Entropy | No |
| 13 | Compression Ratio | Entropy | No |
| 14 | Kolmogorov Complexity | Entropy | No |
| 15 | Autocorrelation | Correlation | Yes |
| 16 | Serial Correlation | Correlation | Yes |
| 17 | Lag-N Correlation | Correlation | No |
| 18 | Cross-Correlation | Correlation | Yes |
| 19 | Kolmogorov-Smirnov | Distribution | Yes |
| 20 | Anderson-Darling | Distribution | No |
| 21 | Overlapping Template | Pattern | Yes |
| 22 | Non-overlapping Template | Pattern | Yes |
| 23 | Maurer's Universal | Pattern | Yes |
| 24 | Binary Matrix Rank | Advanced | Yes |
| 25 | Linear Complexity | Advanced | Yes |
| 26 | Cumulative Sums | Advanced | Yes |
| 27 | Random Excursions | Advanced | No |
| 28 | Birthday Spacing | Advanced | Yes |
| 29 | Bit Avalanche | Practical | Yes |
| 30 | Monte Carlo Pi | Practical | No |
| 31 | Mean & Variance | Practical | Yes |

---

## Complete Example

```python
#!/usr/bin/env python3
"""Example: generate hardware-entropy-backed random data and verify its quality."""

from openentropy import EntropyPool, EsotericRandom

# Create a pool with all available sources
pool = EntropyPool.auto()
print(f"Sources available: {pool.source_count}")

# Generate random bytes
data = pool.get_random_bytes(10000)
print(f"Generated {len(data)} bytes of conditioned entropy")

# Show health
pool.print_health()

# Use as a NumPy Generator
rng = EsotericRandom()
print(f"\n10 random floats: {rng.random(10)}")
print(f"5 random ints [0,100): {rng.integers(0, 100, size=5)}")
print(f"Normal sample: {rng.standard_normal():.4f}")

# Run NIST test battery (Rust backend only)
try:
    from openentropy import run_all_tests, calculate_quality_score

    results = run_all_tests(data)
    passed = sum(1 for r in results if r['passed'])
    score = calculate_quality_score(results)

    print(f"\nNIST Test Battery: {passed}/{len(results)} passed")
    print(f"Quality Score: {score:.1f}/100")

    for r in results:
        status = "PASS" if r['passed'] else "FAIL"
        print(f"  [{status}] {r['grade']} {r['name']}")
except ImportError:
    print("\nNIST tests require the Rust backend (run: maturin develop --release)")
```

---

## Rust Backend vs Pure Python

| Feature | Rust Backend | Pure Python |
|---------|:----------:|:-----------:|
| `EntropyPool.auto()` | Native Rust sources | Python source implementations |
| `get_random_bytes()` | SHA-256 via `sha2` crate | `hashlib.sha256` |
| `collect_all(parallel=True)` | `std::thread::scope` | Not supported |
| `run_all_tests()` | Available | Not available |
| `calculate_quality_score()` | Available | Not available |
| `detect_available_sources()` | Available | Use `platform.detect_available_sources()` |
| `EsotericRandom` / `EsotericBitGenerator` | Works with either backend | Works with either backend |
| Performance | ~10-100x faster collection | Baseline |

The Rust backend is recommended for production use. The pure-Python fallback ensures the package always works, even without a Rust toolchain.

---

## API Summary

### Module-level exports

```python
import openentropy

openentropy.__version__        # str: version string
openentropy.__rust_backend__   # bool: True if Rust extension loaded

# Classes
openentropy.EntropyPool        # Main entropy pool
openentropy.EntropySource      # Base class for sources (Python side)
openentropy.EsotericBitGenerator  # NumPy BitGenerator (lazy import)
openentropy.EsotericRandom     # NumPy Generator factory (lazy import)

# Functions (Rust backend only)
openentropy.detect_available_sources()  # -> list[dict]
openentropy.run_all_tests(data)         # -> list[dict]
openentropy.calculate_quality_score(results)  # -> float
```

### EntropyPool methods

| Method | Signature | Returns |
|--------|-----------|---------|
| `auto()` | `@staticmethod` | `EntropyPool` |
| `__init__()` | `seed: bytes = None` | `EntropyPool` |
| `get_random_bytes()` | `n_bytes: int` | `bytes` |
| `collect_all()` | `parallel: bool = False, timeout: float = 10.0` | `int` (bytes collected) |
| `health_report()` | -- | `dict` |
| `print_health()` | -- | `None` (prints to stdout) |
| `sources()` | -- | `list[dict]` |
| `source_count` | `@property` | `int` |
