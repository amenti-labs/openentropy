"""NumPy-compatible random generator backed by esoteric entropy sources.

Usage::

    from esoteric_entropy import EsotericRandom
    rng = EsotericRandom()
    rng.random(10)
    rng.integers(0, 256, size=100)
"""

from __future__ import annotations

import struct

import numpy as np


class EsotericBitGenerator:
    """A BitGenerator-like object backed by the esoteric entropy pool.

    Not a true numpy BitGenerator subclass (that requires C capsules),
    but compatible with our EsotericGenerator wrapper.

    Parameters
    ----------
    pool : EntropyPool or None
        Pre-configured pool. If None, ``EntropyPool.auto()`` is used.
    """

    def __init__(self, pool=None):
        if pool is None:
            from esoteric_entropy.pool import EntropyPool
            pool = EntropyPool.auto()
        self._pool = pool
        self._buf = bytearray()

    def _refill(self, n: int) -> None:
        while len(self._buf) < n:
            self._buf.extend(self._pool.get_random_bytes(max(n, 4096)))

    def _take(self, n: int) -> bytes:
        self._refill(n)
        out = bytes(self._buf[:n])
        self._buf = self._buf[n:]
        return out

    def random_raw(self, n: int = 1) -> np.ndarray:
        raw = self._take(n * 8)
        return np.frombuffer(raw, dtype=np.uint64)[:n].copy()

    @property
    def state(self) -> dict:
        return {
            "bit_generator": "EsotericBitGenerator",
            "pool_sources": len(self._pool.sources),
            "buffer_size": len(self._buf),
        }


class EsotericGenerator:
    """NumPy Generator-compatible RNG backed by esoteric entropy.

    Provides the same interface as ``numpy.random.Generator`` for common operations.

    Usage::

        from esoteric_entropy import EsotericRandom
        rng = EsotericRandom()
        rng.random(10)
        rng.integers(0, 256, size=100)
        rng.standard_normal(1000)
        rng.bytes(32)
    """

    def __init__(self, bit_generator: EsotericBitGenerator | None = None):
        self._bg = bit_generator or EsotericBitGenerator()
        # Use a numpy Generator seeded from our entropy for distributions
        seed_bytes = self._bg._take(32)
        seed_int = int.from_bytes(seed_bytes, "little") % (2**128)
        self._rng = np.random.Generator(np.random.PCG64(seed_int))
        self._reseed_counter = 0
    
    def _maybe_reseed(self) -> None:
        """Periodically reseed the internal generator from hardware entropy."""
        self._reseed_counter += 1
        if self._reseed_counter >= 100:
            seed_bytes = self._bg._take(32)
            seed_int = int.from_bytes(seed_bytes, "little") % (2**128)
            self._rng = np.random.Generator(np.random.PCG64(seed_int))
            self._reseed_counter = 0

    def random(self, size=None):
        """Random floats in [0, 1)."""
        self._maybe_reseed()
        return self._rng.random(size)

    def integers(self, low, high=None, size=None, dtype=np.int64, endpoint=False):
        """Random integers."""
        self._maybe_reseed()
        return self._rng.integers(low, high, size=size, dtype=dtype, endpoint=endpoint)

    def standard_normal(self, size=None):
        """Standard normal distribution."""
        self._maybe_reseed()
        return self._rng.standard_normal(size)

    def normal(self, loc=0.0, scale=1.0, size=None):
        """Normal distribution."""
        self._maybe_reseed()
        return self._rng.normal(loc, scale, size)

    def uniform(self, low=0.0, high=1.0, size=None):
        """Uniform distribution."""
        self._maybe_reseed()
        return self._rng.uniform(low, high, size)

    def bytes(self, length: int) -> bytes:
        """Random bytes directly from the entropy pool."""
        return self._bg._take(length)

    def choice(self, a, size=None, replace=True, p=None):
        """Random choice."""
        self._maybe_reseed()
        return self._rng.choice(a, size=size, replace=replace, p=p)

    def shuffle(self, x):
        """Shuffle in place."""
        self._maybe_reseed()
        return self._rng.shuffle(x)

    def permutation(self, x):
        """Random permutation."""
        self._maybe_reseed()
        return self._rng.permutation(x)

    @property
    def bit_generator(self):
        return self._bg
