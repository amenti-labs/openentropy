# Contributing to esoteric-entropy

## Adding a New Entropy Source

1. Create `esoteric_entropy/sources/your_source.py`
2. Subclass `EntropySource`:

```python
from esoteric_entropy.sources.base import EntropySource
import numpy as np

class YourSource(EntropySource):
    name = "your_source"
    description = "What physical phenomenon this captures"
    platform_requirements = ["darwin"]  # or [] for cross-platform
    entropy_rate_estimate = 500.0  # bits/second

    def is_available(self) -> bool:
        # Check if hardware/software requirements are met
        return True

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        # Collect raw samples, return uint8 array
        ...

    def entropy_quality(self) -> dict:
        data = self.collect(1000)
        return self._quick_quality(data, self.name)
```

3. Register in `esoteric_entropy/sources/__init__.py`
4. Add tests in `tests/test_sources.py`
5. Run `make lint test`

## Development Setup

```bash
git clone https://github.com/esoteric-entropy/esoteric-entropy
cd esoteric-entropy
make dev   # installs in editable mode with dev deps
make test  # run tests
make lint  # check code quality
```

## Guidelines

- Every source must handle unavailable hardware gracefully (`is_available()` returns `False`)
- Type hints on all public functions
- Docstrings explaining the physics behind the entropy source
- No hardcoded paths — use platform detection
