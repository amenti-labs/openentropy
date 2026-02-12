"""Comprehensive NIST-inspired randomness test battery."""

from __future__ import annotations

import zlib
from collections import Counter
from dataclasses import dataclass, field
from math import factorial, log2, sqrt, pi, erfc, exp, ceil
from typing import Sequence

import numpy as np
from scipy import stats as sp_stats
from scipy.fft import fft


@dataclass
class TestResult:
    """Result of a single randomness test."""
    name: str
    passed: bool
    p_value: float | None
    statistic: float
    details: str
    grade: str  # A/B/C/D/F

    @staticmethod
    def grade_from_p(p: float | None) -> str:
        if p is None:
            return "F"
        if p >= 0.1:
            return "A"
        if p >= 0.01:
            return "B"
        if p >= 0.001:
            return "C"
        if p >= 0.0001:
            return "D"
        return "F"

    @staticmethod
    def pass_from_p(p: float | None, threshold: float = 0.01) -> bool:
        return p is not None and p >= threshold


def _to_bits(data: np.ndarray) -> np.ndarray:
    """Convert uint8 array to bit array."""
    return np.unpackbits(data.astype(np.uint8))


def _insufficient(name: str, needed: int, got: int) -> TestResult:
    return TestResult(name=name, passed=False, p_value=None, statistic=0.0,
                      details=f"Insufficient data: need {needed}, got {got}", grade="F")


# ═══════════════════════ FREQUENCY TESTS ═══════════════════════

def monobit_frequency(data: np.ndarray) -> TestResult:
    """Proportion of 1s vs 0s should be ~50%."""
    name = "Monobit Frequency"
    bits = _to_bits(data)
    n = len(bits)
    if n < 100:
        return _insufficient(name, 100, n)
    s = int(np.sum(bits.astype(np.int64))) * 2 - n  # sum of +1/-1
    s_obs = abs(s) / sqrt(n)
    p = erfc(s_obs / sqrt(2))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=s_obs, details=f"S={s}, n={n}", grade=TestResult.grade_from_p(p))


def block_frequency(data: np.ndarray, block_size: int = 128) -> TestResult:
    """Frequency within M-bit blocks."""
    name = "Block Frequency"
    bits = _to_bits(data)
    n = len(bits)
    num_blocks = n // block_size
    if num_blocks < 10:
        return _insufficient(name, block_size * 10, n)
    blocks = bits[:num_blocks * block_size].reshape(num_blocks, block_size)
    proportions = np.mean(blocks, axis=1)
    chi2 = 4 * block_size * np.sum((proportions - 0.5) ** 2)
    p = float(sp_stats.chi2.sf(chi2, num_blocks))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=float(chi2), details=f"blocks={num_blocks}, M={block_size}",
                      grade=TestResult.grade_from_p(p))


def byte_frequency(data: np.ndarray) -> TestResult:
    """Chi-squared on byte value distribution."""
    name = "Byte Frequency"
    n = len(data)
    if n < 256:
        return _insufficient(name, 256, n)
    hist = np.bincount(data.astype(np.uint8), minlength=256)
    expected = n / 256.0
    chi2 = float(np.sum((hist - expected) ** 2 / expected))
    p = float(sp_stats.chi2.sf(chi2, 255))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=chi2, details=f"n={n}, expected_per_bin={expected:.1f}",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ RUNS TESTS ═══════════════════════

def runs_test(data: np.ndarray) -> TestResult:
    """Number of uninterrupted runs of 0s or 1s."""
    name = "Runs Test"
    bits = _to_bits(data)
    n = len(bits)
    if n < 100:
        return _insufficient(name, 100, n)
    prop = np.mean(bits)
    if abs(prop - 0.5) >= 2 / sqrt(n):
        return TestResult(name=name, passed=False, p_value=0.0, statistic=0.0,
                          details=f"Pre-test failed: proportion={prop:.4f}", grade="F")
    runs = 1 + np.sum(bits[:-1] != bits[1:])
    expected = 2 * n * prop * (1 - prop) + 1
    std = 2 * sqrt(2 * n) * prop * (1 - prop)
    if std < 1e-10:
        return TestResult(name=name, passed=False, p_value=0.0, statistic=0.0,
                          details="Zero variance", grade="F")
    z = abs(runs - expected) / std
    p = erfc(z / sqrt(2))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=float(z), details=f"runs={runs}, expected={expected:.0f}",
                      grade=TestResult.grade_from_p(p))


