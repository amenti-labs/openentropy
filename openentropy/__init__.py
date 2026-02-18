"""
openentropy: Your computer is a hardware noise observatory.

Harvests entropy from unconventional hardware sources — clock jitter,
kernel counters, memory timing, GPU scheduling, network latency, and more.

This package requires the compiled Rust extension (built via maturin).
"""

__author__ = "Amenti Labs"

from openentropy.openentropy import (
    EntropyPool,
    detect_available_sources,
    platform_info,
    detect_machine_info,
    run_all_tests,
    calculate_quality_score,
    condition,
    min_entropy_estimate,
    quick_min_entropy,
    quick_shannon,
    grade_min_entropy,
    quick_quality,
    version as _rust_version,
)

__rust_backend__ = True
__version__ = _rust_version()


def version() -> str:
    return _rust_version()

__all__ = [
    "EntropyPool",
    "detect_available_sources",
    "platform_info",
    "detect_machine_info",
    "run_all_tests",
    "calculate_quality_score",
    "condition",
    "min_entropy_estimate",
    "quick_min_entropy",
    "quick_shannon",
    "grade_min_entropy",
    "quick_quality",
    "version",
    "__version__",
    "__rust_backend__",
]
