"""Statistical test suite for entropy characterisation."""

from __future__ import annotations

import zlib
from collections import Counter
from math import factorial, log2

import numpy as np


def shannon_entropy(data: np.ndarray) -> float:
    """Shannon entropy in bits for uint8 data."""
    data = np.asarray(data).flatten()
    counts = np.array(list(Counter(data).values()))
    probs = counts / len(data)
    return float(-np.sum(probs * np.log2(probs + 1e-15)))


def min_entropy(data: np.ndarray) -> float:
    """Min-entropy (NIST SP 800-90B) — most conservative estimate."""
    data = np.asarray(data).flatten()
    p_max = max(Counter(data).values()) / len(data)
    return float(-np.log2(p_max + 1e-15))


def compression_ratio(data: bytes | np.ndarray) -> float:
    """Compression ratio via zlib level 9.  ≈1.0 means incompressible."""
    if isinstance(data, np.ndarray):
        data = data.astype(np.uint8).tobytes()
    if len(data) < 10:
        return 0.0
    return len(zlib.compress(data, 9)) / len(data)


def chi_squared_uniformity(data: np.ndarray) -> dict:
    """Chi-squared uniformity test for byte distribution."""
    data = np.asarray(data).flatten()
    hist = np.bincount(data.astype(np.uint8), minlength=256)
    expected = len(data) / 256
    chi2 = float(np.sum((hist - expected) ** 2 / max(expected, 1e-15)))
    return {"chi2": chi2, "uniform": chi2 < 293}  # p=0.05 for 255 df


def serial_correlation(data: np.ndarray, lag: int = 1) -> float:
    """Serial autocorrelation at given lag."""
    data = np.asarray(data, dtype=float).flatten()
    if len(data) < lag + 2:
        return 0.0
    mean = np.mean(data)
    var = np.var(data)
    if var < 1e-15:
        return 0.0
    return float(np.mean((data[:-lag] - mean) * (data[lag:] - mean)) / var)


def permutation_entropy(data: np.ndarray, order: int = 3) -> float:
    """Normalised permutation entropy (1.0 = maximally complex)."""
    data = np.asarray(data, dtype=float).flatten()
    n = len(data)
    if n < order + 1:
        return 0.0
    patterns: list[tuple[int, ...]] = []
    for i in range(n - order):
        w = tuple(data[i: i + order])
        patterns.append(tuple(sorted(range(order), key=lambda k: (w[k], k))))
    counts = Counter(patterns)
    total = len(patterns)
    pe = sum(-((c / total) * log2(c / total)) for c in counts.values())
    return pe / log2(factorial(order))


def full_report(data: np.ndarray, label: str = "") -> dict:
    """Run all tests and return a structured report."""
    data = np.asarray(data).flatten()
    raw = data.astype(np.uint8).tobytes()

    sh = shannon_entropy(data)
    me = min_entropy(data)
    cr = compression_ratio(raw)
    chi = chi_squared_uniformity(data)
    sc = serial_correlation(data)
    pe = permutation_entropy(data)

    # Score
    eff = sh / 8.0
    score = eff * 40 + min(cr, 1.0) * 20 + (20 if chi["uniform"] else 0) + pe * 20
    grade = (
        "A" if score >= 80 else
        "B" if score >= 60 else
        "C" if score >= 40 else
        "D" if score >= 20 else "F"
    )

    return {
        "label": label,
        "samples": len(data),
        "unique_values": int(len(np.unique(data))),
        "shannon_entropy": round(sh, 4),
        "min_entropy": round(me, 4),
        "compression_ratio": round(cr, 4),
        "chi_squared": chi,
        "serial_correlation": round(sc, 6),
        "permutation_entropy": round(pe, 4),
        "quality_score": round(score, 1),
        "grade": grade,
    }
