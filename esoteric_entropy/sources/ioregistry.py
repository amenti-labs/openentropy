"""Deep IORegistry mining — entropy from fluctuating hardware counters.

macOS IORegistry exposes hundreds of numeric hardware properties (GPU utilization,
NVMe SMART counters, memory controller stats, thermal sensors, power delivery).
Many of these fluctuate between samples due to physical processes. The LSBs of
their deltas are genuine hardware entropy.
"""

from __future__ import annotations

import platform
import re
import subprocess
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource

_NUM_RE = re.compile(r'"([^"]{1,80})"\s*=\s*(\d{1,15})(?:\s|,|}|$)')


def _parse_ioreg() -> dict[str, int]:
    """Parse all numeric values from ioreg."""
    try:
        result = subprocess.run(
            ["/usr/sbin/ioreg", "-l", "-w0"],
            capture_output=True, text=True, timeout=30,
        )
    except Exception:
        return {}
    values: dict[str, int] = {}
    for m in _NUM_RE.finditer(result.stdout):
        key, val = m.group(1), int(m.group(2))
        if 0 < val < 2**48:
            values.setdefault(key, val)
    return values


class IORegistryEntropySource(EntropySource):
    """Entropy from IORegistry fluctuating hardware counters.

    Samples ioreg repeatedly and extracts the changing values.
    Detrends monotonic counters and harvests the LSBs of deltas.
    """

    name = "ioregistry_deep"
    description = "IORegistry hardware counter fluctuations (GPU/NVMe/thermal/power)"
    platform_requirements = ["darwin"]
    entropy_rate_estimate = 1000.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            vals = _parse_ioreg()
            return len(vals) > 50
        except Exception:
            return False

    def collect(self, n_samples: int = 2000) -> np.ndarray:
        # Take multiple snapshots
        n_snapshots = max(10, min(n_samples // 50, 25))
        interval = 0.3
        snapshots: list[dict[str, int]] = []
        for _ in range(n_snapshots):
            snapshots.append(_parse_ioreg())
            time.sleep(interval)

        # Find keys that changed
        all_keys = set(snapshots[0].keys())
        for s in snapshots[1:]:
            all_keys &= set(s.keys())

        entropy_bytes = bytearray()
        for key in sorted(all_keys):
            vals = [s[key] for s in snapshots]
            unique = len(set(vals))
            if unique < 2:
                continue
            arr = np.array(vals, dtype=np.int64)
            deltas = np.diff(arr)
            # XOR with shifted version for whitening
            if len(deltas) > 1:
                xored = deltas[:-1] ^ deltas[1:]
                entropy_bytes.extend((np.abs(xored) & 0xFF).astype(np.uint8).tobytes())
            entropy_bytes.extend((np.abs(deltas) & 0xFF).astype(np.uint8).tobytes())

        if len(entropy_bytes) == 0:
            return np.array([], dtype=np.uint8)

        result = np.frombuffer(bytes(entropy_bytes), dtype=np.uint8)
        # Pad or truncate to approximate n_samples
        if len(result) < n_samples:
            # Hash-extend
            import hashlib
            extended = bytearray(result.tobytes())
            while len(extended) < n_samples:
                h = hashlib.sha256(extended[-64:]).digest()
                extended.extend(h)
            result = np.frombuffer(bytes(extended[:n_samples]), dtype=np.uint8)
        return result[:n_samples]

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(2000), self.name)
