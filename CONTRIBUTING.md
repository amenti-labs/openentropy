# Contributing to esoteric-entropy

## Development Setup

```bash
git clone https://github.com/amenti-labs/esoteric-entropy
cd esoteric-entropy
pip install -e ".[dev]"
make test
make lint
```

## Project Structure

```
esoteric_entropy/
├── __init__.py          # Public API exports
├── pool.py              # Multi-source entropy pool
├── conditioning.py      # Whitening algorithms
├── platform.py          # Source auto-discovery
├── cli.py               # CLI commands
├── http_server.py       # HTTP server (stdlib)
├── numpy_compat.py      # NumPy Generator adapter
├── test_suite.py        # NIST-inspired test battery
├── stats.py             # Statistical utilities
├── report.py            # Report generation
└── sources/
    ├── base.py          # EntropySource ABC
    └── *.py             # Source implementations (20 modules, 30 classes)

tests/                   # pytest test suite
docs/                    # Documentation
explore/                 # Experimental source prototypes
```

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
        return True  # check hardware/software requirements

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        # Collect raw samples, return uint8 array
        ...

    def entropy_quality(self) -> dict:
        data = self.collect(1000)
        return self._quick_quality(data, self.name)
```

3. Register in `esoteric_entropy/sources/__init__.py` (add to imports and `ALL_SOURCES`)
4. Add tests in `tests/test_sources.py`
5. Document in `docs/SOURCES.md` with physics explanation

## Guidelines

- Every source must handle unavailable hardware gracefully (`is_available()` → `False`)
- Type hints on all public functions
- Docstrings explaining the physics behind the entropy source
- No hardcoded paths — use platform detection
- Use full paths for system binaries: `/usr/sbin/ioreg`, `/usr/sbin/sysctl`

## Running Tests

```bash
make test          # run all tests
make lint          # ruff check
pytest tests/ -v   # verbose test output
pytest tests/test_sources.py -k "clock"  # run specific test
```

## Code Style

- Ruff for linting and formatting
- Line length: 100
- Python 3.10+ (use `X | Y` union syntax, not `Union[X, Y]`)