def longest_run_of_ones(data: np.ndarray) -> TestResult:
    """Longest run of ones within blocks."""
    name = "Longest Run of Ones"
    bits = _to_bits(data)
    n = len(bits)
    if n < 128:
        return _insufficient(name, 128, n)
    M = 8
    num_blocks = n // M
    blocks = bits[:num_blocks * M].reshape(num_blocks, M)
    max_runs = []
    for block in blocks:
        s = ''.join(map(str, block))
        runs = s.split('0')
        max_runs.append(max(len(r) for r in runs))
    max_runs = np.array(max_runs)
    # Use chi-squared against expected distribution
    K = 3
    bins = [0, 1, 2, 3, max(4, max_runs.max() + 1)]
    observed, _ = np.histogram(max_runs, bins=bins)
    # Theoretical probabilities for M=8
    probs = np.array([0.2148, 0.3672, 0.2305, 0.1875])
    expected = probs * num_blocks
    mask = expected > 0
    chi2 = float(np.sum((observed[mask] - expected[mask]) ** 2 / expected[mask]))
    p = float(sp_stats.chi2.sf(chi2, K))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=chi2, details=f"blocks={num_blocks}, M={M}",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ SERIAL TESTS ═══════════════════════

def serial_test(data: np.ndarray, m: int = 4) -> TestResult:
    """Frequency of overlapping m-bit patterns."""
    name = "Serial Test"
    bits = _to_bits(data)
    n = len(bits)
    # Limit to first 20K bits for speed
    if n > 20000:
        bits = bits[:20000]
        n = 20000
    if n < 2 ** m + 10:
        return _insufficient(name, 2 ** m + 10, n)

    def psi_sq(m_val):
        if m_val < 1:
            return 0.0
        # Convert overlapping m-bit windows to integers
        extended = np.concatenate([bits, bits[:m_val - 1]])
        vals = np.zeros(n, dtype=np.int64)
        for j in range(m_val):
            vals = (vals << 1) | extended[j:j + n].astype(np.int64)
        counts = np.bincount(vals, minlength=2 ** m_val)
        return float(np.sum(counts.astype(np.int64) ** 2) * (2 ** m_val) / n - n)

    psi_m = psi_sq(m)
    psi_m1 = psi_sq(m - 1)
    psi_m2 = psi_sq(m - 2) if m >= 2 else 0
    delta1 = psi_m - psi_m1
    p = float(sp_stats.chi2.sf(delta1, 2 ** (m - 1)))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=delta1, details=f"m={m}, n_bits={n}",
                      grade=TestResult.grade_from_p(p))


def approximate_entropy(data: np.ndarray, m: int = 3) -> TestResult:
    """Compare frequencies of m and m+1 bit patterns (ApEn)."""
    name = "Approximate Entropy"
    bits = _to_bits(data)
    n = len(bits)
    if n > 20000:
        bits = bits[:20000]
        n = 20000
    if n < 64:
        return _insufficient(name, 64, n)

    def phi(block_len):
        extended = np.concatenate([bits, bits[:block_len - 1]])
        vals = np.zeros(n, dtype=np.int64)
        for j in range(block_len):
            vals = (vals << 1) | extended[j:j + n].astype(np.int64)
        counts = np.bincount(vals, minlength=2 ** block_len)
        probs = counts[counts > 0] / n
        return float(np.sum(probs * np.log2(probs)))

    phi_m = phi(m)
    phi_m1 = phi(m + 1)
    apen = phi_m - phi_m1
    chi2 = 2 * n * (log2(2) - apen)
    p = float(sp_stats.chi2.sf(chi2, 2 ** m))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=chi2, details=f"ApEn={apen:.6f}, m={m}",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ SPECTRAL TESTS ═══════════════════════

def dft_spectral(data: np.ndarray) -> TestResult:
    """Detect periodic features via FFT."""
    name = "DFT Spectral"
    bits = _to_bits(data).astype(float) * 2 - 1
    n = len(bits)
    if n < 64:
        return _insufficient(name, 64, n)
    S = np.abs(fft(bits))[:n // 2]
    T = sqrt(2.995732274 * n)  # threshold
    N0 = 0.95 * n / 2.0
    N1 = np.sum(S < T)
    d = (N1 - N0) / sqrt(n * 0.95 * 0.05 / 4)
    p = erfc(abs(d) / sqrt(2))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=float(d), details=f"peaks_below_threshold={N1}/{n // 2}",
                      grade=TestResult.grade_from_p(p))


