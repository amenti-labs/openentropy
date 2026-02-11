#!/usr/bin/env python3
"""
IORegistry Deep Mining — discover ALL fluctuating numeric values in the IORegistry.
"""
import subprocess
import re
import time
import hashlib
import numpy as np
from collections import defaultdict

# Pre-compile regex for speed
NUM_RE = re.compile(r'"([^"]{1,80})"\s*=\s*(\d{1,15})(?:\s|,|}|$)')


def parse_ioreg_fast():
    """Fast parse of ioreg for numeric values."""
    result = subprocess.run(
        ['/usr/sbin/ioreg', '-l', '-w0'],
        capture_output=True, text=True, timeout=30
    )
    values = {}
    for m in NUM_RE.finditer(result.stdout):
        key, val = m.group(1), int(m.group(2))
        if 0 < val < 2**48:
            if key not in values:
                values[key] = val
            else:
                # If same key appears multiple times, make unique
                values[f"{key}#{hash(val) % 10000}"] = val
    return values


def multi_sample(n_samples=15, interval_s=0.5):
    """Sample ioreg repeatedly."""
    print(f"[IOReg] Sampling {n_samples} times at {interval_s}s intervals...")
    all_samples = defaultdict(list)
    
    for i in range(n_samples):
        t0 = time.time()
        values = parse_ioreg_fast()
        elapsed = time.time() - t0
        for key, val in values.items():
            all_samples[key].append(val)
        time.sleep(max(0, interval_s - elapsed))
        if (i + 1) % 5 == 0:
            print(f"  Sample {i+1}/{n_samples} ({len(values)} values, {elapsed:.1f}s parse)")
    
    return dict(all_samples)


def find_fluctuating(samples_dict, min_samples=10):
    """Find values that change between samples."""
    fluctuating = {}
    static = 0
    
    for key, values in samples_dict.items():
        if len(values) < min_samples:
            continue
        unique = len(set(values))
        if unique > 1:
            arr = np.array(values, dtype=np.float64)
            fluctuating[key] = {
                'values': values,
                'unique': unique,
                'mean': np.mean(arr),
                'std': np.std(arr),
                'range_pct': (np.max(arr) - np.min(arr)) / (abs(np.mean(arr)) + 1e-30) * 100,
            }
        else:
            static += 1
    
    return fluctuating, static


def extract_entropy(fluctuating):
    """Extract entropy from all fluctuating values."""
    all_bytes = bytearray()
    
    for key, info in fluctuating.items():
        arr = np.array(info['values'], dtype=np.float64)
        if len(arr) < 5:
            continue
        # Detrend
        k = min(3, len(arr) // 2)
        detrended = arr - np.convolve(arr, np.ones(k)/k, mode='same')
        noise = detrended[k:-k]
        
        if len(noise) < 3 or np.std(noise) == 0:
            continue
        
        normalized = (noise - noise.min()) / (noise.max() - noise.min() + 1e-30)
        quantized = (normalized * 255).astype(np.uint8)
        all_bytes.extend(quantized.tobytes())
    
    return bytes(all_bytes)


def run(output_file='explore/entropy_ioregistry.bin'):
    print("=" * 60)
    print("IOREGISTRY DEEP MINING — Discover All Fluctuating Values")
    print("=" * 60)
    
    # Sample
    print("\n[Phase 1] Multi-sample ioreg...")
    samples = multi_sample(n_samples=15, interval_s=0.5)
    print(f"  Tracked {len(samples)} unique keys")
    
    # Find fluctuating
    print("\n[Phase 2] Finding fluctuating values...")
    fluctuating, static = find_fluctuating(samples)
    print(f"  {len(fluctuating)} fluctuating keys, {static} static keys")
    
    if not fluctuating:
        print("[FAIL] No fluctuating values found")
        return None
    
    # Report top fluctuators
    sorted_fluct = sorted(fluctuating.items(), key=lambda x: -x[1]['std'])
    print(f"\n  {'Key':<50} {'Unique':>6} {'StdDev':>12} {'Range%':>8}")
    print("  " + "-" * 78)
    for key, info in sorted_fluct[:30]:
        print(f"  {key[:50]:<50} {info['unique']:>6} {info['std']:>12.1f} {info['range_pct']:>7.2f}%")
    
    # Extract entropy
    print(f"\n[Phase 3] Extracting entropy from {len(fluctuating)} sources...")
    entropy_data = extract_entropy(fluctuating)
    
    if not entropy_data:
        print("[FAIL] No entropy extracted")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(entropy_data)
    
    sha = hashlib.sha256(entropy_data).hexdigest()
    print(f"\n[RESULT] Collected {len(entropy_data)} entropy bytes from {len(fluctuating)} sources")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    if len(entropy_data) > 100:
        ratio = len(zlib.compress(entropy_data)) / len(entropy_data)
        print(f"  Compression ratio: {ratio:.3f}")
    
    # Categorize
    categories = defaultdict(int)
    for key in fluctuating:
        kl = key.lower()
        if any(w in kl for w in ['temp', 'therm', 'die']):
            categories['thermal'] += 1
        elif any(w in kl for w in ['volt', 'power', 'watt', 'current']):
            categories['power'] += 1
        elif any(w in kl for w in ['time', 'count', 'stat', 'busy']):
            categories['counters'] += 1
        elif any(w in kl for w in ['mem', 'alloc', 'page']):
            categories['memory'] += 1
        else:
            categories['other'] += 1
    
    print(f"\n  Categories: {dict(categories)}")
    
    return {
        'total_bytes': len(entropy_data),
        'fluctuating_keys': len(fluctuating),
        'static_keys': static,
        'sha256': sha,
    }


if __name__ == '__main__':
    run()
