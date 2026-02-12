"""
esoteric-entropy: Your computer is a quantum noise observatory.

Harvests entropy from unconventional hardware sources — clock jitter,
kernel counters, memory timing, GPU scheduling, network latency, and more.
"""

__version__ = "0.2.0"
__author__ = "Amenti Labs"

from esoteric_entropy.pool import EntropyPool
from esoteric_entropy.sources.base import EntropySource

__all__ = ["EntropyPool", "EntropySource", "EsotericBitGenerator", "EsotericRandom", "__version__"]


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
