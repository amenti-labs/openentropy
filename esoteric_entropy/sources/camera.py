"""Camera photon shot noise entropy source (requires opencv-python)."""

from __future__ import annotations

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class CameraNoiseSource(EntropySource):
    """Entropy from camera sensor dark-current / photon shot noise.

    Image sensor pixels accumulate charge from photon arrivals (Poisson
    process) and thermal dark current.  With the lens covered, the LSBs
    of pixel values are dominated by quantum shot noise.

    Requires ``opencv-python`` optional dependency and a camera.
    """

    name = "camera_shot_noise"
    description = "Camera sensor photon shot noise / dark current"
    category = "hardware"
    physics = (
        "Captures frames from the camera sensor in darkness. The sensor's photodiodes generate dark current from thermal electron-hole pair generation in silicon — a quantum process. Read noise from the amplifier adds further randomness. The LSBs of pixel values in dark frames are dominated by shot noise (Poisson-distributed photon counting)."
    )
    platform_requirements = ["opencv-python", "camera"]
    entropy_rate_estimate = 50000.0

    def is_available(self) -> bool:
        try:
            import cv2  # noqa: F401

            cap = cv2.VideoCapture(0)
            ok = cap.isOpened()
            cap.release()
            return ok
        except Exception:
            return False

    def collect(self, n_samples: int = 10000) -> np.ndarray:
        import cv2

        cap = cv2.VideoCapture(0)
        try:
            ret, frame = cap.read()
            if not ret or frame is None:
                return np.array([], dtype=np.uint8)
            # Flatten and take LSBs
            flat = frame.flatten()
            return (flat & 0x0F).astype(np.uint8)[:n_samples]
        finally:
            cap.release()

    def entropy_quality(self) -> dict:
        data = self.collect(10000)
        return self._quick_quality(data, self.name)
