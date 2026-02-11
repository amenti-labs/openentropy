#!/usr/bin/env python3
"""
Harvest entropy from disk I/O latency jitter.

NVMe/SSD I/O timing varies due to:
- Flash cell read voltage variations
- Wear leveling decisions
- Garbage collection interrupts
- Controller queue state
- Thermal effects on NAND
- Read retry/ECC correction time

Also measures os.urandom() timing as a comparison baseline.
"""
import os
import time
import tempfile
import numpy as np


def measure_read_jitter(n_samples=1000, read_size=4096):
    """Measure jitter in small disk read operations.
    
    Reads from a temporary file to capture NVMe latency variations.
    """
    # Create a temp file with random data
    tmpfile = tempfile.NamedTemporaryFile(delete=False)
    tmpfile.write(os.urandom(1024 * 1024))  # 1MB
    tmpfile.flush()
    os.fsync(tmpfile.fileno())
    tmpfile.close()
    
    timings = []
    offsets = np.random.randint(0, 1024 * 1024 - read_size, size=n_samples)
    
    try:
        fd = os.open(tmpfile.name, os.O_RDONLY)
        
        for offset in offsets:
            # Seek to random position to avoid read-ahead cache
            os.lseek(fd, int(offset), os.SEEK_SET)
            
            t1 = time.perf_counter_ns()
            _ = os.read(fd, read_size)
            t2 = time.perf_counter_ns()
            
            timings.append(t2 - t1)
        
        os.close(fd)
    finally:
        os.unlink(tmpfile.name)
    
    return np.array(timings)


def measure_write_jitter(n_samples=500, write_size=4096):
    """Measure jitter in small disk write operations."""
    tmpfile = tempfile.NamedTemporaryFile(delete=False)
    tmpfile.close()
    
    timings = []
    data = os.urandom(write_size)
    
    try:
        fd = os.open(tmpfile.name, os.O_WRONLY)
        
        for _ in range(n_samples):
            t1 = time.perf_counter_ns()
            os.write(fd, data)
            os.fsync(fd)
            t2 = time.perf_counter_ns()
            timings.append(t2 - t1)
        
        os.close(fd)
    finally:
        os.unlink(tmpfile.name)
    
    return np.array(timings)


def measure_urandom_jitter(n_samples=1000, read_size=32):
    """Measure os.urandom() call timing as baseline.
    
    /dev/urandom timing includes kernel CSPRNG state + system call overhead.
    """
    timings = []
    for _ in range(n_samples):
        t1 = time.perf_counter_ns()
        _ = os.urandom(read_size)
        t2 = time.perf_counter_ns()
        timings.append(t2 - t1)
    
    return np.array(timings)


def extract_entropy(timings, n_bits=4):
    """Extract entropy from timing LSBs."""
    mask = (1 << n_bits) - 1
    lsb = np.bitwise_and(timings.astype(np.int64), mask)
    return lsb


def analyze_timings(timings, label):
    """Print timing analysis."""
    print(f"\n  {label}:")
    print(f"    Samples: {len(timings)}")
    print(f"    Mean: {np.mean(timings):.0f} ns")
    print(f"    Std: {np.std(timings):.0f} ns")
    print(f"    Min: {np.min(timings)} ns, Max: {np.max(timings)} ns")
    print(f"    CV: {np.std(timings)/np.mean(timings)*100:.1f}%")
    
    # LSB entropy
    lsb = extract_entropy(timings)
    unique, counts = np.unique(lsb, return_counts=True)
    probs = counts / len(lsb)
    ent = -np.sum(probs * np.log2(probs + 1e-15))
    print(f"    LSB(4bit) Shannon entropy: {ent:.4f} / 4.0 bits")
    
    return lsb


if __name__ == '__main__':
    print("=== Disk I/O Jitter Entropy Explorer ===\n")
    
    print("--- Read Latency Jitter ---")
    read_timings = measure_read_jitter(n_samples=1000)
    read_lsb = analyze_timings(read_timings, "Random 4KB reads")
    
    print("\n--- Write Latency Jitter ---")
    write_timings = measure_write_jitter(n_samples=500)
    write_lsb = analyze_timings(write_timings, "4KB writes + fsync")
    
    print("\n--- os.urandom() Timing (baseline) ---")
    urandom_timings = measure_urandom_jitter(n_samples=1000)
    urandom_lsb = analyze_timings(urandom_timings, "os.urandom(32)")
    
    # Combine all sources
    combined = np.concatenate([read_lsb, write_lsb, urandom_lsb])
    
    outfile = 'entropy_disk_io.bin'
    combined.astype(np.uint8).tofile(outfile)
    print(f"\nSaved {len(combined)} combined LSB samples to {outfile}")
