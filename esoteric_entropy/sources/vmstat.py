"""VM statistics entropy source.

``vm_stat`` on macOS reports page faults, pageins, pageouts, swapins,
and other counters that change rapidly under any workload.  The deltas
between successive reads are unpredictable at fine granularity.
"""

from __future__ import annotations

import platform
import subprocess

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _parse_vmstat() -> dict[str, int]:
    """Run vm_stat and parse output into a counter dict."""
    try:
        r = subprocess.run(
            ["vm_stat"], capture_output=True, text=True, timeout=5
        )
    except (OSError, subprocess.TimeoutExpired):
        return {}
    counters: dict[str, int] = {}
    for line in r.stdout.splitlines():
        if ":" not in line:
            continue
        key, _, val = line.partition(":")
        val = val.strip().rstrip(".")
        try:
            counters[key.strip()] = int(val)
        except ValueError:
            continue
    return counters


class VmstatSource(EntropySource):
    """Entropy from VM page-fault and memory-pressure counter deltas."""

    name = "vmstat"
    description = "VM statistics counter deltas (page faults, swaps, etc.)"
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 1000.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        return bool(_parse_vmstat())

    def collect(self, n_samples: int = 500) -> np.ndarray:
        rounds = max(2, n_samples // 19 + 1)  # ~19 counters per read
        prev = _parse_vmstat()
        deltas: list[int] = []
        for _ in range(rounds):
            curr = _parse_vmstat()
            for k in prev:
                if k in curr:
                    d = curr[k] - prev[k]
                    if d != 0:
                        deltas.append(d)
            prev = curr
        if not deltas:
            return np.array([], dtype=np.uint8)
        return (np.array(deltas, dtype=np.int64) & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(500), self.name)
