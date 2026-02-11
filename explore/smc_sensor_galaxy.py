#!/usr/bin/env python3
"""
SMC Sensor Galaxy — harvest entropy from ALL System Management Controller sensors.
Uses ioreg with grep pre-filtering for speed. No sudo required.
"""
import subprocess
import re
import time
import hashlib
import numpy as np
from collections import defaultdict

NUM_RE = re.compile(r'"([^"]{1,80})"\s*=\s*(\d{1,15})(?:\s|,|}|$)')


def sample_ioreg_fast():
    """Fast sample: pipe ioreg through grep to only get numeric lines."""
    # Use shell pipe for speed - grep only lines with = <number>
    result = subprocess.run(
        '/usr/sbin/ioreg -l -w0 | grep -E \'"[^"]+"\s*=\s*[0-9]\'',
        shell=True, capture_output=True, text=True, timeout=15
    )
    values = {}
    for m in NUM_RE.finditer(result.stdout):
        key, val = m.group(1), int(m.group(2))
        if 0 < val < 2**48:
            values[key] = val
    return values


def rapid_sample(n_samples=100, interval_s=0.1):
    """Rapidly sample ioreg."""
    print(f"[SMC] Rapid sampling {n_samples}x at {interval_s}s intervals...")
    all_samples = defaultdict(list)
    
    for i in range(n_samples):
        t0 = time.time()
        values = sample_ioreg_fast()
        elapsed = time.time() - t0
        for key, val in values.items():
            all_samples[key].append(val)
        wait = max(0, interval_s - elapsed)
        time.sleep(wait)
        if (i + 1) % 25 == 0:
            print(f"  Sample {i+1}/{n_samples} ({len(values)} values, {elapsed:.2f}s)")
    
    return dict(all_samples)


def find_fluctuating(samples_dict, min_samples=20):
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
                'std': np.std(arr),
                'mean': np.mean(arr),
            }
        else:
            static += 1
    
    return fluctuating, static


def extract_lsb_entropy(values):
    """Extract LSB noise from sensor values."""
    if len(values) < 10:
        return None
    arr = np.array(values, dtype=np.float64)
    k = min(5, len(arr) // 3)
    if k < 2:
        return None
    detrended = arr - np.convolve(arr, np.ones(k)/k, mode='same')
    noise = detrended[k:-k]
    if len(noise) < 5 or np.std(noise) == 0:
        return None
    normalized = (noise - noise.min()) / (noise.max() - noise.min() + 1e-30)
    return (normalized * 255).astype(np.uint8)


def cross_correlate(fluctuating, max_pairs=30):
    """Check independence between streams."""
    keys = sorted(fluctuating.keys(), key=lambda k: -fluctuating[k]['std'])[:max_pairs]
    n = len(keys)
    correlations = {}
    for i in range(n):
        for j in range(i+1, n):
            v1 = np.array(fluctuating[keys[i]]['values'], dtype=np.float64)
            v2 = np.array(fluctuating[keys[j]]['values'], dtype=np.float64)
            ml = min(len(v1), len(v2))
            if ml < 10:
                continue
            c = abs(np.corrcoef(v1[:ml], v2[:ml])[0, 1])
            if not np.isnan(c):
                correlations[f"{keys[i]} vs {keys[j]}"] = c
    return correlations


def run(output_file='explore/entropy_smc_galaxy.bin'):
    print("=" * 60)
    print("SMC SENSOR GALAXY — Multi-Sensor ADC Noise Explorer")
    print("=" * 60)
    
    # Phase 1: Rapid sampling
    print("\n[Phase 1] Rapid ioreg sampling...")
    samples = rapid_sample(n_samples=100, interval_s=0.1)
    print(f"  Tracked {len(samples)} unique keys")
    
    # Phase 2: Find fluctuating
    print("\n[Phase 2] Finding fluctuating values...")
    fluctuating, static = find_fluctuating(samples, min_samples=50)
    print(f"  {len(fluctuating)} fluctuating, {static} static")
    
    if not fluctuating:
        print("[FAIL] No fluctuating values found")
        return None
    
    # Phase 3: Extract entropy
    all_entropy = bytearray()
    results = []
    
    for key, info in sorted(fluctuating.items(), key=lambda x: -x[1]['std']):
        ent = extract_lsb_entropy(info['values'])
        if ent is not None and len(ent) > 0:
            all_entropy.extend(ent.tobytes())
            results.append({
                'sensor': key,
                'samples': len(info['values']),
                'unique': info['unique'],
                'std': info['std'],
                'entropy_bytes': len(ent),
            })
    
    print(f"\n{'Sensor':<50} {'Samp':>5} {'Uniq':>5} {'StdDev':>12} {'Bytes':>6}")
    print("-" * 80)
    for r in results[:25]:
        print(f"{r['sensor'][:50]:<50} {r['samples']:>5} {r['unique']:>5} "
              f"{r['std']:>12.1f} {r['bytes'] if 'bytes' in r else r['entropy_bytes']:>6}")
    
    # Phase 4: Cross-correlation
    print(f"\n[Phase 4] Cross-correlation...")
    corr = cross_correlate(fluctuating)
    if corr:
        independent = sum(1 for v in corr.values() if v < 0.2)
        correlated = sum(1 for v in corr.values() if v > 0.5)
        print(f"  {len(corr)} pairs: {independent} independent, {correlated} correlated")
    
    if not all_entropy:
        print("[FAIL] No entropy extracted")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] {len(all_entropy)} entropy bytes from {len(results)} sensors")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")
    
    return {
        'sensors': results,
        'total_bytes': len(all_entropy),
        'sha256': sha,
        'sensor_count': len(results),
    }


if __name__ == '__main__':
    run()
