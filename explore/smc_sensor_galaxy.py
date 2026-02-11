#!/usr/bin/env python3
"""
SMC Sensor Galaxy — harvest entropy from ALL System Management Controller sensors.

Apple Silicon exposes hundreds of sensor keys via powermetrics and ioreg.
Each ADC reading contains quantization noise + thermal noise. With dozens
of independent sensors sampled rapidly, we get parallel entropy streams.
"""
import subprocess
import re
import time
import hashlib
import struct
import numpy as np
from collections import defaultdict

def parse_powermetrics(n_samples=20, interval_ms=100):
    """Run powermetrics and parse ALL sensor values."""
    print(f"[SMC] Running powermetrics ({n_samples} samples, {interval_ms}ms interval)...")
    try:
        result = subprocess.run(
            ['sudo', 'powermetrics', '--samplers', 'smc', '-i', str(interval_ms), '-n', str(n_samples)],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode != 0:
            print(f"[SMC] powermetrics failed: {result.stderr[:200]}")
            return None
        return result.stdout
    except subprocess.TimeoutExpired:
        print("[SMC] powermetrics timed out")
        return None
    except Exception as e:
        print(f"[SMC] powermetrics error: {e}")
        return None

def parse_ioreg_sensors():
    """Parse ioreg for all numeric sensor-like values."""
    print("[SMC] Parsing ioreg for sensor values...")
    try:
        result = subprocess.run(
            ['ioreg', '-l', '-w0'],
            capture_output=True, text=True, timeout=15
        )
        # Find all numeric values that look like sensor readings
        # Pattern: "key" = <number> or "key" = number
        readings = {}
        for line in result.stdout.split('\n'):
            # Match temperature, voltage, current, power patterns
            for pattern in [
                r'"([^"]*(?:temp|volt|curr|power|watt|amp|fan|speed|therm|sensor)[^"]*)".*?=\s*(\d+\.?\d*)',
                r'"([^"]*)".*?=\s*(\d+\.\d{2,})',  # floats with 2+ decimals (likely sensor readings)
            ]:
                matches = re.finditer(pattern, line, re.IGNORECASE)
                for m in matches:
                    key, val = m.group(1), float(m.group(2))
                    if 0.001 < val < 100000:  # reasonable sensor range
                        readings[key] = val
        return readings
    except Exception as e:
        print(f"[SMC] ioreg error: {e}")
        return {}

def parse_smc_block(text):
    """Parse a single powermetrics SMC sample block into sensor dict."""
    sensors = {}
    # Match lines like: "CPU die temperature: 42.56 C" or "GPU Power: 1.23 mW"
    patterns = [
        r'(.+?):\s*([\d.]+)\s*(C|W|mW|V|mV|A|mA|RPM|%)',
    ]
    for line in text.split('\n'):
        for pat in patterns:
            m = re.match(pat, line.strip())
            if m:
                name = m.group(1).strip()
                value = float(m.group(2))
                unit = m.group(3)
                # Convert to base units
                if unit == 'mW': value /= 1000
                elif unit == 'mV': value /= 1000
                elif unit == 'mA': value /= 1000
                sensors[name] = value
    return sensors

def multi_sample_powermetrics(n_rounds=5, samples_per_round=20, interval_ms=100):
    """Collect multiple rounds of powermetrics data."""
    all_samples = defaultdict(list)
    for i in range(n_rounds):
        raw = parse_powermetrics(samples_per_round, interval_ms)
        if raw is None:
            continue
        # Split into sample blocks (separated by "*** Sampled system activity")
        blocks = re.split(r'\*+\s*Sampled system activity.*?\*+', raw)
        for block in blocks:
            sensors = parse_smc_block(block)
            for key, val in sensors.items():
                all_samples[key].append(val)
    return dict(all_samples)

def multi_sample_ioreg(n_samples=50, interval_s=0.1):
    """Rapidly sample ioreg for fluctuating values."""
    all_samples = defaultdict(list)
    for i in range(n_samples):
        readings = parse_ioreg_sensors()
        for key, val in readings.items():
            all_samples[key].append(val)
        time.sleep(interval_s)
        if (i+1) % 10 == 0:
            print(f"  [ioreg] sample {i+1}/{n_samples}")
    return dict(all_samples)

def extract_lsb_entropy(values, bits=8):
    """Extract LSB noise from a series of float sensor values."""
    if len(values) < 10:
        return None, 0
    arr = np.array(values)
    # Get the fractional noise
    mean_val = np.mean(arr)
    if mean_val == 0:
        return None, 0
    # Detrend
    detrended = arr - np.convolve(arr, np.ones(5)/5, mode='same')
    # Scale to use LSBs
    noise_range = np.std(detrended)
    if noise_range == 0:
        return None, 0
    # Quantize noise to bits
    normalized = ((detrended - detrended.min()) / (detrended.max() - detrended.min() + 1e-30) * (2**bits - 1)).astype(np.uint8)
    return normalized, noise_range

def cross_correlate_sources(samples_dict):
    """Check independence between sensor entropy streams."""
    keys = list(samples_dict.keys())
    n = len(keys)
    if n < 2:
        return {}
    correlations = {}
    for i in range(min(n, 20)):  # limit to 20 for speed
        for j in range(i+1, min(n, 20)):
            v1 = np.array(samples_dict[keys[i]])
            v2 = np.array(samples_dict[keys[j]])
            min_len = min(len(v1), len(v2))
            if min_len < 10:
                continue
            corr = abs(np.corrcoef(v1[:min_len], v2[:min_len])[0,1])
            if not np.isnan(corr):
                correlations[f"{keys[i]} vs {keys[j]}"] = corr
    return correlations

def run(output_file='explore/entropy_smc_galaxy.bin'):
    """Main exploration routine."""
    print("=" * 60)
    print("SMC SENSOR GALAXY — Multi-Sensor ADC Noise Explorer")
    print("=" * 60)
    
    all_entropy = bytearray()
    all_samples = {}
    
    # Try powermetrics first (needs sudo)
    print("\n[Phase 1] powermetrics sampling...")
    pm_samples = multi_sample_powermetrics(n_rounds=3, samples_per_round=20, interval_ms=100)
    if pm_samples:
        print(f"  Found {len(pm_samples)} sensor streams via powermetrics")
        all_samples.update(pm_samples)
    else:
        print("  powermetrics unavailable, falling back to ioreg only")
    
    # Always also try ioreg (no sudo needed)
    print("\n[Phase 2] ioreg rapid sampling...")
    ioreg_samples = multi_sample_ioreg(n_samples=30, interval_s=0.1)
    if ioreg_samples:
        print(f"  Found {len(ioreg_samples)} value streams via ioreg")
        all_samples.update(ioreg_samples)
    
    if not all_samples:
        print("\n[FAIL] No sensor data available!")
        return None
    
    # Extract entropy from each sensor
    print(f"\n[Phase 3] Extracting LSB entropy from {len(all_samples)} streams...")
    results = []
    for key, values in sorted(all_samples.items()):
        entropy_bytes, noise_range = extract_lsb_entropy(values)
        if entropy_bytes is not None and len(entropy_bytes) > 0:
            all_entropy.extend(entropy_bytes)
            # Estimate entropy: unique values / possible values
            unique_ratio = len(set(entropy_bytes.tobytes())) / 256
            results.append({
                'sensor': key,
                'samples': len(values),
                'noise_range': noise_range,
                'entropy_bytes': len(entropy_bytes),
                'unique_ratio': unique_ratio,
                'mean': np.mean(values),
                'std': np.std(values),
            })
    
    # Sort by quality
    results.sort(key=lambda x: x['unique_ratio'], reverse=True)
    
    print(f"\n{'Sensor':<40} {'Samples':>8} {'StdDev':>10} {'Unique%':>8}")
    print("-" * 70)
    for r in results[:30]:  # top 30
        print(f"{r['sensor'][:40]:<40} {r['samples']:>8} {r['std']:>10.6f} {r['unique_ratio']*100:>7.1f}%")
    
    # Cross-correlation analysis
    print(f"\n[Phase 4] Cross-correlation analysis...")
    # Use only sensors with enough samples
    good_sensors = {k: v for k, v in all_samples.items() if len(v) >= 10}
    correlations = cross_correlate_sources(good_sensors)
    if correlations:
        high_corr = {k: v for k, v in correlations.items() if v > 0.5}
        low_corr = {k: v for k, v in correlations.items() if v < 0.2}
        print(f"  {len(correlations)} pairs analyzed")
        print(f"  {len(low_corr)} pairs with low correlation (<0.2) — GOOD (independent)")
        print(f"  {len(high_corr)} pairs with high correlation (>0.5) — correlated")
        if high_corr:
            print("  Highly correlated pairs:")
            for k, v in sorted(high_corr.items(), key=lambda x: -x[1])[:5]:
                print(f"    {v:.3f}: {k}")
    
    # Compress and hash for final entropy
    entropy_hash = hashlib.sha256(bytes(all_entropy)).digest()
    
    # Save raw entropy
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes from {len(results)} sensors")
    print(f"  SHA256: {entropy_hash.hex()[:32]}...")
    print(f"  Saved to: {output_file}")
    
    # Compression test
    import zlib
    if len(all_entropy) > 100:
        compressed = zlib.compress(bytes(all_entropy))
        ratio = len(compressed) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f} (1.0 = incompressible = good entropy)")
    
    return {
        'sensors': results,
        'total_bytes': len(all_entropy),
        'sha256': entropy_hash.hex(),
        'correlations': correlations,
    }

if __name__ == '__main__':
    run()
