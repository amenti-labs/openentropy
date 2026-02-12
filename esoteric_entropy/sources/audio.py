"""Audio thermal noise entropy source (requires sounddevice)."""

from __future__ import annotations

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class AudioNoiseSource(EntropySource):
    """Entropy from microphone thermal (Johnson-Nyquist) noise.

    With no signal present the ADC still digitises thermal agitation
    of electrons in the input impedance — genuine quantum-origin noise.
    Requires the ``sounddevice`` optional dependency and a microphone.
    """

    name = "audio_thermal"
    description = "Microphone ADC thermal noise (Johnson-Nyquist)"
    platform_requirements = ["sounddevice", "microphone"]
    entropy_rate_estimate = 10000.0

    def is_available(self) -> bool:
        try:
            import sounddevice as sd  # noqa: F401

            devs = sd.query_devices()
            # Check for actual input devices (not just virtual/aggregate)
            has_input = any(d.get("max_input_channels", 0) > 0 for d in devs)  # type: ignore[union-attr]
            if not has_input:
                return False
            # Quick probe with timeout — catches devices that list but hang on record
            import threading

            result = [False]

            def _probe():
                try:
                    sd.rec(int(44100 * 0.01), samplerate=44100, channels=1, dtype="int16", blocking=True)
                    result[0] = True
                except Exception:
                    pass

            t = threading.Thread(target=_probe, daemon=True)
            t.start()
            t.join(timeout=3.0)
            return result[0]
        except Exception:
            return False

    def collect(self, n_samples: int = 4096) -> np.ndarray:
        import sounddevice as sd

        duration = max(0.05, n_samples / 44100)
        audio = sd.rec(
            int(44100 * duration),
            samplerate=44100,
            channels=1,
            dtype="int16",
            blocking=True,
        )
        raw = audio.flatten()
        # Take LSBs — dominated by thermal noise
        return (raw & 0xFF).astype(np.uint8)[:n_samples]

    def entropy_quality(self) -> dict:
        data = self.collect(4096)
        return self._quick_quality(data, self.name)
