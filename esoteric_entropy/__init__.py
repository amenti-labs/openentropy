"""
esoteric-entropy: Your computer is a quantum noise observatory.

Harvests entropy from unconventional hardware sources — clock jitter,
kernel counters, memory timing, GPU scheduling, network latency, and more.
"""

__version__ = "0.2.0"
__author__ = "Amenti Labs"

# Try to use the Rust backend (via PyO3) if available, fall back to pure Python.
try:
    from esoteric_entropy.esoteric_entropy import (
        EntropyPool,
        detect_available_sources,
        run_all_tests,
        calculate_quality_score,
        version as _rust_version,
    )
    __rust_backend__ = True
    __version__ = _rust_version()
except ImportError:
    from esoteric_entropy.pool import EntropyPool
    __rust_backend__ = False

from esoteric_entropy.sources.base import EntropySource

__all__ = [
    "EntropyPool",
    "EntropySource",
    "EsotericBitGenerator",
    "EsotericRandom",
    "__version__",
    "__rust_backend__",
]

if __rust_backend__:
    __all__ += ["detect_available_sources", "run_all_tests", "calculate_quality_score"]


def EsotericBitGenerator(**kwargs):
    """Lazy import to avoid heavy deps at module level."""
    from esoteric_entropy.numpy_compat import EsotericBitGenerator as _EBG
    return _EBG(**kwargs)


def EsotericRandom(**kwargs):
    """Create a Generator-compatible RNG backed by esoteric entropy.

    Example::

        from esoteric_entropy import EsotericRandom
        rng = EsotericRandom()
        rng.random(10)       # 10 floats from hardware entropy
        rng.integers(0, 100) # random int
    """
    from esoteric_entropy.numpy_compat import EsotericGenerator
    return EsotericGenerator(**kwargs)
