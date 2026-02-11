#!/usr/bin/env python3
"""
Deep Entropy Tests — enhanced statistical testing for low-bit-rate sources.

Includes compression ratio, permutation entropy, approximate entropy,
cumulative sums, chi-squared, and visualization.
"""
import numpy as np
import zlib
import hashlib
from math import log2, factorial, log, ceil
from itertools import permutations
from collections import Counter

def compression_ratio_test(data):
    """Good randomness is incompressible. Ratio near 1.0 = good."""
    if len(data) < 10:
        return {'ratio': None, 'verdict': 'insufficient data'}
    compressed = zlib.compress(bytes(data), level=9)
    ratio = len(compressed) / len(data)
    verdict = 'excellent' if ratio > 0.95 else 'good' if ratio > 0.85 else 'fair' if ratio > 0.7 else 'poor'
    return {'ratio': ratio, 'verdict': verdict}

def chi_squared_test(data, bins=256):
    """Test uniformity of byte distribution."""
    if len(data) < bins:
        return {'chi2': None, 'verdict': 'insufficient data'}
    arr = np.frombuffer(bytes(data), dtype=np.uint8)
    hist = np.histogram(arr, bins=bins, range=(0, bins))[0]
    expected = len(arr) / bins
    chi2 = float(np.sum((hist - expected)**2 / expected))
    # For 255 df, p=0.05 critical value ~293
    verdict = 'pass' if chi2 < 293 else 'marginal' if chi2 < 350 else 'fail'
    return {'chi2': chi2, 'expected': 255.0, 'verdict': verdict}

def permutation_entropy(data, order=3, delay=1):
    """Permutation entropy — measures complexity of ordinal patterns."""
    if len(data) < (order - 1) * delay + order:
        return {'pe': None, 'verdict': 'insufficient data'}
    
    arr = np.frombuffer(bytes(data), dtype=np.uint8).astype(float)
    n = len(arr)
    
    # Generate ordinal patterns
    patterns = []
    for i in range(n - (order - 1) * delay):
        window = tuple(arr[i + j*delay] for j in range(order))
        # Convert to ordinal pattern
        pattern = tuple(sorted(range(order), key=lambda k: (window[k], k)))
        patterns.append(pattern)
    
    # Count patterns
    counter = Counter(patterns)
    total = len(patterns)
    max_patterns = factorial(order)
    
    # Shannon entropy of pattern distribution
    pe = 0
    for count in counter.values():
        p = count / total
        if p > 0:
            pe -= p * log2(p)
    
    # Normalize by maximum possible entropy
    max_entropy = log2(max_patterns)
    normalized_pe = pe / max_entropy if max_entropy > 0 else 0
    
    verdict = 'excellent' if normalized_pe > 0.95 else 'good' if normalized_pe > 0.85 else 'fair' if normalized_pe > 0.7 else 'poor'
    return {
        'pe': float(pe),
        'normalized_pe': float(normalized_pe),
        'max_entropy': float(max_entropy),
        'unique_patterns': len(counter),
        'max_patterns': max_patterns,
        'verdict': verdict,
    }

def approximate_entropy(data, m=2, r=None):
    """Approximate entropy (ApEn) — regularity measure."""
    arr = np.frombuffer(bytes(data), dtype=np.uint8).astype(float)
    n = len(arr)
    if n < 50:
        return {'apen': None, 'verdict': 'insufficient data'}
    
    # Use subset for speed
    if n > 5000:
        arr = arr[:5000]
        n = 5000
    
    if r is None:
        r = 0.2 * np.std(arr)
    if r == 0:
        r = 0.5
    
    def phi(m_val):
        patterns = np.array([arr[i:i+m_val] for i in range(n - m_val + 1)])
        counts = np.zeros(len(patterns))
        for i, p in enumerate(patterns):
            dists = np.max(np.abs(patterns - p), axis=1)
            counts[i] = np.sum(dists <= r) / len(patterns)
        return np.mean(np.log(counts + 1e-30))
    
    apen = float(phi(m) - phi(m + 1))
    # Higher ApEn = more random
    verdict = 'good' if apen > 1.0 else 'fair' if apen > 0.5 else 'poor'
    return {'apen': apen, 'verdict': verdict}

def cumulative_sums_test(data):
    """NIST cumulative sums test — checks for bias in running sum."""
    if len(data) < 100:
        return {'max_excursion': None, 'verdict': 'insufficient data'}
    
    arr = np.frombuffer(bytes(data), dtype=np.uint8)
    # Convert to +1/-1 based on MSB
    bits = ((arr >> 7) & 1).astype(np.int32) * 2 - 1
    cumsum = np.cumsum(bits)
    
    max_exc = float(np.max(np.abs(cumsum)))
    n = len(bits)
    expected_max = np.sqrt(n) * 1.96  # 95% confidence
    
    verdict = 'pass' if max_exc < expected_max else 'marginal' if max_exc < expected_max * 1.5 else 'fail'
    return {
        'max_excursion': max_exc,
        'expected_95pct': float(expected_max),
        'ratio': float(max_exc / expected_max),
        'verdict': verdict,
    }

