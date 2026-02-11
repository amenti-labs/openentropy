#!/usr/bin/env python3
"""
Harvest entropy from memory access timing variations.

Memory timing jitter comes from:
- DRAM refresh cycles (every ~64ms, causes access delays)
- Cache miss timing (L1 vs L2 vs L3 vs main memory)
- TLB misses
- Memory controller scheduling
- Row buffer hits vs misses
- Thermal effects on DRAM timing margins

These variations are physical in nature and difficult to predict.
"""
import time
import os
import ctypes
import numpy as np


def measure_allocation_timing(n_samples=1000, alloc_size=4096):
    """Measure memory allocation timing variations.
    
    malloc() timing varies due to heap state, page faults,
    and memory pressure.
    """
    timings = []
    for _ in range(n_samples):
        t1 = time.perf_counter_ns()
        buf = bytearray(alloc_size)
        t2 = time.perf_counter_ns()
        timings.append(t2 - t1)
        del buf
    
    return np.array(timings)


def measure_cache_miss_timing(n_samples=1000):
    """Measure cache miss timing by accessing random memory locations.
    
    Random access pattern causes L1/L2/L3 cache misses,
    revealing memory hierarchy timing.
    """
    # Allocate a large array (larger than L3 cache, ~12MB on M1)
    size = 32 * 1024 * 1024  # 32MB
    arr = np.zeros(size // 8, dtype=np.int64)
    
    # Random indices to force cache misses
    indices = np.random.randint(0, len(arr), size=n_samples)
    
    timings = []
    for idx in indices:
        t1 = time.perf_counter_ns()
        _ = arr[idx]
        t2 = time.perf_counter_ns()
        timings.append(t2 - t1)
    
    del arr
    return np.array(timings)


def measure_sequential_vs_random(n_samples=500):
    """Compare sequential vs random access timing to detect cache effects."""
    size = 16 * 1024 * 1024  # 16MB
    arr = np.zeros(size // 8, dtype=np.int64)
    
    # Sequential access (should hit cache)
    seq_timings = []
    for i in range(n_samples):
        idx = i % len(arr)
        t1 = time.perf_counter_ns()
        _ = arr[idx]
        t2 = time.perf_counter_ns()
        seq_timings.append(t2 - t1)
    
    # Random access (cache misses)
    rand_indices = np.random.randint(0, len(arr), size=n_samples)
    rand_timings = []
    for idx in rand_indices:
        t1 = time.perf_counter_ns()
        _ = arr[idx]
        t2 = time.perf_counter_ns()
        rand_timings.append(t2 - t1)
    
    del arr
    return np.array(seq_timings), np.array(rand_timings)


def measure_dram_refresh_effect(duration_sec=1.0):
    """Try to detect DRAM refresh timing (~64ms period).
    
    During DRAM refresh, access latency increases slightly.
    By measuring a continuous stream of accesses, we can detect
    periodic latency spikes that correlate with refresh cycles.
    """
    size = 32 * 1024 * 1024
    arr = np.zeros(size // 8, dtype=np.int64)
    
    timings = []
    timestamps = []
    start = time.perf_counter_ns()
    end = start + int(duration_sec * 1e9)
    
    indices = np.random.randint(0, len(arr), size=100000)
    i = 0
    while time.perf_counter_ns() < end and i < len(indices):
        t1 = time.perf_counter_ns()
        _ = arr[indices[i]]
        t2 = time.perf_counter_ns()
        timings.append(t2 - t1)
        timestamps.append(t1 - start)
        i += 1
    
    del arr
    return np.array(timings), np.array(timestamps)


def extract_entropy(timings, n_bits=4):
    """Extract entropy from timing LSBs."""
    mask = (1 << n_bits) - 1
    return np.bitwise_and(timings.astype(np.int64), mask)


def analyze(timings, label):
    """Analyze and print timing stats."""
    print(f"\n  {label}:")
    print(f"    Samples: {len(timings)}")
    print(f"    Mean: {np.mean(timings):.0f} ns")
    print(f"    Std: {np.std(timings):.0f} ns")
    print(f"    Min: {np.min(timings)} ns, Max: {np.max(timings)} ns")
    
    lsb = extract_entropy(timings)
    unique, counts = np.unique(lsb, return_counts=True)
    probs = counts / len(lsb)
    ent = -np.sum(probs * np.log2(probs + 1e-15))
    print(f"    LSB(4bit) entropy: {ent:.4f} / 4.0 bits")
    return lsb


if __name__ == '__main__':
    print("=== Memory Timing Entropy Explorer ===\n")
    
    print("--- Memory Allocation Timing ---")
    alloc_timings = measure_allocation_timing(n_samples=1000)
    alloc_lsb = analyze(alloc_timings, "malloc(4096)")
    
    print("\n--- Cache Miss Timing ---")
    miss_timings = measure_cache_miss_timing(n_samples=1000)
    miss_lsb = analyze(miss_timings, "Random 32MB access")
    
    print("\n--- Sequential vs Random Access ---")
    seq_t, rand_t = measure_sequential_vs_random(n_samples=500)
    seq_lsb = analyze(seq_t, "Sequential access")
    rand_lsb = analyze(rand_t, "Random access")
    
    print("\n--- DRAM Refresh Detection ---")
    refresh_t, timestamps = measure_dram_refresh_effect(duration_sec=0.5)
    refresh_lsb = analyze(refresh_t, "Continuous random access")
    
    # Check for ~64ms periodicity in latency spikes
    if len(refresh_t) > 100:
        # Find outlier timings (potential refresh-induced delays)
        threshold = np.mean(refresh_t) + 2 * np.std(refresh_t)
        spikes = timestamps[refresh_t > threshold]
        if len(spikes) > 2:
            intervals = np.diff(spikes) / 1e6  # Convert to ms
            print(f"    Spike intervals: mean={np.mean(intervals):.1f}ms, std={np.std(intervals):.1f}ms")
            if 50 < np.mean(intervals) < 80:
                print(f"    ⚡ Possible DRAM refresh signature detected!")
    
    # Combine all
    combined = np.concatenate([alloc_lsb, miss_lsb, rand_lsb, refresh_lsb])
    outfile = 'entropy_memory_timing.bin'
    combined.astype(np.uint8).tofile(outfile)
    print(f"\nSaved {len(combined)} combined LSB samples to {outfile}")
