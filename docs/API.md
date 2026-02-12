# API Reference

## Core Classes

### `EntropyPool`

The central class. Manages multiple entropy sources, collects and conditions output.

```python
from esoteric_entropy import EntropyPool
```

#### `EntropyPool(seed=None)`
Create a new pool. Optional `seed` (bytes) for initial state.

#### `EntropyPool.auto() → EntropyPool`  (classmethod)
Create a pool with all sources available on this machine.

#### `pool.add_source(source, weight=1.0)`
Register an `EntropySource` instance with optional weight.

#### `pool.get_random_bytes(n_bytes) → bytes`
Return `n_bytes` of SHA-256 conditioned random output. Automatically collects from sources if the buffer is low.

#### `pool.collect_all() → int`
Collect from all sources. Returns total raw bytes collected.

#### `pool.health_report() → dict`
Returns health metrics: `healthy`, `total`, `raw_bytes`, `output_bytes`, `buffer_size`, per-source stats.

#### `pool.sources → list[SourceState]`
List of registered source states.

---

### `EntropySource` (ABC)

Base class for all entropy sources.

```python
from esoteric_entropy.sources.base import EntropySource
```

#### Attributes
- `name: str` — Machine-readable name
- `description: str` — Human-readable description
- `platform_requirements: list[str]` — e.g. `["darwin"]`
- `entropy_rate_estimate: float` — Estimated bits/second

#### Abstract Methods
- `is_available() → bool` — Can this source work on this machine?
- `collect(n_samples=1000) → np.ndarray` — Collect uint8 samples
- `entropy_quality() → dict` — Run quality checks, return grade/score/metrics

#### Helper Methods
- `_quick_shannon(data) → float` — Shannon entropy in bits/byte
- `_quick_quality(data, label) → dict` — Lightweight quality metrics

---

### `EsotericRandom`

Factory function returning a `numpy.random.Generator` backed by hardware entropy.

```python
from esoteric_entropy import EsotericRandom

rng = EsotericRandom()
rng.random(10)                  # 10 uniform floats [0, 1)
rng.integers(0, 256, size=100)  # 100 random ints
rng.standard_normal(1000)       # Gaussian samples
rng.bytes(32)                   # 32 raw bytes
rng.choice([1, 2, 3], size=5)   # random selection
rng.shuffle(array)              # in-place shuffle
rng.permutation(10)             # random permutation
```

### `EsotericBitGenerator`

The underlying `numpy.random.BitGenerator` subclass. Use directly if you need lower-level control.

```python
from esoteric_entropy import EsotericBitGenerator
import numpy as np

bg = EsotericBitGenerator()
rng = np.random.Generator(bg)
```

---

## Conditioning Functions

```python
from esoteric_entropy.conditioning import von_neumann_debias, xor_fold, sha256_condition
```

### `von_neumann_debias(bits) → (output, stats)`
Remove bias from a bit stream. ~25% throughput.

### `xor_fold(data, fold_factor=2) → np.ndarray`
XOR-fold to increase entropy density.

### `sha256_condition(data, output_bytes=32) → bytes`
NIST SP 800-90B approved conditioning.

---

## Platform Detection

```python
from esoteric_entropy.platform import detect_available_sources, platform_info

sources = detect_available_sources()  # list[EntropySource]
info = platform_info()                # dict with system, machine, python
```
