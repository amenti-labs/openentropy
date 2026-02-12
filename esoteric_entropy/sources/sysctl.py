"""Sysctl kernel counter entropy source — the crown jewel.

macOS exposes 1600+ sysctl keys.  Many are kernel counters that change
rapidly (TCP stats, VM page faults, context switches, etc.).  By sampling
the *deltas* of fluctuating keys at high frequency, we harvest entropy
from the unpredictable micro-behaviour of the entire operating system.

Discovery on a Mac Mini M4 found **58 keys** that change within 0.2 s.
"""

from __future__ import annotations

import platform
import subprocess
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource

# Categories for discovered keys
CATEGORIES = {
    "vm": "Virtual memory counters",
    "net": "Network stack counters",
    "kern": "Kernel statistics",
    "debug": "Debug / profiling counters",
    "security": "Security subsystem counters",
    "hw": "Hardware counters",
}


def _sysctl_snapshot() -> dict[str, int]:
    """Batch read ALL sysctl keys as integers using a single `sysctl -a` call."""
    try:
        r = subprocess.run(
            ["/usr/sbin/sysctl", "-a"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (subprocess.TimeoutExpired, OSError):
        return {}

    result: dict[str, int] = {}
    for line in r.stdout.splitlines():
        if ":" not in line:
            continue
        key = line.split(":")[0].strip()
        val = line.split(":", 1)[1].strip()
        try:
            result[key] = int(val)
        except ValueError:
            continue
    return result


def _sysctl_value(key: str) -> int | None:
    """Read a single sysctl key as an integer."""
    try:
        r = subprocess.run(
            ["/usr/sbin/sysctl", "-n", key],
            capture_output=True,
            text=True,
            timeout=2,
        )
        if r.returncode != 0:
            return None
        val = r.stdout.strip()
        if val.startswith("{"):
            nums = [int(x) for x in val.strip("{}").split() if x.lstrip("-").isdigit()]
            return sum(nums) if nums else None
        if ":" in val and not val.replace(":", "").replace(" ", "").replace("-", "").isdigit():
            return None
        return int(val)
    except (ValueError, subprocess.TimeoutExpired, OSError):
        return None


class SysctlSource(EntropySource):
    """Harvest entropy from rapidly-changing sysctl kernel counters.

    On first use, probes all numeric sysctl keys and discovers which ones
    fluctuate.  Subsequent calls sample only the fluctuating keys and
    extract entropy from their deltas.
    """

    name = "sysctl_counters"
    description = "Kernel counter deltas from 50+ fluctuating sysctl keys"
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 5000.0  # very high — many independent counters

    def __init__(self) -> None:
        self._fluctuating_keys: list[str] | None = None

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        # Check sysctl works by reading any numeric key
        return _sysctl_value("kern.osrelease") is not None or _sysctl_value("hw.ncpu") is not None

    def discover_fluctuating_keys(self, probe_delay: float = 0.2) -> list[str]:
        """Probe all numeric sysctl keys; return those that change.

        Uses batch ``sysctl -a`` (2 calls total) instead of per-key probing.
        """
        snap1 = _sysctl_snapshot()
        if not snap1:
            return []

        time.sleep(probe_delay)

        snap2 = _sysctl_snapshot()
        changing = [k for k in snap1 if k in snap2 and snap2[k] != snap1[k]]

        self._fluctuating_keys = sorted(changing)
        return self._fluctuating_keys

    @property
    def fluctuating_keys(self) -> list[str]:
        if self._fluctuating_keys is None:
            self.discover_fluctuating_keys()
        return self._fluctuating_keys  # type: ignore[return-value]

    def categorize_keys(self) -> dict[str, list[str]]:
        """Group fluctuating keys by category prefix."""
        cats: dict[str, list[str]] = {}
        for k in self.fluctuating_keys:
            prefix = k.split(".")[0]
            cats.setdefault(prefix, []).append(k)
        return cats

    def collect(self, n_samples: int = 1000) -> np.ndarray:
        """Sample fluctuating sysctl keys and extract delta LSBs.

        Uses batch ``sysctl -a`` snapshots for speed (~0.1s each vs minutes
        when reading keys individually).
        """
        keys = self.fluctuating_keys
        if not keys:
            return np.array([], dtype=np.uint8)

        key_set = set(keys)
        rounds = max(2, n_samples // max(len(keys), 1) + 1)
        prev: dict[str, int] | None = None
        deltas: list[int] = []

        for _ in range(rounds):
            snap = _sysctl_snapshot()
            if prev is not None:
                for k in keys:
                    if k in snap and k in prev:
                        d = snap[k] - prev[k]
                        if d != 0:
                            deltas.append(d)
            prev = {k: v for k, v in snap.items() if k in key_set}
            if len(deltas) >= n_samples:
                break

        if not deltas:
            return np.array([], dtype=np.uint8)

        arr = np.array(deltas[:n_samples], dtype=np.int64)
        return (arr & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(1000)
        q = self._quick_quality(data, self.name)
        q["fluctuating_keys"] = len(self.fluctuating_keys)
        q["categories"] = self.categorize_keys()
        return q
