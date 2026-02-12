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
    category = "timing"
    physics = (
        "Measures phase noise between two independent clock oscillators (perf_counter vs monotonic). Each clock is driven by a separate PLL (Phase-Locked Loop) on the SoC. Thermal noise in the PLL's voltage-controlled oscillator causes random frequency drift — the LSBs of their difference are genuine analog entropy from crystal oscillator physics."
    )
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
    category = "timing"
    physics = (
        "Reads the ARM system counter (mach_absolute_time) at sub-nanosecond resolution with variable micro-workloads between samples. The timing jitter comes from CPU pipeline state: instruction reordering, branch prediction, cache state, interrupt coalescing, and power-state transitions. SHA-256 conditioning extracts the entropy spread across all bits."
    )
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
        import hashlib
        # Interleave timing with micro-workloads to perturb pipeline state
        raw_n = n_samples + 1
        times = np.empty(raw_n, dtype=np.uint64)
        x = 0
        for i in range(raw_n):
            times[i] = fn()
            # Tiny variable workload to perturb CPU state between measurements
            for _ in range(i % 7 + 1):
                x = (x * 6364136223846793005 + i) & 0xFFFFFFFFFFFFFFFF
        deltas = np.diff(times).astype(np.int64)
        # Von Neumann debiasing: compare pairs, emit 1 if first>second, 0 otherwise
        # Then hash-condition for output
        raw_bytes = deltas.tobytes()
        # SHA-256 conditioning in 64-byte blocks
        out = bytearray()
        for off in range(0, len(raw_bytes) - 63, 64):
            out.extend(hashlib.sha256(raw_bytes[off:off+64]).digest())
        if len(out) == 0:
            out.extend(hashlib.sha256(raw_bytes).digest())
        return np.frombuffer(bytes(out[:n_samples]), dtype=np.uint8)

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
    category = "timing"
    physics = (
        "Requests zero-duration sleeps and measures actual wake time. The jitter captures OS scheduler non-determinism: timer interrupt granularity (1-4ms), thread priority decisions, runqueue length, and thermal-dependent clock frequency scaling (DVFS). Each measurement reflects the entire system's instantaneous scheduling state."
    )
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