def spectral_flatness(data: np.ndarray) -> TestResult:
    """How close to white noise in frequency domain."""
    name = "Spectral Flatness"
    arr = data.astype(float)
    n = len(arr)
    if n < 64:
        return _insufficient(name, 64, n)
    S = np.abs(fft(arr))[:n // 2] ** 2
    S = S + 1e-15
    geo_mean = np.exp(np.mean(np.log(S)))
    arith_mean = np.mean(S)
    flatness = float(geo_mean / arith_mean)
    # Flatness close to 1.0 = white noise
    passed = flatness > 0.5
    grade = "A" if flatness > 0.8 else "B" if flatness > 0.6 else "C" if flatness > 0.4 else "D" if flatness > 0.2 else "F"
    return TestResult(name=name, passed=passed, p_value=None,
                      statistic=flatness, details=f"flatness={flatness:.4f} (1.0=white noise)",
                      grade=grade)


# ═══════════════════════ ENTROPY TESTS ═══════════════════════

def shannon_entropy_test(data: np.ndarray) -> TestResult:
    """Shannon entropy in bits per byte (max 8.0)."""
    name = "Shannon Entropy"
    n = len(data)
    if n < 16:
        return _insufficient(name, 16, n)
    counts = np.bincount(data.astype(np.uint8), minlength=256)
    probs = counts[counts > 0] / n
    h = float(-np.sum(probs * np.log2(probs)))
    ratio = h / 8.0
    grade = "A" if ratio > 0.95 else "B" if ratio > 0.85 else "C" if ratio > 0.7 else "D" if ratio > 0.5 else "F"
    return TestResult(name=name, passed=ratio > 0.85, p_value=None,
                      statistic=h, details=f"{h:.4f} / 8.0 bits ({ratio:.1%})", grade=grade)


def min_entropy_test(data: np.ndarray) -> TestResult:
    """Min-entropy (NIST SP 800-90B)."""
    name = "Min-Entropy"
    n = len(data)
    if n < 16:
        return _insufficient(name, 16, n)
    p_max = max(Counter(data.astype(np.uint8).tolist()).values()) / n
    h_min = float(-log2(p_max + 1e-15))
    ratio = h_min / 8.0
    grade = "A" if ratio > 0.9 else "B" if ratio > 0.75 else "C" if ratio > 0.5 else "D" if ratio > 0.25 else "F"
    return TestResult(name=name, passed=ratio > 0.7, p_value=None,
                      statistic=h_min, details=f"{h_min:.4f} / 8.0 bits ({ratio:.1%})", grade=grade)


def permutation_entropy_test(data: np.ndarray, order: int = 4) -> TestResult:
    """Complexity of ordinal patterns."""
    name = "Permutation Entropy"
    n = len(data)
    if n < order + 10:
        return _insufficient(name, order + 10, n)
    arr = data.astype(float)
    patterns = Counter()
    for i in range(n - order):
        w = tuple(arr[i:i + order])
        patterns[tuple(np.argsort(w))] += 1
    total = sum(patterns.values())
    h = sum(-((c / total) * log2(c / total)) for c in patterns.values())
    h_max = log2(factorial(order))
    normalized = h / h_max if h_max > 0 else 0
    grade = "A" if normalized > 0.95 else "B" if normalized > 0.85 else "C" if normalized > 0.7 else "D" if normalized > 0.5 else "F"
    return TestResult(name=name, passed=normalized > 0.85, p_value=None,
                      statistic=normalized, details=f"PE={h:.4f}/{h_max:.4f} = {normalized:.4f}",
                      grade=grade)


def compression_ratio_test(data: np.ndarray) -> TestResult:
    """Ratio after zlib compression (random = incompressible ≈ 1.0+)."""
    name = "Compression Ratio"
    raw = data.astype(np.uint8).tobytes()
    n = len(raw)
    if n < 32:
        return _insufficient(name, 32, n)
    compressed = len(zlib.compress(raw, 9))
    ratio = compressed / n
    # ratio >= ~1.0 means incompressible
    grade = "A" if ratio > 0.95 else "B" if ratio > 0.85 else "C" if ratio > 0.7 else "D" if ratio > 0.5 else "F"
    return TestResult(name=name, passed=ratio > 0.85, p_value=None,
                      statistic=ratio, details=f"{compressed}/{n} = {ratio:.4f}",
                      grade=grade)


def kolmogorov_complexity_test(data: np.ndarray) -> TestResult:
    """Kolmogorov complexity estimate via compression."""
    name = "Kolmogorov Complexity"
    raw = data.astype(np.uint8).tobytes()
    n = len(raw)
    if n < 32:
        return _insufficient(name, 32, n)
    # Try multiple levels
    c1 = len(zlib.compress(raw, 1))
    c9 = len(zlib.compress(raw, 9))
    complexity = c9 / n
    spread = (c1 - c9) / n  # how much compression improves = structure
    grade = "A" if complexity > 0.95 else "B" if complexity > 0.85 else "C" if complexity > 0.7 else "D" if complexity > 0.5 else "F"
    return TestResult(name=name, passed=complexity > 0.85, p_value=None,
                      statistic=complexity, details=f"K≈{complexity:.4f}, spread={spread:.4f}",
                      grade=grade)


# ═══════════════════════ CORRELATION TESTS ═══════════════════════

def autocorrelation_test(data: np.ndarray, max_lag: int = 50) -> TestResult:
    """Autocorrelation at lags 1 to max_lag."""
    name = "Autocorrelation"
    arr = data.astype(float)
    n = len(arr)
    if n < max_lag + 10:
        return _insufficient(name, max_lag + 10, n)
    mean = np.mean(arr)
    var = np.var(arr)
    if var < 1e-10:
        return TestResult(name=name, passed=False, p_value=None, statistic=1.0,
                          details="Zero variance", grade="F")
    max_corr = 0.0
    threshold = 2.0 / sqrt(n)  # 95% confidence
    violations = 0
    for lag in range(1, min(max_lag + 1, n)):
        c = float(np.mean((arr[:-lag] - mean) * (arr[lag:] - mean)) / var)
        max_corr = max(max_corr, abs(c))
        if abs(c) > threshold:
            violations += 1
    # Expect ~5% violations by chance
    expected_violations = 0.05 * max_lag
    p = float(sp_stats.poisson.sf(violations, max(expected_violations, 1)))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=max_corr, details=f"violations={violations}/{max_lag}, max|r|={max_corr:.4f}",
                      grade=TestResult.grade_from_p(p))


