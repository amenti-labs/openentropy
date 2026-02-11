#!/usr/bin/env python3
"""
Cross-Clock-Domain Beat Frequency — entropy from PLL phase noise interactions.

Apple Silicon has multiple independent clock domains (CPU, GPU, ANE, IO, memory).
When operations cross domains, the beat frequency of their independent PLLs
creates timing jitter that is physically random.
"""
import ctypes
import ctypes.util
import time
import hashlib
import os
import subprocess
import tempfile
import threading
import numpy as np

libsystem = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
libsystem.mach_absolute_time.restype = ctypes.c_uint64

def time_cpu_to_io(n_ops=1000):
    """Alternate CPU-bound and IO-bound operations, measure timing."""
    print(f"[Beat] CPU↔IO transitions ({n_ops} ops)...")
    timings = []
    tmp = tempfile.NamedTemporaryFile(delete=False)
    tmp.close()
    
    for i in range(n_ops):
        # CPU-bound burst
        x = 0
        for j in range(100):
            x = (x * 6364136223846793005 + 1) & 0xFFFFFFFFFFFFFFFF
        
        # IO-bound: write to disk
        start = libsystem.mach_absolute_time()
        with open(tmp.name, 'wb') as f:
            f.write(x.to_bytes(8, 'little'))
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    os.unlink(tmp.name)
    return timings

def time_cpu_to_memory(n_ops=2000):
    """Measure CPU-to-memory-controller crossing via cache eviction."""
    print(f"[Beat] CPU↔Memory transitions ({n_ops} ops)...")
    # Allocate large array to force cache misses
    big = np.random.randint(0, 256, size=16*1024*1024, dtype=np.uint8)  # 16MB > L2 cache
    
    timings = []
    for i in range(n_ops):
        # Random access pattern to force cache misses (memory controller involvement)
        idx = np.random.randint(0, len(big))
        
        start = libsystem.mach_absolute_time()
        _ = big[idx]  # cache miss → memory controller
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    return timings

def time_cpu_to_gpu(n_ops=100):
    """Time operations crossing CPU→GPU boundary."""
    print(f"[Beat] CPU↔GPU transitions ({n_ops} ops)...")
    timings = []
    
    # Use sips as GPU proxy
    tmp_in = tempfile.NamedTemporaryFile(suffix='.png', delete=False)
    tmp_out = tempfile.NamedTemporaryFile(suffix='.png', delete=False)
    tmp_in.close()
    tmp_out.close()
    
    # Create test image
    subprocess.run(['/usr/sbin/screencapture', '-x', '-R', '0,0,32,32', tmp_in.name],
                  capture_output=True, timeout=5)
    
    if not os.path.exists(tmp_in.name) or os.path.getsize(tmp_in.name) == 0:
        # Fallback: copy system image
        subprocess.run(['cp', '/System/Library/Desktop Pictures/Solid Colors/Black.png', tmp_in.name],
                      capture_output=True)
    
    for i in range(n_ops):
        # CPU work
        x = sum(range(1000))
        
        # GPU dispatch (via sips)
        start = libsystem.mach_absolute_time()
        subprocess.run(['/usr/bin/sips', '-z', '16', '16', tmp_in.name, '--out', tmp_out.name],
                      capture_output=True, timeout=5)
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    try: os.unlink(tmp_in.name)
    except: pass
    try: os.unlink(tmp_out.name)
    except: pass
    
    return timings

def time_cpu_to_usb(n_ops=200):
    """Time USB/Thunderbolt I/O operations."""
    print(f"[Beat] CPU↔USB/TB transitions ({n_ops} ops)...")
    timings = []
    
    for i in range(n_ops):
        start = libsystem.mach_absolute_time()
        # ioreg queries cross the IO domain boundary
        subprocess.run(['/usr/sbin/ioreg', '-c', 'IOUSBHostDevice', '-d', '1'],
                      capture_output=True, timeout=5)
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    return timings

def time_clock_domain_differences(n_samples=5000):
    """Measure difference between perf_counter_ns and monotonic_ns — different clock sources."""
    print(f"[Beat] Clock domain differences ({n_samples} samples)...")
    diffs = []
    for i in range(n_samples):
        t1 = time.perf_counter_ns()
        t2 = time.monotonic_ns()
        diffs.append(t1 - t2)
    return diffs


def time_interleaved_domains(n_rounds=500):
    """Rapidly interleave operations across all domains."""
    print(f"[Beat] Interleaved multi-domain timing ({n_rounds} rounds)...")
    timings = []
    big = np.random.randint(0, 256, size=4*1024*1024, dtype=np.uint8)
    
    for i in range(n_rounds):
        t0 = libsystem.mach_absolute_time()
        
        # CPU
        x = 0
        for j in range(50):
            x ^= j * 2654435761
        
        # Memory (cache miss)
        idx = np.random.randint(0, len(big))
        _ = big[idx]
        
        # Syscall (crosses kernel boundary)
        _ = os.getpid()
        
        t1 = libsystem.mach_absolute_time()
        timings.append(t1 - t0)
    
    return timings

def extract_beat_entropy(timings, label=""):
    """Extract entropy from cross-domain timing."""
    if len(timings) < 10:
        return np.array([], dtype=np.uint8)
    
    arr = np.array(timings, dtype=np.int64)
    lsbs = (np.abs(arr) & 0xFF).astype(np.uint8)
    
    # Also extract from consecutive XOR (highlights jitter)
    xored = np.bitwise_xor(arr[:-1], arr[1:])
    xor_lsbs = (np.abs(xored) & 0xFF).astype(np.uint8)
    
    combined = np.concatenate([lsbs, xor_lsbs])
    
    print(f"  [{label}] Mean: {np.mean(arr):.0f} ticks, Jitter: {np.std(arr):.0f}, Unique LSBs: {len(set(lsbs))}/256")
    return combined

def run(output_file='explore/entropy_cross_domain.bin'):
    print("=" * 60)
    print("CROSS-DOMAIN BEAT — PLL Phase Noise Interaction Entropy")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # CPU ↔ IO
    print("\n[Phase 1] CPU ↔ IO domain...")
    io_timings = time_cpu_to_io(1000)
    ent = extract_beat_entropy(io_timings, "CPU↔IO")
    all_entropy.extend(ent.tobytes())
    
    # CPU ↔ Memory
    print("\n[Phase 2] CPU ↔ Memory domain...")
    mem_timings = time_cpu_to_memory(2000)
    ent = extract_beat_entropy(mem_timings, "CPU↔Mem")
    all_entropy.extend(ent.tobytes())
    
    # CPU ↔ GPU
    print("\n[Phase 3] CPU ↔ GPU domain...")
    gpu_timings = time_cpu_to_gpu(50)
    ent = extract_beat_entropy(gpu_timings, "CPU↔GPU")
    all_entropy.extend(ent.tobytes())
    
    # Clock domain differences
    print("\n[Phase 4] Clock domain differences...")
    clock_diffs = time_clock_domain_differences(5000)
    ent = extract_beat_entropy(clock_diffs, "ClockDiff")
    all_entropy.extend(ent.tobytes())
    
    # Interleaved
    print("\n[Phase 5] Multi-domain interleaved...")
    interleaved = time_interleaved_domains(500)
    ent = extract_beat_entropy(interleaved, "Interleaved")
    all_entropy.extend(ent.tobytes())
    
    if not all_entropy:
        print("[FAIL] No entropy collected")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")
    
    return {'total_bytes': len(all_entropy), 'sha256': sha}

if __name__ == '__main__':
    run()
