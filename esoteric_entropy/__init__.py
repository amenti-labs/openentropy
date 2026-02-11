"""
esoteric-entropy: Your computer is a quantum noise observatory.

Harvests entropy from unconventional hardware sources — clock jitter,
kernel counters, memory timing, GPU scheduling, network latency, and more.
"""

__version__ = "0.1.0"
__author__ = "Esoteric Entropy Contributors"

from esoteric_entropy.pool import EntropyPool
from esoteric_entropy.sources.base import EntropySource

__all__ = ["EntropyPool", "EntropySource", "__version__"]