def serial_correlation_test(data: np.ndarray) -> TestResult:
    """Adjacent value correlation."""
    name = "Serial Correlation"
    arr = data.astype(float)
    n = len(arr)
    if n < 20:
        return _insufficient(name, 20, n)
    mean = np.mean(arr)
    var = np.var(arr)
    if var < 1e-10:
        return TestResult(name=name, passed=False, p_value=None, statistic=1.0,
                          details="Zero variance", grade="F")
    r = float(np.mean((arr[:-1] - mean) * (arr[1:] - mean)) / var)
    z = r * sqrt(n)
    p = 2 * (1 - sp_stats.norm.cdf(abs(z)))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=abs(r), details=f"r={r:.6f}, z={z:.4f}",
                      grade=TestResult.grade_from_p(p))


def lag_n_correlation(data: np.ndarray, lags: Sequence[int] = (1, 2, 4, 8, 16, 32)) -> TestResult:
    """Correlation at specific lag distances."""
    name = "Lag-N Correlation"
    arr = data.astype(float)
    n = len(arr)
    if n < max(lags) + 10:
        return _insufficient(name, max(lags) + 10, n)
    mean = np.mean(arr)
    var = np.var(arr)
    if var < 1e-10:
        return TestResult(name=name, passed=False, p_value=None, statistic=1.0,
                          details="Zero variance", grade="F")
    max_corr = 0.0
    details_parts = []
    for lag in lags:
        if lag >= n:
            continue
        c = float(np.mean((arr[:-lag] - mean) * (arr[lag:] - mean)) / var)
        max_corr = max(max_corr, abs(c))
        details_parts.append(f"lag{lag}={c:.4f}")
    threshold = 2.0 / sqrt(n)
    passed = max_corr < threshold
    grade = "A" if max_corr < threshold * 0.5 else "B" if max_corr < threshold else "C" if max_corr < threshold * 2 else "D" if max_corr < threshold * 4 else "F"
    return TestResult(name=name, passed=passed, p_value=None,
                      statistic=max_corr, details=", ".join(details_parts), grade=grade)


