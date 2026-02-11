#!/usr/bin/env python3
"""
Harvest entropy from CPU temperature sensor jitter and timing noise.

Temperature sensors have ADC quantization noise and thermal Johnson noise.
The LSBs fluctuate even at steady state — this is physical randomness.
System call timing also has non-deterministic jitter from interrupts,
cache effects, and OS scheduling.
"""
import subprocess
import platform
import time
import numpy as np

def get_cpu_temp_macos():
    """Read CPU temp on macOS (requires osx-cpu-temp or similar)."""
    try:
        # Try powermetrics (needs sudo) or third-party tools
        result = subprocess.run(
            ['sudo', 'powermetrics', '--samplers', 'smc', '-i1', '-n1'],
            capture_output=True, text=True, timeout=5
        )
        for line in result.stdout.split('\n'):
            if 'CPU die temperature' in line:
                return float(line.split(':')[1].strip().replace(' C', ''))
    except Exception:
        pass
    return None

def timing_jitter_entropy(n_samples=1000):
    """
    Measure timing jitter of high-resolution clock calls.
    Non-deterministic due to interrupts, cache, scheduling.
    """
    print(f"Collecting {n_samples} timing jitter samples...")
    times = []
    for _ in range(n_samples):
        t1 = time.perf_counter_ns()
        t2 = time.perf_counter_ns()
        times.append(t2 - t1)
    
    arr = np.array(times)
    print(f"\nTiming jitter stats (nanoseconds):")
    print(f"  Mean: {np.mean(arr):.1f}")
    print(f"  Std:  {np.std(arr):.1f}")
    print(f"  Min:  {np.min(arr)}, Max: {np.max(arr)}")
    
    # Extract entropy from LSBs
    lsb_bits = np.bitwise_and(arr.astype(np.int64), 0x07)  # bottom 3 bits
    unique, counts = np.unique(lsb_bits, return_counts=True)
    probs = counts / len(lsb_bits)
    entropy = -np.sum(probs * np.log2(probs + 1e-10))
    print(f"  LSB(3bit) Shannon entropy: {entropy:.4f} / 3.0 bits")
    
    return arr, lsb_bits

def interrupt_timing_entropy(n_samples=500):
    """
    Sleep for tiny intervals — actual wake time has jitter
    from OS scheduling and hardware interrupts.
    """
    print(f"\nCollecting {n_samples} sleep-jitter samples...")
    jitters = []
    target_ns = 100_000  # 100μs target sleep
    
    for _ in range(n_samples):
        t1 = time.perf_counter_ns()
        time.sleep(target_ns / 1e9)
        t2 = time.perf_counter_ns()
        jitters.append(t2 - t1 - target_ns)
    
    arr = np.array(jitters)
    print(f"\nSleep jitter stats (ns over target):")
    print(f"  Mean: {np.mean(arr):.0f}")
    print(f"  Std:  {np.std(arr):.0f}")
    
    lsb_bits = np.bitwise_and(np.abs(arr).astype(np.int64), 0x0F)
    unique, counts = np.unique(lsb_bits, return_counts=True)
    probs = counts / len(lsb_bits)
    entropy = -np.sum(probs * np.log2(probs + 1e-10))
    print(f"  LSB(4bit) Shannon entropy: {entropy:.4f} / 4.0 bits")
    
    return arr, lsb_bits

if __name__ == '__main__':
    print("=== Sensor Jitter Entropy Explorer ===\n")
    
    print("--- Clock Jitter ---")
    timing_data, timing_bits = timing_jitter_entropy()
    
    print("\n--- Sleep/Interrupt Jitter ---")
    sleep_data, sleep_bits = interrupt_timing_entropy()
    
    # Combine sources
    combined = np.concatenate([timing_bits, sleep_bits])
    print(f"\n--- Combined ---")
    print(f"Total entropy samples: {len(combined)}")
    outfile = 'entropy_jitter.bin'
    combined.astype(np.uint8).tofile(outfile)
    print(f"Saved to {outfile}")
