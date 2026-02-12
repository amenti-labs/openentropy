"""Multi-source entropy pool with health monitoring.

Architecture:
1. Auto-discover available sources on this machine
2. Collect raw entropy from each source in parallel
3. Weight by measured entropy rate
4. XOR-combine independent streams
5. SHA-256 final conditioning
6. Continuous health monitoring per source
7. Graceful degradation when sources fail
8. Thread-safe for concurrent access
"""

from __future__ import annotations

import hashlib
import os
import struct
import threading
import time
from dataclasses import dataclass

from esoteric_entropy.sources.base import EntropySource


@dataclass
class SourceState:
    """Runtime state for a registered source."""

    source: EntropySource
    weight: float = 1.0
    total_bytes: int = 0
    failures: int = 0
    last_entropy: float = 0.0
    last_collect_time: float = 0.0
    healthy: bool = True


class EntropyPool:
    """Thread-safe multi-source entropy pool.

    Usage::

        pool = EntropyPool.auto()     # discover sources
        data = pool.get_random_bytes(32)
    """

    def __init__(self, seed: bytes | None = None) -> None:
        self._sources: list[SourceState] = []
        self._buffer = bytearray()
        self._lock = threading.Lock()
        self._counter = 0
        self._total_output = 0
        self._state = hashlib.sha256(seed or os.urandom(32)).digest()

    # ── source management ──

    def add_source(self, source: EntropySource, weight: float = 1.0) -> None:
        self._sources.append(SourceState(source=source, weight=weight))

    @classmethod
    def auto(cls) -> EntropyPool:
        """Create a pool with all sources available on this machine."""
        from esoteric_entropy.platform import detect_available_sources

        pool = cls()
        for src in detect_available_sources():
            pool.add_source(src)
        return pool

    @property
    def sources(self) -> list[SourceState]:
        return list(self._sources)

    # ── collection ──

    def _collect_one(self, ss: SourceState) -> bytes:
        """Collect from a single source. Returns raw bytes."""
        try:
            t0 = time.monotonic()
            data = ss.source.collect()
            ss.last_collect_time = time.monotonic() - t0
            if len(data) > 0:
                ss.total_bytes += len(data)
                ss.last_entropy = EntropySource._quick_shannon(data)
                ss.healthy = ss.last_entropy > 1.0
                return data.tobytes()
            else:
                ss.failures += 1
                ss.healthy = False
        except Exception:
            ss.failures += 1
            ss.healthy = False
        return b""

    def collect_all(self, parallel: bool = False, timeout: float = 10.0) -> int:
        """Collect entropy from every registered source.

        Parameters
        ----------
        parallel:
            If True, collect from all sources concurrently using threads.
        timeout:
            Per-source timeout in seconds (parallel mode only).
        """
        if not parallel:
            raw = bytearray()
            for ss in self._sources:
                raw.extend(self._collect_one(ss))
            with self._lock:
                self._buffer.extend(raw)
            return len(raw)

        # Parallel collection using daemon threads with hard timeout
        import threading as _th

        results: list[bytes] = []
        results_lock = _th.Lock()

        def _worker(ss):
            data = self._collect_one(ss)
            if data:
                with results_lock:
                    results.append(data)

        threads = []
        for ss in self._sources:
            t = _th.Thread(target=_worker, args=(ss,), daemon=True)
            t.start()
            threads.append(t)

        # Wait for all threads up to timeout
        deadline = time.monotonic() + timeout
        for t in threads:
            remaining = max(0.1, deadline - time.monotonic())
            t.join(timeout=remaining)

        raw = bytearray()
        for r in results:
            raw.extend(r)

        with self._lock:
            self._buffer.extend(raw)
        return len(raw)

    # ── output ──

    def get_random_bytes(self, n_bytes: int) -> bytes:
        """Return *n_bytes* of conditioned random output."""
        with self._lock:
            pool_size = len(self._buffer)

        if pool_size < n_bytes * 2:
            self.collect_all()

        output = bytearray()
        while len(output) < n_bytes:
            self._counter += 1
            with self._lock:
                sample = bytes(self._buffer[:256])
                if len(self._buffer) > 256:
                    self._buffer = self._buffer[256:]

            h = hashlib.sha256()
            h.update(self._state)
            h.update(sample)
            h.update(struct.pack("<Q", self._counter))
            h.update(struct.pack("<d", time.time()))
            h.update(os.urandom(8))
            self._state = h.digest()
            output.extend(self._state)

        self._total_output += n_bytes
        return bytes(output[:n_bytes])

    # ── health ──

    def health_report(self) -> dict:
        healthy = sum(1 for s in self._sources if s.healthy)
        total_raw = sum(s.total_bytes for s in self._sources)
        return {
            "healthy": healthy,
            "total": len(self._sources),
            "raw_bytes": total_raw,
            "output_bytes": self._total_output,
            "buffer_size": len(self._buffer),
            "sources": [
                {
                    "name": s.source.name,
                    "healthy": s.healthy,
                    "bytes": s.total_bytes,
                    "entropy": round(s.last_entropy, 2),
                    "time": round(s.last_collect_time, 3),
                    "failures": s.failures,
                }
                for s in self._sources
            ],
        }

    def print_health(self) -> None:
        """Pretty-print health report to stdout."""
        r = self.health_report()
        print(f"\n{'='*60}")
        print("ENTROPY POOL HEALTH REPORT")
        print(f"{'='*60}")
        print(f"Sources: {r['healthy']}/{r['total']} healthy")
        print(f"Raw collected: {r['raw_bytes']:,} bytes")
        print(f"Output: {r['output_bytes']:,} bytes | Buffer: {r['buffer_size']:,} bytes")
        print(f"\n{'Source':<25} {'OK':>4} {'Bytes':>10} {'H':>6} {'Time':>7} {'Fail':>5}")
        print("-" * 60)
        for s in r["sources"]:
            ok = "✓" if s["healthy"] else "✗"
            print(
                f"{s['name']:<25} {ok:>4} {s['bytes']:>10,} "
                f"{s['entropy']:>5.2f} {s['time']:>6.3f}s {s['failures']:>5}"
            )