def cross_correlation_test(data: np.ndarray) -> TestResult:
    """Independence of byte positions (even vs odd, etc.)."""
    name = "Cross-Correlation"
    n = len(data)
    if n < 100:
        return _insufficient(name, 100, n)
    even = data[::2].astype(float)
    odd = data[1::2].astype(float)
    min_len = min(len(even), len(odd))
    even, odd = even[:min_len], odd[:min_len]
    r, p = sp_stats.pearsonr(even, odd)
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=abs(float(r)), details=f"r={r:.6f} (even vs odd bytes)",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ DISTRIBUTION TESTS ═══════════════════════

def chi_squared_test(data: np.ndarray) -> TestResult:
    """Chi-squared goodness of fit (byte distribution vs uniform)."""
    # Same as byte_frequency but named differently for the category
    return byte_frequency(data)


def ks_test(data: np.ndarray) -> TestResult:
    """Kolmogorov-Smirnov test: compare CDF to uniform."""
    name = "Kolmogorov-Smirnov"
    n = len(data)
    if n < 50:
        return _insufficient(name, 50, n)
    normalized = data.astype(float) / 255.0
    stat, p = sp_stats.kstest(normalized, 'uniform')
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=float(stat), details=f"D={stat:.6f}, n={n}",
                      grade=TestResult.grade_from_p(p))


def anderson_darling_test(data: np.ndarray) -> TestResult:
    """Anderson-Darling test (more sensitive to tails)."""
    name = "Anderson-Darling"
    n = len(data)
    if n < 50:
        return _insufficient(name, 50, n)
    normalized = data.astype(float) / 255.0
    # Add tiny noise to avoid ties
    normalized = normalized + np.random.default_rng(42).normal(0, 1e-10, n)
    result = sp_stats.anderson(normalized, dist='norm')
    stat = float(result.statistic)
    if hasattr(result, 'critical_values') and result.critical_values is not None:
        crit_5 = result.critical_values[2]  # 5% level
        passed = stat < crit_5
        grade = "A" if stat < result.critical_values[0] else "B" if stat < result.critical_values[1] else "C" if stat < crit_5 else "D" if stat < result.critical_values[3] else "F"
        details = f"A²={stat:.4f}, 5% critical={crit_5:.4f}"
    elif hasattr(result, 'pvalue'):
        p = float(result.pvalue)
        passed = p > 0.05
        grade = "A" if p > 0.5 else "B" if p > 0.1 else "C" if p > 0.05 else "D" if p > 0.01 else "F"
        details = f"A²={stat:.4f}, p={p:.6f}"
    else:
        passed = False
        grade = "F"
        details = f"A²={stat:.4f}"
    return TestResult(name=name, passed=passed, p_value=None,
                      statistic=stat, details=details, grade=grade)


# ═══════════════════════ PATTERN TESTS ═══════════════════════

def overlapping_template(data: np.ndarray, template: tuple = (1, 1, 1, 1)) -> TestResult:
    """Frequency of specific overlapping bit patterns."""
    name = "Overlapping Template"
    bits = _to_bits(data)
    n = len(bits)
    m = len(template)
    if n < 1000:
        return _insufficient(name, 1000, n)
    # Use convolution-based matching for speed
    t = np.array(template)
    # Vectorized: check all positions at once
    windows = np.lib.stride_tricks.sliding_window_view(bits[:n], m)
    count = int(np.sum(np.all(windows == t, axis=1)))
    expected = (n - m + 1) / (2 ** m)
    std = sqrt(expected * (1 - 1 / (2 ** m)))
    if std < 1e-10:
        return TestResult(name=name, passed=False, p_value=None, statistic=0.0,
                          details="Zero std", grade="F")
    z = (count - expected) / std
    p = 2 * (1 - sp_stats.norm.cdf(abs(z)))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=abs(z), details=f"count={count}, expected={expected:.0f}",
                      grade=TestResult.grade_from_p(p))


def non_overlapping_template(data: np.ndarray, template: tuple = (0, 0, 1, 1)) -> TestResult:
    """Count non-overlapping occurrences of a template."""
    name = "Non-overlapping Template"
    bits = _to_bits(data)
    n = len(bits)
    m = len(template)
    if n < 1000:
        return _insufficient(name, 1000, n)
    t = np.array(template)
    count = 0
    i = 0
    while i <= n - m:
        if np.array_equal(bits[i:i + m], t):
            count += 1
            i += m
        else:
            i += 1
    expected = (n / (2 ** m))
    var = n * (1 / (2 ** m) - (2 * m - 1) / (2 ** (2 * m)))
    if var <= 0:
        var = 1.0
    z = (count - expected) / sqrt(var)
    p = 2 * (1 - sp_stats.norm.cdf(abs(z)))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=abs(z), details=f"count={count}, expected={expected:.0f}",
                      grade=TestResult.grade_from_p(p))


