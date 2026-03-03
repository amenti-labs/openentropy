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
    # Analysis
    full_analysis,
    autocorrelation_profile,
    spectral_analysis,
    bit_bias,
    distribution_stats,
    stationarity_test,
    runs_analysis,
    cross_correlation_matrix,
    pearson_correlation,
    # Comparison
    compare,
    aggregate_delta,
    two_sample_tests,
    cliffs_delta,
    temporal_analysis,
    digram_analysis,
    markov_analysis,
    multi_lag_analysis,
    run_length_comparison,
    # Trials
    trial_analysis,
    stouffer_combine,
    calibration_check,
    chaos_analysis,
    hurst_exponent,
    lyapunov_exponent,
    correlation_dimension,
    bientropy,
    epiplexity,
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
    # Analysis
    "full_analysis",
    "autocorrelation_profile",
    "spectral_analysis",
    "bit_bias",
    "distribution_stats",
    "stationarity_test",
    "runs_analysis",
    "cross_correlation_matrix",
    "pearson_correlation",
    # Comparison
    "compare",
    "aggregate_delta",
    "two_sample_tests",
    "cliffs_delta",
    "temporal_analysis",
    "digram_analysis",
    "markov_analysis",
    "multi_lag_analysis",
    "run_length_comparison",
    # Trials
    "trial_analysis",
    "stouffer_combine",
    "calibration_check",
    "chaos_analysis",
    "hurst_exponent",
    "lyapunov_exponent",
    "correlation_dimension",
    "bientropy",
    "epiplexity",
]
