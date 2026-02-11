"""DRAM access timing entropy source."""

from __future__ import annotations

import mmap
import os
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class MemoryTimingSource(EntropySource):
    """Entropy from memory allocation and access timing jitter.

    Timing varies due to DRAM refresh cycles (~64 ms), cache misses,
    TLB misses, memory controller scheduling, row-buffer hits/misses,
    and thermal effects on DRAM timing margins.
    """

    name = "memory_timing"
    description = "DRAM allocation and access timing jitter"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 1500.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        page_size = os.sysconf("SC_PAGE_SIZE") if hasattr(os, "sysconf") else 4096
        timings = np.empty(n_samples, dtype=np.int64)
        for i in range(n_samples):
            t0 = time.perf_counter_ns()
            mm = mmap.mmap(-1, page_size)
            mm[0] = 42
            mm.close()
            timings[i] = time.perf_counter_ns() - t0
        return (timings & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(1000), self.name)
