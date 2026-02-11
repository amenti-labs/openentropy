"""Process table and CPU fluctuation entropy source."""

from __future__ import annotations

import os
import subprocess
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class ProcessSource(EntropySource):
    """Entropy from process table churn and PID allocation.

    PID assignment, process CPU times, and RSS sizes change
    unpredictably.  We hash the process table and extract jitter
    from rapid ``os.getpid()``-adjacent operations.
    """

    name = "process_table"
    description = "Process table churn, PID allocation, CPU fluctuation"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 400.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 500) -> np.ndarray:
        import hashlib

        samples: list[int] = []
        for _ in range(max(1, n_samples // 10)):
            # Snapshot process table
            t0 = time.perf_counter_ns()
            try:
                r = subprocess.run(
                    ["ps", "-eo", "pid,pcpu,rss"],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                h = hashlib.sha256(r.stdout.encode()).digest()
                samples.extend(h)
            except (OSError, subprocess.TimeoutExpired):
                pass
            # Also capture timing
            samples.append((time.perf_counter_ns() - t0) & 0xFF)

            # Fork/exec jitter
            for _ in range(5):
                t0 = time.perf_counter_ns()
                os.getpid()
                samples.append((time.perf_counter_ns() - t0) & 0xFF)

        arr = np.array(samples[: n_samples], dtype=np.uint8)
        return arr

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(500), self.name)
