"""Disk I/O timing entropy source."""

from __future__ import annotations

import os
import tempfile
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class DiskIOSource(EntropySource):
    """Entropy from NVMe/SSD I/O latency jitter.

    Flash read latency varies due to cell voltage margins, wear leveling
    decisions, garbage collection, controller queue state, and thermal
    effects on NAND.
    """

    name = "disk_io"
    description = "NVMe/SSD read latency jitter"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 800.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        # Create a temp file to read from
        tmp = tempfile.NamedTemporaryFile(delete=False)
        try:
            tmp.write(os.urandom(1024 * 64))  # 64 KB
            tmp.flush()
            os.fsync(tmp.fileno())
            tmp.close()

            timings = np.empty(n_samples, dtype=np.int64)
            for i in range(n_samples):
                t0 = time.perf_counter_ns()
                with open(tmp.name, "rb") as f:
                    f.seek(int.from_bytes(os.urandom(2), "big") % (1024 * 60))
                    f.read(4096)
                timings[i] = time.perf_counter_ns() - t0
        finally:
            os.unlink(tmp.name)

        return (timings & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(1000), self.name)