def runs_test(data):
    """Count runs of consecutive bits — should be near expected."""
    if len(data) < 100:
        return {'runs': None, 'verdict': 'insufficient data'}
    
    arr = np.frombuffer(bytes(data), dtype=np.uint8)
    bits = (arr >> 7) & 1
    
    # Count runs
    runs = 1
    for i in range(1, len(bits)):
        if bits[i] != bits[i-1]:
            runs += 1
    
    n = len(bits)
    ones = np.sum(bits)
    zeros = n - ones
    
    if ones == 0 or zeros == 0:
        return {'runs': runs, 'verdict': 'fail (all same)'}
    
    expected = 1 + 2 * ones * zeros / n
    variance = 2 * ones * zeros * (2 * ones * zeros - n) / (n * n * (n - 1) + 1e-30)
    
    if variance <= 0:
        return {'runs': runs, 'expected': float(expected), 'verdict': 'indeterminate'}
    
    z = abs(runs - expected) / np.sqrt(variance)
    verdict = 'pass' if z < 1.96 else 'marginal' if z < 2.58 else 'fail'
    
    return {
        'runs': runs,
        'expected': float(expected),
        'z_score': float(z),
        'verdict': verdict,
    }

def byte_entropy(data):
    """Shannon entropy of byte distribution."""
    if len(data) == 0:
        return {'entropy': 0, 'max': 8, 'efficiency': 0}
    counter = Counter(data)
    total = len(data)
    ent = 0
    for count in counter.values():
        p = count / total
        if p > 0:
            ent -= p * log2(p)
    return {
        'entropy_bits': float(ent),
        'max_bits': 8.0,
        'efficiency': float(ent / 8.0),
    }

def full_test_suite(data, label=""):
    """Run all tests on data, return comprehensive results."""
    if isinstance(data, str):
        with open(data, 'rb') as f:
            data = f.read()
    
    results = {
        'label': label,
        'size_bytes': len(data),
        'byte_entropy': byte_entropy(data),
        'compression': compression_ratio_test(data),
        'chi_squared': chi_squared_test(data),
        'permutation': permutation_entropy(data),
        'approximate': approximate_entropy(data),
        'cumulative_sums': cumulative_sums_test(data),
        'runs': runs_test(data),
    }
    
    return results

def print_results(results):
    """Pretty-print test results."""
    print(f"\n{'='*60}")
    print(f"  Entropy Tests: {results['label']} ({results['size_bytes']} bytes)")
    print(f"{'='*60}")
    
    tests = [
        ('Byte Entropy', f"{results['byte_entropy']['entropy_bits']:.3f}/8.0 bits ({results['byte_entropy']['efficiency']*100:.1f}%)"),
        ('Compression', f"{results['compression']['ratio']:.3f}" if results['compression']['ratio'] else 'N/A', results['compression']['verdict']),
        ('Chi-Squared', f"{results['chi_squared']['chi2']:.1f}" if results['chi_squared']['chi2'] else 'N/A', results['chi_squared']['verdict']),
        ('Permutation Ent.', f"{results['permutation']['normalized_pe']:.3f}" if results['permutation'].get('normalized_pe') else 'N/A', results['permutation']['verdict']),
        ('Approx Entropy', f"{results['approximate']['apen']:.3f}" if results['approximate'].get('apen') else 'N/A', results['approximate']['verdict']),
        ('Cumulative Sums', f"{results['cumulative_sums']['ratio']:.3f}" if results['cumulative_sums'].get('ratio') else 'N/A', results['cumulative_sums']['verdict']),
        ('Runs Test', f"z={results['runs']['z_score']:.2f}" if results['runs'].get('z_score') else 'N/A', results['runs']['verdict']),
    ]
    
    for t in tests:
        if len(t) == 3:
            name, val, verdict = t
            emoji = '✅' if verdict in ('pass','excellent','good') else '⚠️' if verdict in ('marginal','fair') else '❌'
            print(f"  {emoji} {name:<20} {val:<15} [{verdict}]")
        else:
            name, val = t
            print(f"  📊 {name:<20} {val}")
    
    return results

def generate_heatmap_text(all_results):
    """Generate a text-based heatmap of entropy quality across sources."""
    if not all_results:
        return ""
    
    header = f"\n{'Source':<30} {'Bytes':>8} {'Entropy':>8} {'Compress':>9} {'Chi2':>8} {'PermEnt':>8} {'Verdict':>10}"
    lines = [header, "-" * 85]
    
    for r in all_results:
        ent = r['byte_entropy']['efficiency'] * 100
        comp = r['compression']['ratio'] * 100 if r['compression']['ratio'] else 0
        chi2 = r['chi_squared']['chi2'] if r['chi_squared']['chi2'] else 0
        pe = r['permutation'].get('normalized_pe', 0) or 0
        
        # Overall score
        score = (ent/100 + min(comp/100, 1) + (1 if chi2 < 293 else 0) + pe) / 4
        verdict = '★★★' if score > 0.85 else '★★' if score > 0.7 else '★' if score > 0.5 else '○'
        
        lines.append(f"{r['label'][:30]:<30} {r['size_bytes']:>8} {ent:>7.1f}% {comp:>8.1f}% {chi2:>8.1f} {pe:>7.3f} {verdict:>10}")
    
    return '\n'.join(lines)

if __name__ == '__main__':
    import sys
    if len(sys.argv) > 1:
        for path in sys.argv[1:]:
            results = full_test_suite(path, label=os.path.basename(path))
            print_results(results)
    else:
        # Test with random data
        import os
        results = full_test_suite(os.urandom(10000), label="os.urandom (reference)")
        print_results(results)
