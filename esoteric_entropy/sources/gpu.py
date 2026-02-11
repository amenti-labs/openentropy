"""GPU compute timing entropy source."""

from __future__ import annotations

import platform
import subprocess
import tempfile
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class GPUTimingSource(EntropySource):
    """Entropy from GPU-accelerated operation timing jitter.

    GPU shader execution is non-deterministic due to thermal throttling,
    memory controller arbitration, and warp/SIMD scheduling.  We time
    GPU-involved image operations via ``sips`` on macOS.
    """

    name = "gpu_timing"
    description = "GPU dispatch completion timing jitter"
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 300.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            r = subprocess.run(
                ["/usr/bin/sips", "--help"],
                capture_output=True,
                timeout=5,
            )
            return r.returncode in (0, 1)  # sips --help returns 1
        except (OSError, subprocess.TimeoutExpired):
            return False

    def collect(self, n_samples: int = 100) -> np.ndarray:
        # Find a system image to use as input
        src = "/System/Library/Desktop Pictures/Solid Colors/Black.png"
        if not __import__("os").path.exists(src):
            # Fallback: create a tiny PNG
            src = tempfile.NamedTemporaryFile(suffix=".png", delete=False).name
            subprocess.run(
                ["/usr/bin/sips", "-z", "1", "1", "-s", "format", "png",
                 "/dev/null", "--out", src],
                capture_output=True, timeout=5,
            )

        out = tempfile.NamedTemporaryFile(suffix=".png", delete=False).name
        timings = np.empty(n_samples, dtype=np.int64)
        for i in range(n_samples):
            t0 = time.perf_counter_ns()
            subprocess.run(
                ["/usr/bin/sips", "-z", "64", "64", src, "--out", out],
                capture_output=True,
                timeout=10,
            )
            timings[i] = time.perf_counter_ns() - t0

        try:
            __import__("os").unlink(out)
        except OSError:
            pass

        return (timings & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(50), self.name)
