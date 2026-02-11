"""Clock jitter, mach timing, and sleep jitter entropy sources."""

from __future__ import annotations

import platform
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class ClockJitterSource(EntropySource):
    """Entropy from differences between independent clock domains.

    ``time.perf_counter_ns()`` and ``time.monotonic_ns()`` may be driven
    by different oscillators.  Their difference drifts unpredictably due
    to PLL phase noise — the LSBs are genuine entropy.
    """

    name = "clock_jitter"
    description = "Phase noise between perf_counter and monotonic clocks"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 500.0  # bits/s (conservative)

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 2000) -> np.ndarray:
        diffs = np.empty(n_samples, dtype=np.int64)
        for i in range(n_samples):
            diffs[i] = time.perf_counter_ns() - time.monotonic_ns()
        # Extract LSBs
        return (diffs & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(2000)
        return self._quick_quality(data, self.name)


class MachTimingSource(EntropySource):
    """Entropy from Mach absolute time LSBs (macOS only).

    ``mach_absolute_time()`` reads the ARM system counter at sub-ns
    resolution.  The LSBs of successive deltas contain jitter from
    interrupt coalescing, power-state transitions, and speculative
    execution pipeline state.
    """

    name = "mach_timing"
    description = "Mach kernel absolute-time LSB jitter"
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 2000.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            self._get_mach_fn()
            return True
        except Exception:
            return False

    @staticmethod
    def _get_mach_fn():
        import ctypes

        lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
        lib.mach_absolute_time.restype = ctypes.c_uint64
        lib.mach_absolute_time.argtypes = []
        return lib.mach_absolute_time

    def collect(self, n_samples: int = 5000) -> np.ndarray:
        fn = self._get_mach_fn()
        times = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            times[i] = fn()
        deltas = np.diff(times)
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(5000)
        return self._quick_quality(data, self.name)


class SleepJitterSource(EntropySource):
    """Entropy from sleep/wake timing inaccuracy.

    Requesting a very short sleep and measuring actual elapsed time
    captures OS scheduling jitter, timer interrupt granularity, and
    thermal-dependent clock drift.
    """

    name = "sleep_jitter"
    description = "OS scheduling jitter from short sleeps"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 200.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 500) -> np.ndarray:
        jitters = np.empty(n_samples, dtype=np.int64)
        for i in range(n_samples):
            t0 = time.perf_counter_ns()
            time.sleep(0)  # yield to scheduler
            jitters[i] = time.perf_counter_ns() - t0
        return (jitters & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(500)
        return self._quick_quality(data, self.name)
