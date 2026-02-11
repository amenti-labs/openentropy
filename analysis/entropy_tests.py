#!/usr/bin/env python3
"""
Statistical test framework for entropy characterization.
Implements Shannon entropy, min-entropy, chi-squared, serial correlation,
runs test, autocorrelation, and FFT spectral analysis.
"""
import numpy as np
from scipy import stats as sp_stats
from collections import Counter
import json


def shannon_entropy(data, base=2):
    """Calculate Shannon entropy of data in bits (default).
    
    H = -Σ p(x) * log2(p(x))
    """
    data = np.asarray(data)
    counts = Counter(data.flat)
    n = len(data.flat)
    probs = np.array([c / n for c in counts.values()])
    ent = -np.sum(probs * np.log2(probs + 1e-15))
    # Normalize by max possible entropy
    max_ent = np.log2(len(counts)) if len(counts) > 1 else 1.0
    return {
        'shannon_entropy': float(ent),
        'max_possible': float(max_ent),
        'efficiency': float(ent / max_ent) if max_ent > 0 else 0.0,
        'n_symbols': len(counts),
        'n_samples': n,
    }


def min_entropy(data):
    """Estimate min-entropy (NIST SP 800-90B style).
    
    H_min = -log2(max(p(x)))
    Most conservative entropy estimate — based on most probable symbol.
    """
    data = np.asarray(data).flat
    counts = Counter(data)
    n = sum(counts.values())
    p_max = max(counts.values()) / n
    h_min = -np.log2(p_max + 1e-15)
    return {
        'min_entropy': float(h_min),
        'p_max': float(p_max),
        'most_common': counts.most_common(1)[0],
    }


def chi_squared_uniformity(data):
    """Chi-squared test for uniformity of distribution.
    
    H0: data is uniformly distributed across observed symbols.
    """
    data = np.asarray(data).flat
    counts = Counter(data)
    observed = np.array(list(counts.values()))
    n = sum(observed)
    k = len(observed)
    expected = n / k
    chi2 = np.sum((observed - expected) ** 2 / expected)
    p_value = 1.0 - float(sp_stats.chi2.cdf(chi2, df=k - 1))
    return {
        'chi2_statistic': float(chi2),
        'degrees_of_freedom': k - 1,
        'p_value': float(p_value),
        'uniform': p_value > 0.01,  # reject uniformity if p < 0.01
        'n_bins': k,
    }


def serial_correlation(data, lag=1):
    """Compute serial (auto)correlation at given lag.
    
    Values near 0 indicate no linear dependence between successive samples.
    """
    data = np.asarray(data, dtype=float).flatten()
    n = len(data)
    if n < lag + 2:
        return {'serial_correlation': None, 'lag': lag, 'error': 'insufficient data'}
    
    mean = np.mean(data)
    var = np.var(data)
    if var < 1e-15:
        return {'serial_correlation': 0.0, 'lag': lag, 'note': 'zero variance'}
    
    x = data[:-lag] - mean
    y = data[lag:] - mean
    corr = np.mean(x * y) / var
    
    # Significance: under independence, r ~ N(0, 1/sqrt(n))
    z_score = corr * np.sqrt(n)
    p_value = 2 * (1 - float(sp_stats.norm.cdf(abs(z_score))))
    
    return {
        'serial_correlation': float(corr),
        'lag': lag,
        'z_score': float(z_score),
        'p_value': float(p_value),
        'independent': p_value > 0.01,
    }


def runs_test(data):
    """Runs test for randomness (Wald-Wolfowitz).
    
    Counts runs of consecutive values above/below the median.
    Too few runs → positive correlation; too many → negative correlation.
    """
    data = np.asarray(data, dtype=float).flatten()
    n = len(data)
    if n < 20:
        return {'error': 'need at least 20 samples'}
    
    median = np.median(data)
    binary = (data >= median).astype(int)
    
    # Count runs
    runs = 1
    for i in range(1, n):
        if binary[i] != binary[i - 1]:
            runs += 1
    
    n1 = np.sum(binary)
    n0 = n - n1
    
    if n0 == 0 or n1 == 0:
        return {'runs': runs, 'error': 'all values same side of median'}
    
    # Expected runs and variance under H0
    expected = 1 + (2 * n0 * n1) / n
    var = (2 * n0 * n1 * (2 * n0 * n1 - n)) / (n ** 2 * (n - 1))
    
    if var <= 0:
        return {'runs': runs, 'expected': float(expected), 'error': 'degenerate variance'}
    
    z = (runs - expected) / np.sqrt(var)
    p_value = 2 * (1 - float(sp_stats.norm.cdf(abs(z))))
    
    return {
        'runs': int(runs),
        'expected_runs': float(expected),
        'z_score': float(z),
        'p_value': float(p_value),
        'random': p_value > 0.01,
    }


