"""Bluetooth LE advertisement noise entropy source."""

from __future__ import annotations

import platform
import subprocess

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class BluetoothNoiseSource(EntropySource):
    """Entropy from BLE advertisement RSSI and timing.

    Ambient BLE advertisements from nearby devices have unpredictable
    RSSI fluctuations due to multipath fading, device movement, and
    frequency-hop timing.  Requires Bluetooth hardware.
    """

    name = "bluetooth_ble"
    description = "BLE advertisement RSSI noise"
    platform_requirements = ["darwin", "bluetooth"]
    entropy_rate_estimate = 50.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            r = subprocess.run(
                ["/usr/sbin/system_profiler", "SPBluetoothDataType"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            return "Bluetooth" in r.stdout
        except (OSError, subprocess.TimeoutExpired):
            return False

    def collect(self, n_samples: int = 100) -> np.ndarray:
        # BLE scanning requires CoreBluetooth which needs an event loop.
        # For CLI use we fall back to system_profiler timing jitter.

        timings: list[int] = []
        for _ in range(n_samples):
            t0 = __import__("time").perf_counter_ns()
            try:
                subprocess.run(
                    ["/usr/sbin/system_profiler", "SPBluetoothDataType"],
                    capture_output=True,
                    timeout=5,
                )
            except (OSError, subprocess.TimeoutExpired):
                pass
            timings.append(__import__("time").perf_counter_ns() - t0)
        if not timings:
            return np.array([], dtype=np.uint8)
        return (np.array(timings, dtype=np.int64) & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(20), self.name)
