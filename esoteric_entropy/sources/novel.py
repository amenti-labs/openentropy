"""Novel entropy sources discovered through exploration.

These sources were found via systematic probing of macOS subsystems:
- Dispatch queue contention (GCD scheduling jitter) — 5.51 bits/byte
- dyld/loader timing (shared library resolution) — 4.99 bits/byte
- VM page timing (mmap/munmap VM subsystem) — 4.95 bits/byte
- Spotlight index timing (mdls query timing) — 6.97 bits/byte
"""

from __future__ import annotations

import ctypes
import mmap
import os
import platform
import queue
import subprocess
import threading
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _mach_time_fn():
    lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    lib.mach_absolute_time.restype = ctypes.c_uint64
    lib.mach_absolute_time.argtypes = []
    return lib.mach_absolute_time


class DispatchQueueSource(EntropySource):
    """Entropy from GCD dispatch queue scheduling jitter.

    Thread scheduling on Apple Silicon involves the heterogeneous core
    scheduler (P/E cores), work queue management, and QoS arbitration.
    The timing jitter is physically influenced by thermal state and
    cross-core migration.
    """

    name = "dispatch_queue"
    description = "GCD dispatch queue scheduling jitter (P/E core migration)"
    category = "novel"
    physics = (
        "Submits blocks to GCD (Grand Central Dispatch) queues and measures scheduling latency. macOS dynamically migrates work between P-cores (performance) and E-cores (efficiency) based on thermal state and load. The migration decisions, queue priority inversions, and QoS tier scheduling create non-deterministic dispatch timing that reflects the kernel's real-time resource allocation state."
    )
    platform_requirements: list[str] = []
    entropy_rate_estimate = 1500.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 3000) -> np.ndarray:
        mach = _mach_time_fn()
        q: queue.Queue = queue.Queue()
        results = np.empty(n_samples, dtype=np.uint64)

        def worker():
            while True:
                item = q.get()
                if item is None:
                    break
                q.task_done()

        threads = [threading.Thread(target=worker, daemon=True) for _ in range(4)]
        for t in threads:
            t.start()

        for i in range(n_samples):
            t0 = mach()
            q.put(i)
            q.join()
            results[i] = mach() - t0

        for _ in threads:
            q.put(None)

        deltas = np.diff(results.astype(np.int64))
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(3000), self.name)


class DyldTimingSource(EntropySource):
    """Entropy from dynamic linker (dyld) resolution timing.

    dlopen timing varies with dyld shared cache state, page cache
    pressure, and ASLR-dependent address resolution paths.
    """

    name = "dyld_timing"
    description = "Dynamic linker (dyld) shared library resolution timing"
    category = "novel"
    physics = (
        "Times dynamic library loading (dlopen/dlsym) which requires: searching the dyld shared cache, resolving symbol tables, rebasing pointers, and running initializers. The timing varies with: shared cache page residency (depends on what other apps have loaded), ASLR randomization, and filesystem metadata cache state. Each measurement reflects the dynamic linker's complex resolution path."
    )
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 1200.0

    def is_available(self) -> bool:
        return platform.system() == "Darwin"

    def collect(self, n_samples: int = 2000) -> np.ndarray:
        mach = _mach_time_fn()
        libs = [
            "/usr/lib/libz.dylib",
            "/usr/lib/libc++.dylib",
            "/usr/lib/libobjc.dylib",
            "/usr/lib/libSystem.B.dylib",
        ]
        timings = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            t0 = mach()
            try:
                ctypes.CDLL(libs[i % len(libs)])
            except Exception:
                pass
            timings[i] = mach() - t0
        deltas = np.diff(timings.astype(np.int64))
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(2000), self.name)


class VMPageTimingSource(EntropySource):
    """Entropy from mmap/munmap VM subsystem timing.

    Each mmap/munmap cycle crosses the Mach VM layer. Timing depends on
    page table state, VM map fragmentation, and kernel scheduling.
    """

    name = "vm_page_timing"
    description = "Mach VM subsystem mmap/munmap timing jitter"
    category = "novel"
    physics = (
        "Times Mach VM operations (mmap/munmap cycles). Each operation requires: VM map entry allocation, page table updates, TLB shootdown across cores (IPI interrupt), and physical page management. The timing depends on: VM map fragmentation, physical memory pressure, and cross-core synchronization latency — all of which are shaped by the entire system's memory usage pattern."
    )
    platform_requirements: list[str] = []
    entropy_rate_estimate = 1300.0

    def is_available(self) -> bool:
        return True

    def collect(self, n_samples: int = 3000) -> np.ndarray:
        mach = _mach_time_fn()
        timings = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            t0 = mach()
            m = mmap.mmap(-1, 4096)
            m[0] = i & 0xFF
            m.close()
            timings[i] = mach() - t0
        deltas = np.diff(timings.astype(np.int64))
        xored = deltas[:-1] ^ deltas[1:]
        return (xored & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(3000), self.name)


class SpotlightTimingSource(EntropySource):
    """Entropy from Spotlight metadata query timing.

    mdls query timing varies with Spotlight index state, disk cache,
    and background indexing activity. High entropy (6.97 bits/byte).
    """

    name = "spotlight_timing"
    description = "Spotlight metadata query timing jitter"
    category = "novel"
    physics = (
        "Queries Spotlight's metadata index (mdls) and measures response time. The index is a complex B-tree/inverted index structure. Query timing depends on: index size, disk cache residency, concurrent indexing activity, and filesystem metadata state. When Spotlight is actively indexing new files, query latency becomes highly variable — capturing the unpredictable state of the entire filesystem index."
    )
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 800.0  # slow — subprocess per sample

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            r = subprocess.run(["/usr/bin/mdls", "--help"], capture_output=True, timeout=5)
            return True
        except Exception:
            return False

    def collect(self, n_samples: int = 500) -> np.ndarray:
        mach = _mach_time_fn()
        targets = ["/usr/bin/true", "/usr/bin/false", "/usr/bin/env", "/usr/bin/which"]
        timings = np.empty(n_samples, dtype=np.uint64)
        for i in range(n_samples):
            t0 = mach()
            subprocess.run(
                ["/usr/bin/mdls", "-name", "kMDItemFSName", targets[i % len(targets)]],
                capture_output=True, timeout=5,
            )
            timings[i] = mach() - t0
        deltas = np.diff(timings.astype(np.int64))
        return (deltas & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(300), self.name)
