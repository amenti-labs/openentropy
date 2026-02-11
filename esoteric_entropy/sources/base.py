"""Abstract base class for all entropy sources."""

from abc import ABC, abstractmethod

import numpy as np


class EntropySource(ABC):
    """Base class for an esoteric entropy source.

    Every source must declare metadata and implement three methods:
    ``is_available``, ``collect``, and ``entropy_quality``.
    """

    name: str = "unnamed"
    description: str = ""
    platform_requirements: list[str] = []
    entropy_rate_estimate: float = 0.0  # bits per second

    @abstractmethod
    def is_available(self) -> bool:
        """Return True if the source can operate on this machine."""
        ...

    @abstractmethod
    def collect(self, n_samples: int = 1000) -> np.ndarray:
        """Collect raw entropy samples.

        Parameters
        ----------
        n_samples:
            Requested number of byte-valued samples.

        Returns
        -------
        numpy.ndarray
            1-D uint8 array of collected samples. May be shorter
            than *n_samples* if the source cannot provide enough.
        """
        ...

    @abstractmethod
    def entropy_quality(self) -> dict:
        """Collect a sample and run basic quality checks.

        Returns a dict with at least ``shannon_entropy``,
        ``compression_ratio``, and ``grade`` keys.
        """
        ...

    # ── helpers available to subclasses ──

    @staticmethod
    def _quick_shannon(data: np.ndarray) -> float:
        """Fast Shannon entropy in bits/byte for uint8 data."""
        if len(data) == 0:
            return 0.0
        _, counts = np.unique(data, return_counts=True)
        probs = counts / len(data)
        return float(-np.sum(probs * np.log2(probs + 1e-15)))

    @staticmethod
    def _quick_quality(data: np.ndarray, label: str = "") -> dict:
        """Run lightweight quality metrics on uint8 data."""
        import zlib

        if len(data) < 16:
            return {"grade": "F", "error": "insufficient data", "samples": len(data)}

        raw = data.astype(np.uint8).tobytes()
        shannon = EntropySource._quick_shannon(data)
        comp_ratio = len(zlib.compress(raw, 9)) / max(len(raw), 1)
        n_unique = int(len(np.unique(data)))

        eff = shannon / 8.0
        score = eff * 60 + min(comp_ratio, 1.0) * 20 + min(n_unique / 256, 1.0) * 20
        grade = (
            "A" if score >= 80 else
            "B" if score >= 60 else
            "C" if score >= 40 else
            "D" if score >= 20 else "F"
        )
        return {
            "label": label,
            "samples": len(data),
            "unique_values": n_unique,
            "shannon_entropy": round(shannon, 4),
            "compression_ratio": round(comp_ratio, 4),
            "quality_score": round(score, 1),
            "grade": grade,
        }

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__} name={self.name!r}>"