def autocorrelation_analysis(data, max_lag=50):
    """Compute autocorrelation for lags 1 through max_lag.
    
    Returns the autocorrelation profile and flags concerning lags.
    """
    data = np.asarray(data, dtype=float).flatten()
    n = len(data)
    max_lag = min(max_lag, n // 4)
    
    mean = np.mean(data)
    var = np.var(data)
    if var < 1e-15:
        return {'error': 'zero variance', 'autocorrelations': []}
    
    centered = data - mean
    acf = []
    for lag in range(1, max_lag + 1):
        c = np.mean(centered[:-lag] * centered[lag:]) / var
        acf.append(float(c))
    
    # Flag lags with |acf| > 2/sqrt(n) (95% confidence under white noise)
    threshold = 2.0 / np.sqrt(n)
    significant_lags = [i + 1 for i, c in enumerate(acf) if abs(c) > threshold]
    
    return {
        'autocorrelations': acf,
        'max_lag': max_lag,
        'threshold_95': float(threshold),
        'significant_lags': significant_lags,
        'n_significant': len(significant_lags),
        'max_autocorr': float(max(abs(c) for c in acf)) if acf else 0.0,
    }


def fft_spectral_analysis(data):
    """FFT-based spectral analysis to detect hidden periodicities.
    
    Computes power spectral density and identifies dominant frequencies.
    """
    data = np.asarray(data, dtype=float).flatten()
    n = len(data)
    if n < 16:
        return {'error': 'need at least 16 samples'}
    
    # Remove mean
    centered = data - np.mean(data)
    
    # Compute FFT
    fft_vals = np.fft.rfft(centered)
    psd = np.abs(fft_vals) ** 2 / n
    freqs = np.fft.rfftfreq(n)
    
    # Skip DC component
    psd_no_dc = psd[1:]
    freqs_no_dc = freqs[1:]
    
    if len(psd_no_dc) == 0:
        return {'error': 'insufficient spectral resolution'}
    
    # Find peaks (> 3x median power)
    median_power = np.median(psd_no_dc)
    mean_power = np.mean(psd_no_dc)
    peak_threshold = 3 * median_power
    peaks = [(float(freqs_no_dc[i]), float(psd_no_dc[i])) 
             for i in range(len(psd_no_dc)) if psd_no_dc[i] > peak_threshold]
    
    # Spectral flatness (1.0 = white noise, 0.0 = pure tone)
    log_psd = np.log(psd_no_dc + 1e-15)
    geometric_mean = np.exp(np.mean(log_psd))
    arithmetic_mean = np.mean(psd_no_dc)
    flatness = geometric_mean / (arithmetic_mean + 1e-15)
    
    return {
        'spectral_flatness': float(flatness),
        'n_peaks': len(peaks),
        'dominant_frequencies': peaks[:10],  # top 10 peaks
        'mean_power': float(mean_power),
        'median_power': float(median_power),
        'white_noise_like': flatness > 0.5,
    }


def full_report(data, label="unknown"):
    """Run all statistical tests and return a structured report.
    
    Args:
        data: numpy array or list of numeric values
        label: descriptive name for this data source
        
    Returns:
        dict with all test results
    """
    data = np.asarray(data)
    
    report = {
        'source': label,
        'n_samples': len(data.flat),
        'basic_stats': {
            'mean': float(np.mean(data)),
            'std': float(np.std(data)),
            'min': float(np.min(data)),
            'max': float(np.max(data)),
            'n_unique': int(len(np.unique(data))),
        },
        'shannon': shannon_entropy(data),
        'min_entropy': min_entropy(data),
        'chi_squared': chi_squared_uniformity(data),
        'serial_correlation': serial_correlation(data),
        'runs_test': runs_test(data),
        'autocorrelation': autocorrelation_analysis(data),
        'spectral': fft_spectral_analysis(data),
    }
    
    # Overall quality score (0-100)
    scores = []
    scores.append(report['shannon']['efficiency'] * 100)
    
    h_min = report['min_entropy']['min_entropy']
    max_ent = report['shannon']['max_possible']
    if max_ent > 0:
        scores.append((h_min / max_ent) * 100)
    
    if report['chi_squared'].get('uniform'):
        scores.append(80)
    else:
        scores.append(20)
    
    if report['serial_correlation'].get('independent'):
        scores.append(80)
    else:
        scores.append(20)
    
    if report['runs_test'].get('random'):
        scores.append(80)
    else:
        scores.append(20)
    
    if report['spectral'].get('white_noise_like'):
        scores.append(80)
    else:
        scores.append(30)
    
    report['quality_score'] = float(np.mean(scores))
    report['grade'] = (
        'A' if report['quality_score'] >= 80 else
        'B' if report['quality_score'] >= 60 else
        'C' if report['quality_score'] >= 40 else
        'D' if report['quality_score'] >= 20 else 'F'
    )
    
    return report


def print_report(report):
    """Pretty-print a full report."""
    print(f"\n{'='*60}")
    print(f"  Entropy Report: {report['source']}")
    print(f"  Grade: {report['grade']} ({report['quality_score']:.1f}/100)")
    print(f"{'='*60}")
    print(f"  Samples: {report['n_samples']}")
    s = report['basic_stats']
    print(f"  Mean: {s['mean']:.4f}  Std: {s['std']:.4f}  Unique: {s['n_unique']}")
    
    sh = report['shannon']
    print(f"\n  Shannon Entropy: {sh['shannon_entropy']:.4f} / {sh['max_possible']:.4f} ({sh['efficiency']*100:.1f}%)")
    
    me = report['min_entropy']
    print(f"  Min-Entropy:     {me['min_entropy']:.4f} (p_max={me['p_max']:.4f})")
    
    chi = report['chi_squared']
    print(f"  Chi² Uniformity: χ²={chi['chi2_statistic']:.2f}, p={chi['p_value']:.4f} {'✓' if chi.get('uniform') else '✗'}")
    
    sc = report['serial_correlation']
    if sc.get('serial_correlation') is not None:
        print(f"  Serial Corr:     r={sc['serial_correlation']:.4f}, p={sc['p_value']:.4f} {'✓' if sc.get('independent') else '✗'}")
    
    rt = report['runs_test']
    if 'runs' in rt and 'expected_runs' in rt:
        print(f"  Runs Test:       {rt['runs']} runs (expected {rt['expected_runs']:.1f}), p={rt.get('p_value', 0):.4f} {'✓' if rt.get('random') else '✗'}")
    
    ac = report['autocorrelation']
    print(f"  Autocorrelation: {ac.get('n_significant', 0)} significant lags, max={ac.get('max_autocorr', 0):.4f}")
    
    sp = report['spectral']
    print(f"  Spectral:        flatness={sp.get('spectral_flatness', 0):.4f}, peaks={sp.get('n_peaks', 0)} {'✓' if sp.get('white_noise_like') else '✗'}")
    print()


if __name__ == '__main__':
    # Demo with random data
    print("Testing with numpy random data (should score high):")
    rng_data = np.random.randint(0, 256, size=10000)
    report = full_report(rng_data, "numpy.random (reference)")
    print_report(report)
    
    # Demo with biased data
    print("Testing with biased data (should score low):")
    biased = np.random.choice([0, 1, 2, 3], size=10000, p=[0.7, 0.1, 0.1, 0.1])
    report = full_report(biased, "biased distribution")
    print_report(report)
    
    # Demo with periodic data
    print("Testing with periodic data (should detect periodicity):")
    periodic = np.sin(np.arange(1000) * 0.1) * 100
    periodic = (periodic + np.random.normal(0, 5, 1000)).astype(int)
    report = full_report(periodic, "periodic + noise")
    print_report(report)
