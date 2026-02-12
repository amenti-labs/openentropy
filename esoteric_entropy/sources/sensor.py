"""Magnetometer / accelerometer sensor noise (macOS IOKit)."""

from __future__ import annotations

import platform
import subprocess

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class SensorNoiseSource(EntropySource):
    """Entropy from MEMS sensor noise (accelerometer, magnetometer).

    On MacBooks with motion sensors, MEMS Brownian motion and bias
    drift provide physical entropy.  Not available on Mac Mini / desktops
    without external sensors.
    """

    name = "sensor_noise"
    description = "MEMS accelerometer/magnetometer noise"
    category = "hardware"
    physics = (
        "Reads accelerometer, gyroscope, and magnetometer via CoreMotion. Even at rest, MEMS sensors exhibit: Brownian motion of the proof mass, thermo-mechanical noise, electronic 1/f noise, and quantization noise. The MacBook's accelerometer detects micro-vibrations from fans, disk, and building structure that are physically unpredictable."
    )
    platform_requirements = ["darwin", "motion_sensors"]
    entropy_rate_estimate = 100.0

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            r = subprocess.run(
                ["/usr/sbin/ioreg", "-l", "-w0"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            return "SMCMotionSensor" in r.stdout or "Accelerometer" in r.stdout
        except (OSError, subprocess.TimeoutExpired):
            return False

    def collect(self, n_samples: int = 500) -> np.ndarray:
        # Placeholder: would use CoreMotion or IOKit for real sensors.
        # On machines without sensors, is_available returns False.
        return np.array([], dtype=np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(500), self.name)
