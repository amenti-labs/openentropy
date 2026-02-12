"""Compression timing oracle — entropy from CPU pipeline and branch predictor state.

Compression algorithms like zlib have highly data-dependent execution paths.
The time to compress a buffer depends on:
- Branch predictor state (match/no-match decisions)
- Cache state (hash table lookups)
- Pipeline state (speculative execution outcomes)
- Data content (affects number of hash collisions)

By varying the input data and measuring compression time at nanosecond
resolution, we capture micro-architectural state that is physically random.
"""

from __future__ import annotations

import os
import time
import zlib

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class CompressionTimingSource(EntropySource):
    """Entropy from zlib compression timing variations.

    Compresses small buffers with varying content and measures the time.
    The LSBs of timing deltas reflect pipeline and branch predictor state.
    """

    name = "compression_timing"
    description = "Compression timing oracle (zlib pipeline/branch predictor)"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 1800.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 3000) -> np.ndarray:
        timings = np.empty(n_samples, dtype=np.int64)

        for i in range(n_samples):
            # Vary data to exercise different compression paths
            buf = os.urandom(64) + bytes([i & 0xFF] * 64) + os.urandom(64)
            t0 = time.perf_counter_ns()
            _ = zlib.compress(buf, 6)
            timings[i] = time.perf_counter_ns() - t0

        deltas = np.diff(timings)
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(3000), self.name)


class HashTimingSource(EntropySource):
    """Entropy from hash function timing on varying data.

    SHA-256 has data-dependent memory access patterns in its
    message schedule. Timing variations at ns resolution capture
    cache and pipeline state.
    """

    name = "hash_timing"
    description = "SHA-256 timing oracle on varying data"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 2000.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 4000) -> np.ndarray:
        import hashlib

        timings = np.empty(n_samples, dtype=np.int64)
        seed = os.urandom(32)

        for i in range(n_samples):
            data = seed + i.to_bytes(4, "little")
            t0 = time.perf_counter_ns()
            seed = hashlib.sha256(data).digest()
            timings[i] = time.perf_counter_ns() - t0

        deltas = np.diff(timings)
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(4000), self.name)