def maurers_universal(data: np.ndarray, L: int = 6, Q: int = 640) -> TestResult:
    """Maurer's universal statistical test."""
    name = "Maurer's Universal"
    bits = _to_bits(data)
    n_bits = len(bits)
    K = n_bits // L - Q
    if K < 100 or Q < 10 * (2 ** L):
        return _insufficient(name, (Q + 100) * L, n_bits)

    # Initialize table
    table = np.zeros(2 ** L, dtype=int)
    for i in range(Q):
        block = 0
        for j in range(L):
            block = (block << 1) | int(bits[i * L + j])
        table[block] = i + 1

    # Test phase
    total = 0.0
    for i in range(Q, Q + K):
        block = 0
        for j in range(L):
            block = (block << 1) | int(bits[i * L + j])
        total += log2(i + 1 - table[block]) if table[block] > 0 else log2(i + 1)
        table[block] = i + 1

    fn = total / K
    # Expected values for L=6
    expected = 5.2177052
    variance = 2.954
    sigma = sqrt(variance / K)
    z = abs(fn - expected) / max(sigma, 1e-10)
    p = erfc(z / sqrt(2))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=fn, details=f"fn={fn:.4f}, expected={expected:.4f}, L={L}",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ ADVANCED TESTS ═══════════════════════

def binary_matrix_rank(data: np.ndarray) -> TestResult:
    """Rank of random binary matrices."""
    name = "Binary Matrix Rank"
    bits = _to_bits(data)
    n = len(bits)
    M, Q = 32, 32
    N = n // (M * Q)
    if N < 38:
        return _insufficient(name, 38 * M * Q, n)
    ranks = []
    for i in range(N):
        block = bits[i * M * Q:(i + 1) * M * Q].reshape(M, Q).astype(float)
        ranks.append(int(np.linalg.matrix_rank(block)))

    full_rank = sum(1 for r in ranks if r == min(M, Q))
    rank_m1 = sum(1 for r in ranks if r == min(M, Q) - 1)
    rest = N - full_rank - rank_m1

    # Theoretical probabilities
    p_full = 0.2888
    p_m1 = 0.5776
    p_rest = 0.1336
    chi2 = ((full_rank - N * p_full) ** 2 / (N * p_full) +
            (rank_m1 - N * p_m1) ** 2 / (N * p_m1) +
            (rest - N * p_rest) ** 2 / (N * p_rest))
    p = float(sp_stats.chi2.sf(chi2, 2))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=chi2, details=f"N={N}, full={full_rank}, full-1={rank_m1}",
                      grade=TestResult.grade_from_p(p))


def linear_complexity(data: np.ndarray, block_size: int = 200) -> TestResult:
    """LFSR complexity of sequence."""
    name = "Linear Complexity"
    bits = _to_bits(data)
    n = len(bits)
    N = n // block_size
    if N < 6:
        return _insufficient(name, 6 * block_size, n)

    def berlekamp_massey(seq):
        """Berlekamp-Massey for binary sequence."""
        n_s = len(seq)
        c = np.zeros(n_s, dtype=int)
        b = np.zeros(n_s, dtype=int)
        c[0] = b[0] = 1
        L, m, d_prev = 0, -1, 1
        for n_i in range(n_s):
            d = seq[n_i]
            for i in range(1, L + 1):
                d ^= c[i] & seq[n_i - i]
            if d == 1:
                t = c.copy()
                shift = n_i - m
                for i in range(shift, n_s):
                    c[i] ^= b[i - shift]
                if L <= n_i // 2:
                    L = n_i + 1 - L
                    m = n_i
                    b = t
        return L

    complexities = []
    for i in range(N):
        block = bits[i * block_size:(i + 1) * block_size].astype(int)
        complexities.append(berlekamp_massey(block))

    mu = block_size / 2 + (9 + (-1) ** (block_size + 1)) / 36 - (block_size / 3 + 2 / 9) / (2 ** block_size)
    T = np.array([(-1) ** block_size * (c - mu) + 2 / 9 for c in complexities])

    # Bin into categories
    bins = [-float('inf'), -2.5, -1.5, -0.5, 0.5, 1.5, 2.5, float('inf')]
    observed, _ = np.histogram(T, bins=bins)
    probs = np.array([0.010882, 0.03534, 0.08884, 0.5, 0.08884, 0.03534, 0.010882])
    # Adjust last prob to sum to 1
    probs[-1] = 1 - probs[:-1].sum()
    expected = probs * N
    mask = expected > 0
    chi2 = float(np.sum((observed[mask] - expected[mask]) ** 2 / expected[mask]))
    p = float(sp_stats.chi2.sf(chi2, 6))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=chi2, details=f"N={N}, mean_complexity={np.mean(complexities):.1f}",
                      grade=TestResult.grade_from_p(p))


