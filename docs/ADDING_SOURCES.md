# Adding a New Entropy Source

## Step 1: Create the Source File

Create `esoteric_entropy/sources/your_source.py`:

```python
"""Your entropy source description."""
from __future__ import annotations
import numpy as np
from esoteric_entropy.sources.base import EntropySource

class YourSource(EntropySource):
    name = "your_source"
    description = "Brief description of the physics"
    platform_requirements = []  # e.g. ["darwin"] for macOS-only
    entropy_rate_estimate = 500.0  # bits per second

    def is_available(self) -> bool:
        """Check hardware/software requirements."""
        # Return False if requirements aren't met
        return True

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        """Collect raw samples."""
        # Your collection logic here
        # Return uint8 ndarray
        ...

    def entropy_quality(self) -> dict:
        """Self-test."""
        data = self.collect(1000)
        return self._quick_quality(data, self.name)
```

## Step 2: Register It

In `esoteric_entropy/sources/__init__.py`, add your import and add the class to `ALL_SOURCES`.

## Step 3: Test It

Add tests in `tests/test_sources.py` — at minimum, the parametrized metadata tests will automatically cover your new source.

## Step 4: Document It

Add a section in `docs/SOURCES.md` explaining the physics.

## Guidelines

- `is_available()` must be fast and side-effect-free
- `collect()` must handle hardware failures gracefully (return empty array)
- Always return `np.uint8` arrays
- Extract LSBs from timing measurements (`& 0xFF`)
- Document what physical phenomenon produces the entropy
