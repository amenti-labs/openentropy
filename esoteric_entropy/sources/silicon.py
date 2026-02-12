"""Silicon-level entropy: DRAM row buffer timing, cache contention, page faults, speculative execution."""

from __future__ import annotations

import ctypes
import platform
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _mach_time_fn():
    lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    lib.mach_absolute_time.restype = ctypes.c_uint64
    lib.mach_absolute_time.argtypes = []
    return lib.mach_absolute_time


class DRAMRowBufferSource(EntropySource):
    """Entropy from DRAM row buffer hit/miss timing variations.

    Random access across a large buffer forces row buffer misses in the
    memory controller. The timing delta between hits and misses depends on
    DRAM refresh state, thermal conditions, and controller scheduling —
    all physically random at the LSB level.
    """

    name = "dram_row_buffer"
    description = "DRAM row buffer hit/miss timing jitter"
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

    def collect(self, n_samples: int = 4000) -> np.ndarray:
        mach = _mach_time_fn()
        # 32MB buffer exceeds L2/L3, forces DRAM access
        buf = np.zeros(32 * 1024 * 1024 // 8, dtype=np.int64)
        indices = np.random.randint(0, len(buf), size=n_samples)
        timings = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            t0 = mach()
            _ = buf[indices[i]]
            timings[i] = mach() - t0
        del buf
        # XOR consecutive deltas for decorrelation
        deltas = np.diff(timings.astype(np.int64))
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(4000), self.name)


class CacheContentionSource(EntropySource):
    """Entropy from L1/L2 cache line contention patterns.

    By alternating between sequential (cache-friendly) and random
    (cache-hostile) access patterns, we capture the timing difference
    which reflects cache replacement policy state, prefetcher behaviour,
    and micro-architectural noise.
    """

    name = "cache_contention"
    description = "L1/L2 cache miss pattern timing"
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
        # L1 ~128KB, L2 ~4MB on M4. Use 8MB to span L2 boundary.
        buf = np.zeros(8 * 1024 * 1024 // 8, dtype=np.int64)
        rand_idx = np.random.randint(0, len(buf), size=n_samples)

        timings = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            if i & 1:
                # Sequential — should hit cache
                _ = buf[i % len(buf)]
            else:
                # Random — likely cache miss
                _ = buf[rand_idx[i]]
            timings[i] = mach()

        deltas = np.diff(timings.astype(np.int64))
        del buf
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(4000), self.name)


class PageFaultTimingSource(EntropySource):
    """Entropy from page fault and TLB miss timing.

    Allocating fresh memory and touching new pages triggers minor page
    faults. The kernel's page zeroing, TLB refill, and page table walk
    timing vary with system load and memory pressure.
    """

    name = "page_fault_timing"
    description = "TLB miss and page fault timing jitter"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 1500.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 2000) -> np.ndarray:
        page_size = 4096
        timings = np.empty(n_samples, dtype=np.int64)
        for i in range(n_samples):
            # Allocate fresh pages to trigger minor faults
            t0 = time.perf_counter_ns()
            buf = bytearray(page_size * 4)
            buf[0] = 1              # touch first page
            buf[page_size] = 1      # touch second page
            buf[page_size * 2] = 1  # touch third page
            buf[page_size * 3] = 1  # touch fourth page
            timings[i] = time.perf_counter_ns() - t0
            del buf
        return (timings & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(2000), self.name)


class SpeculativeExecutionSource(EntropySource):
    """Entropy from speculative execution and branch predictor state.

    Data-dependent branches with unpredictable outcomes cause the branch
    predictor to mispredict. The resulting pipeline flush timing varies
    with micro-architectural state that is effectively random.
    """

    name = "speculative_execution"
    description = "Branch predictor and speculative execution timing"
    platform_requirements: list[str] = []
    entropy_rate_estimate = 2000.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 4000) -> np.ndarray:
        timings = np.empty(n_samples, dtype=np.int64)
        # Use a random-looking but deterministic sequence for branches
        x = 0x123456789ABCDEF0
        for i in range(n_samples):
            t0 = time.perf_counter_ns()
            # Data-dependent branches that defeat branch prediction
            for _ in range(20):
                x = (x * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
                if x & 0x8000000000000000:
                    x ^= 0xD800000000000000
                else:
                    x = (x << 1) | (x >> 63)
                if (x >> 32) & 1:
                    x += 0x1234
                else:
                    x -= 0x5678
            timings[i] = time.perf_counter_ns() - t0
        return (timings & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(4000), self.name)