def cusum_test(data: np.ndarray) -> TestResult:
    """Cumulative sums (CUSUM) — detect drift/bias."""
    name = "Cumulative Sums"
    bits = _to_bits(data).astype(float) * 2 - 1
    n = len(bits)
    if n < 100:
        return _insufficient(name, 100, n)
    cumsum = np.cumsum(bits)
    z = float(np.max(np.abs(cumsum)))
    # Approximate p-value
    k_start = int((-n / z + 1) / 4)
    k_end = int((n / z - 1) / 4) + 1
    s = 0.0
    for k in range(k_start, k_end + 1):
        s += sp_stats.norm.cdf((4 * k + 1) * z / sqrt(n)) - sp_stats.norm.cdf((4 * k - 1) * z / sqrt(n))
    p = 1.0 - s
    p = max(0.0, min(1.0, p))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=z, details=f"max|S|={z:.1f}, n={n}",
                      grade=TestResult.grade_from_p(p))


def random_excursions_test(data: np.ndarray) -> TestResult:
    """Cycles in cumulative sum random walk."""
    name = "Random Excursions"
    bits = _to_bits(data).astype(float) * 2 - 1
    n = len(bits)
    if n < 1000:
        return _insufficient(name, 1000, n)
    cumsum = np.concatenate(([0], np.cumsum(bits), [0]))
    # Find zero crossings
    zeros = np.where(cumsum == 0)[0]
    J = len(zeros) - 1  # number of cycles
    if J < 500:
        return TestResult(name=name, passed=True, p_value=None, statistic=float(J),
                          details=f"Only {J} cycles (need 500 for reliable test)", grade="B")
    # Simplified: check if number of cycles is reasonable
    expected_cycles = n / sqrt(2 * pi * n)  # rough approximation
    ratio = J / max(expected_cycles, 1)
    passed = 0.5 < ratio < 2.0
    grade = "A" if 0.8 < ratio < 1.2 else "B" if 0.6 < ratio < 1.5 else "C" if passed else "F"
    return TestResult(name=name, passed=passed, p_value=None, statistic=float(J),
                      details=f"cycles={J}, expected≈{expected_cycles:.0f}",
                      grade=grade)


def birthday_spacing_test(data: np.ndarray) -> TestResult:
    """Birthday spacing — spacing between repeated values."""
    name = "Birthday Spacing"
    n = len(data)
    if n < 100:
        return _insufficient(name, 100, n)
    # Use pairs as 16-bit values
    if n < 200:
        values = data.astype(int)
    else:
        values = data[::2][:n // 2].astype(int) * 256 + data[1::2][:n // 2].astype(int)
    values_sorted = np.sort(values)
    spacings = np.diff(values_sorted)
    spacing_sorted = np.sort(spacings)
    # Count duplicate spacings
    dups = np.sum(spacing_sorted[:-1] == spacing_sorted[1:])
    m = len(values)
    d = max(values) + 1 if len(values) > 0 else 1
    # Expected duplicates ~ m^3 / (4*d)
    lam = max(m ** 3 / (4 * d), 0.01)
    p = float(sp_stats.poisson.sf(dups, lam))
    return TestResult(name=name, passed=TestResult.pass_from_p(max(p, 1 - p)), p_value=max(p, 1 - p),
                      statistic=float(dups), details=f"duplicates={dups}, lambda={lam:.2f}, m={m}",
                      grade=TestResult.grade_from_p(max(p, 1 - p)))


# ═══════════════════════ PRACTICAL TESTS ═══════════════════════

def bit_avalanche_test(data: np.ndarray) -> TestResult:
    """Changing 1 input bit should change ~50% of output bits."""
    name = "Bit Avalanche"
    n = len(data)
    if n < 100:
        return _insufficient(name, 100, n)
    # Compare adjacent bytes: how many bits differ?
    xored = np.bitwise_xor(data[:-1].astype(np.uint8), data[1:].astype(np.uint8))
    bit_diffs = np.array([bin(x).count('1') for x in xored])
    mean_diff = np.mean(bit_diffs)
    expected = 4.0  # half of 8 bits
    std = sqrt(2.0)  # binomial std for n=8, p=0.5
    z = abs(mean_diff - expected) / (std / sqrt(len(bit_diffs)))
    p = 2 * (1 - sp_stats.norm.cdf(z))
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=mean_diff, details=f"mean_diff={mean_diff:.3f}/8 bits, expected=4.0",
                      grade=TestResult.grade_from_p(p))


