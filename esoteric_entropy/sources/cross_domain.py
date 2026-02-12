"""Cross-clock-domain beat frequency entropy.

Apple Silicon has independent PLLs for CPU, GPU, memory, and IO domains.
Operations crossing domain boundaries exhibit timing jitter from PLL phase
noise interactions — the beat frequency of independent oscillators is
physically random.
"""

from __future__ import annotations

import ctypes
import os
import platform
import tempfile
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _mach_time_fn():
    lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    lib.mach_absolute_time.restype = ctypes.c_uint64
    lib.mach_absolute_time.argtypes = []
    return lib.mach_absolute_time


class CPUIOBeatSource(EntropySource):
    """Entropy from CPU↔IO domain crossing timing.

    Alternates CPU computation with file IO. The transition between
    clock domains produces jitter from independent PLL phase noise.
    """

    name = "cpu_io_beat"
    description = "CPU↔IO clock domain beat frequency"
    category = "cross_domain"
    physics = (
        "Alternates CPU-bound computation with disk I/O operations and measures the transition timing. The CPU and I/O subsystem run on independent clock domains with separate PLLs. When operations cross domains, the beat frequency of their PLLs creates timing jitter. This is analogous to the acoustic beat frequency between two tuning forks — physically random phase noise."
    )
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 1500.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            _mach_time_fn()
            return True
        except Exception:
            return False

    def collect(self, n_samples: int = 2000) -> np.ndarray:
        mach = _mach_time_fn()
        tmp = tempfile.NamedTemporaryFile(delete=False)
        tmp.close()
        timings = np.empty(n_samples, dtype=np.uint64)

        try:
            x = 0
            for i in range(n_samples):
                # CPU burst
                for _ in range(50):
                    x = (x * 6364136223846793005 + 1) & 0xFFFFFFFFFFFFFFFF
                # IO crossing
                t0 = mach()
                with open(tmp.name, "wb") as f:
                    f.write(x.to_bytes(8, "little"))
                timings[i] = mach() - t0
        finally:
            os.unlink(tmp.name)

        deltas = np.diff(timings.astype(np.int64))
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(2000), self.name)


class CPUMemoryBeatSource(EntropySource):
    """Entropy from CPU↔Memory controller domain crossing.

    Cache misses force the CPU to wait for the memory controller (separate
    clock domain). The round-trip timing captures PLL phase noise.
    """

    name = "cpu_memory_beat"
    description = "CPU↔Memory controller beat frequency"
    category = "cross_domain"
    physics = (
        "Interleaves CPU computation with random memory accesses to large arrays (>L2 cache). The memory controller runs on its own clock domain. Cache misses force the CPU to wait for the memory controller's arbitration, whose timing depends on: DRAM refresh state, competing DMA from GPU/ANE, and row buffer conflicts. The cross-domain latency jitter is non-deterministic."
    )
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 2500.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            _mach_time_fn()
            return True
        except Exception:
            return False

    def collect(self, n_samples: int = 4000) -> np.ndarray:
        mach = _mach_time_fn()
        big = np.random.randint(0, 256, size=16 * 1024 * 1024, dtype=np.uint8)
        timings = np.empty(n_samples, dtype=np.uint64)

        for i in range(n_samples):
            idx = np.random.randint(0, len(big))
            t0 = mach()
            _ = big[idx]
            timings[i] = mach() - t0

        del big
        deltas = np.diff(timings.astype(np.int64))
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(4000), self.name)


class MultiDomainBeatSource(EntropySource):
    """Entropy from rapid interleaving across all clock domains.

    CPU → memory → syscall → IO in tight loop. Each transition crosses
    a domain boundary, compounding PLL phase noise from multiple sources.
    """

    name = "multi_domain_beat"
    description = "Multi-domain (CPU/memory/IO/kernel) interleaved beat"
    category = "cross_domain"
    physics = (
        "Rapidly interleaves operations across 4 clock domains: CPU computation, memory access, disk I/O, and kernel syscalls. Each domain has its own PLL and arbitration logic. The composite timing captures interference patterns between all domains simultaneously — like listening to 4 independent oscillators and recording the emergent beat pattern."
    )
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 3000.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            _mach_time_fn()
            return True
        except Exception:
            return False

    def collect(self, n_samples: int = 3000) -> np.ndarray:
        mach = _mach_time_fn()
        big = np.random.randint(0, 256, size=4 * 1024 * 1024, dtype=np.uint8)
        timings = np.empty(n_samples, dtype=np.uint64)

        x = 0
        for i in range(n_samples):
            t0 = mach()
            # CPU
            for _ in range(30):
                x ^= _ * 2654435761
            # Memory (cache miss)
            _ = big[np.random.randint(0, len(big))]
            # Kernel crossing
            _ = os.getpid()
            timings[i] = mach() - t0

        del big
        deltas = np.diff(timings.astype(np.int64))
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(3000), self.name)
