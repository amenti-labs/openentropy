#!/usr/bin/env python3
"""
Harvest entropy from WiFi RSSI fluctuations.

WiFi signal strength varies due to multipath fading, interference,
and atmospheric effects. The micro-fluctuations contain genuine
environmental randomness.

macOS: Uses CoreWLAN via subprocess.
Linux: Uses iwconfig/iw.
"""
import subprocess
import platform
import time
import numpy as np
import sys

def get_rssi_macos():
    """Get current WiFi RSSI on macOS."""
    try:
        result = subprocess.run(
            ['/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport', '-I'],
            capture_output=True, text=True
        )
        for line in result.stdout.split('\n'):
            if 'agrCtlRSSI' in line:
                return int(line.split(':')[1].strip())
    except Exception as e:
        print(f"Error: {e}")
    return None

def get_rssi_linux():
    """Get current WiFi RSSI on Linux."""
    try:
        result = subprocess.run(['iwconfig'], capture_output=True, text=True)
        for line in result.stdout.split('\n'):
            if 'Signal level' in line:
                parts = line.split('Signal level=')[1]
                return int(parts.split(' ')[0])
    except Exception:
        pass
    return None

def collect_rssi_series(n_samples=200, interval=0.05):
    """Collect a time series of RSSI measurements."""
    get_rssi = get_rssi_macos if platform.system() == 'Darwin' else get_rssi_linux
    
    print(f"Collecting {n_samples} RSSI samples at {1/interval:.0f}Hz...")
    readings = []
    for i in range(n_samples):
        rssi = get_rssi()
        if rssi is not None:
            readings.append(rssi)
        time.sleep(interval)
        if (i + 1) % 50 == 0:
            print(f"  {i+1}/{n_samples}...")
    
    return np.array(readings)

def extract_entropy_from_rssi(readings):
    """Extract entropy from RSSI deltas."""
    deltas = np.diff(readings)
    # Use sign of differences as entropy bits
    bits = (deltas > 0).astype(np.uint8)
    
    # Von Neumann debias
    pairs = bits[:len(bits)//2*2].reshape(-1, 2)
    mask = pairs[:, 0] != pairs[:, 1]
    debiased = pairs[mask, 0]
    
    return deltas, bits, debiased

if __name__ == '__main__':
    print("=== WiFi RSSI Entropy Explorer ===\n")
    
    readings = collect_rssi_series()
    if len(readings) < 10:
        print("Not enough readings. Is WiFi connected?")
        sys.exit(1)
    
    print(f"\nRSSI stats:")
    print(f"  Samples: {len(readings)}")
    print(f"  Mean: {np.mean(readings):.1f} dBm")
    print(f"  Std:  {np.std(readings):.2f} dBm")
    print(f"  Range: {np.min(readings)} to {np.max(readings)} dBm")
    
    deltas, raw_bits, debiased = extract_entropy_from_rssi(readings)
    
    print(f"\nDelta distribution:")
    unique, counts = np.unique(deltas, return_counts=True)
    for v, c in zip(unique, counts):
        print(f"  {v:+d}: {'█' * c} ({c})")
    
    print(f"\nRaw bits: {len(raw_bits)} (bias: {np.mean(raw_bits):.3f})")
    print(f"Debiased: {len(debiased)} bits")
    if len(debiased) > 0:
        print(f"  Bias: {np.mean(debiased):.3f}")