def monte_carlo_pi(data: np.ndarray) -> TestResult:
    """Estimate π using pairs as (x,y) coordinates."""
    name = "Monte Carlo Pi"
    n = len(data)
    if n < 200:
        return _insufficient(name, 200, n)
    pairs = n // 2
    x = data[:pairs].astype(float) / 255.0
    y = data[pairs:2 * pairs].astype(float) / 255.0
    inside = np.sum(x ** 2 + y ** 2 <= 1.0)
    pi_est = 4.0 * inside / pairs
    error = abs(pi_est - pi) / pi
    grade = "A" if error < 0.01 else "B" if error < 0.03 else "C" if error < 0.1 else "D" if error < 0.2 else "F"
    return TestResult(name=name, passed=error < 0.05, p_value=None,
                      statistic=pi_est, details=f"π≈{pi_est:.6f}, error={error:.4%}",
                      grade=grade)


def mean_variance_test(data: np.ndarray) -> TestResult:
    """Mean and variance should match theoretical uniform distribution."""
    name = "Mean & Variance"
    n = len(data)
    if n < 50:
        return _insufficient(name, 50, n)
    arr = data.astype(float)
    mean = np.mean(arr)
    var = np.var(arr)
    expected_mean = 127.5
    expected_var = (256 ** 2 - 1) / 12.0  # ≈ 5461.25
    # Z-test for mean
    z_mean = abs(mean - expected_mean) / (sqrt(expected_var / n))
    p_mean = 2 * (1 - sp_stats.norm.cdf(z_mean))
    # Chi-squared for variance
    chi2_var = (n - 1) * var / expected_var
    p_var = 2 * min(sp_stats.chi2.cdf(chi2_var, n - 1), sp_stats.chi2.sf(chi2_var, n - 1))
    p = min(p_mean, p_var)
    return TestResult(name=name, passed=TestResult.pass_from_p(p), p_value=p,
                      statistic=z_mean, details=f"mean={mean:.2f} (exp 127.5), var={var:.1f} (exp {expected_var:.1f})",
                      grade=TestResult.grade_from_p(p))


# ═══════════════════════ TEST BATTERY ═══════════════════════

ALL_TESTS = [
    # Frequency
    monobit_frequency, block_frequency, byte_frequency,
    # Runs
    runs_test, longest_run_of_ones,
    # Serial
    serial_test, approximate_entropy,
    # Spectral
    dft_spectral, spectral_flatness,
    # Entropy
    shannon_entropy_test, min_entropy_test, permutation_entropy_test,
    compression_ratio_test, kolmogorov_complexity_test,
    # Correlation
    autocorrelation_test, serial_correlation_test, lag_n_correlation, cross_correlation_test,
    # Distribution
    ks_test, anderson_darling_test,
    # Pattern
    overlapping_template, non_overlapping_template, maurers_universal,
    # Advanced
    binary_matrix_rank, linear_complexity, cusum_test,
    random_excursions_test, birthday_spacing_test,
    # Practical
    bit_avalanche_test, monte_carlo_pi, mean_variance_test,
]


def run_all_tests(data: np.ndarray) -> list[TestResult]:
    """Run the complete test battery on a byte array."""
    data = np.asarray(data).flatten().astype(np.uint8)
    results = []
    for test_fn in ALL_TESTS:
        try:
            result = test_fn(data)
            results.append(result)
        except Exception as e:
            results.append(TestResult(
                name=test_fn.__name__.replace('_', ' ').title(),
                passed=False, p_value=None, statistic=0.0,
                details=f"Error: {e}", grade="F"
            ))
    return results


def calculate_quality_score(results: list[TestResult]) -> float:
    """Calculate overall quality score 0-100."""
    if not results:
        return 0.0
    grade_scores = {"A": 100, "B": 75, "C": 50, "D": 25, "F": 0}
    return sum(grade_scores.get(r.grade, 0) for r in results) / len(results)
