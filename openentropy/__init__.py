"""
openentropy: Your computer is a quantum noise observatory.

Harvests entropy from unconventional hardware sources — clock jitter,
kernel counters, memory timing, GPU scheduling, network latency, and more.

This package requires the compiled Rust extension (built via maturin).
"""

__version__ = "0.3.0"
__author__ = "Amenti Labs"

from openentropy.openentropy import (
    EntropyPool,
    detect_available_sources,
    run_all_tests,
    calculate_quality_score,
    version as _rust_version,
)

__rust_backend__ = True
__version__ = _rust_version()

__all__ = [
    "EntropyPool",
    "detect_available_sources",
    "run_all_tests",
    "calculate_quality_score",
    "__version__",
    "__rust_backend__",
]
