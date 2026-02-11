#!/usr/bin/env python3
"""
Mach Timing Deep — kernel-level timing entropy via ctypes.

Direct access to mach_absolute_time(), thread scheduling jitter,
page fault timing, and kernel counter fluctuations.
"""
import ctypes
import ctypes.util
import time
import hashlib
import os
import mmap
import threading
import subprocess
import numpy as np

# Load system libraries
libc = ctypes.CDLL(ctypes.util.find_library('c'))
libsystem = ctypes.CDLL('/usr/lib/libSystem.B.dylib')

# mach_absolute_time returns uint64
libsystem.mach_absolute_time.restype = ctypes.c_uint64
libsystem.mach_absolute_time.argtypes = []

def sample_mach_absolute_time(n_samples=10000):
    """Sample mach_absolute_time() directly via ctypes."""
    print(f"[Mach] Sampling mach_absolute_time {n_samples} times...")
    times = np.zeros(n_samples, dtype=np.uint64)
    for i in range(n_samples):
        times[i] = libsystem.mach_absolute_time()
    deltas = np.diff(times)
    return deltas

def measure_scheduling_jitter(n_switches=2000):
    """Measure thread scheduling jitter via rapid context switches."""
    print(f"[Mach] Measuring scheduling jitter ({n_switches} switches)...")
    
    barrier = threading.Barrier(2)
    timings = []
    done = threading.Event()
    
    def worker():
        for _ in range(n_switches):
            if done.is_set():
                break
            try:
                barrier.wait(timeout=1)
            except threading.BrokenBarrierError:
                break
    
    thread = threading.Thread(target=worker, daemon=True)
    thread.start()
    
    for i in range(n_switches):
        start = libsystem.mach_absolute_time()
        try:
            barrier.wait(timeout=1)
        except threading.BrokenBarrierError:
            break
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    done.set()
    thread.join(timeout=2)
    return timings

def measure_page_fault_timing(n_faults=500):
    """Measure page fault timing by mapping/accessing/unmapping pages."""
    print(f"[Mach] Measuring page fault timing ({n_faults} faults)...")
    page_size = os.sysconf('SC_PAGE_SIZE')
    timings = []
    
    for i in range(n_faults):
        # Map anonymous pages
        mm = mmap.mmap(-1, page_size)
        
        start = libsystem.mach_absolute_time()
        mm[0] = 42  # First write triggers page fault
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
        
        mm.close()
    
    return timings

def measure_sysctl_jitter(n_samples=200):
    """Read kernel counters via sysctl and extract jitter."""
    print(f"[Mach] Reading kernel counters ({n_samples} samples)...")
    timings = []
    values = []
    
    for i in range(n_samples):
        start = libsystem.mach_absolute_time()
        result = subprocess.run(['sysctl', 'kern.boottime', 'vm.swapusage', 'kern.osrelease'],
                              capture_output=True, text=True, timeout=2)
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
        # The timing of the syscall itself is the entropy source
    
    return timings

def measure_pipe_ipc_jitter(n_ops=2000):
    """Measure IPC timing via pipes."""
    print(f"[Mach] Measuring pipe IPC jitter ({n_ops} ops)...")
    r, w = os.pipe()
    timings = []
    
    def writer():
        for _ in range(n_ops):
            os.write(w, b'x')
    
    thread = threading.Thread(target=writer, daemon=True)
    thread.start()
    
    for i in range(n_ops):
        start = libsystem.mach_absolute_time()
        os.read(r, 1)
        elapsed = libsystem.mach_absolute_time() - start
        timings.append(elapsed)
    
    thread.join(timeout=2)
    os.close(r)
    os.close(w)
    return timings

def extract_timing_entropy(timings, label="", use_lsb=True):
    """Extract entropy from timing measurements."""
    if len(timings) < 10:
        return np.array([], dtype=np.uint8)
    
    arr = np.array(timings, dtype=np.float64)
    
    if use_lsb:
        # Extract LSBs directly — most entropy is in the low bits
        int_arr = np.array(timings, dtype=np.uint64)
        lsbs = (int_arr & 0xFF).astype(np.uint8)
        print(f"  [{label}] Mean: {np.mean(arr):.1f} ticks, StdDev: {np.std(arr):.1f}, Unique LSBs: {len(set(lsbs))}/256")
        return lsbs
    else:
        detrended = arr - np.convolve(arr, np.ones(5)/5, mode='same')
        jitter = detrended[2:-2]
        if np.std(jitter) == 0:
            return np.array([], dtype=np.uint8)
        normalized = (jitter - jitter.min()) / (jitter.max() - jitter.min() + 1e-30)
        return (normalized * 255).astype(np.uint8)

def run(output_file='explore/entropy_mach_timing.bin'):
    print("=" * 60)
    print("MACH TIMING DEEP — Kernel-Level Timing Entropy")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # Direct mach_absolute_time deltas
    print("\n[Phase 1] mach_absolute_time delta LSBs...")
    deltas = sample_mach_absolute_time(10000)
    ent = extract_timing_entropy(deltas.tolist(), "mach_time")
    all_entropy.extend(ent.tobytes())
    
    # Thread scheduling jitter
    print("\n[Phase 2] Thread scheduling jitter...")
    sched_timings = measure_scheduling_jitter(1000)
    ent = extract_timing_entropy(sched_timings, "scheduling")
    all_entropy.extend(ent.tobytes())
    
    # Page fault timing
    print("\n[Phase 3] Page fault timing...")
    pf_timings = measure_page_fault_timing(500)
    ent = extract_timing_entropy(pf_timings, "page_fault")
    all_entropy.extend(ent.tobytes())
    
    # Pipe IPC jitter
    print("\n[Phase 4] Pipe IPC jitter...")
    ipc_timings = measure_pipe_ipc_jitter(2000)
    ent = extract_timing_entropy(ipc_timings, "pipe_ipc")
    all_entropy.extend(ent.tobytes())
    
    # Sysctl timing
    print("\n[Phase 5] Sysctl call timing...")
    sys_timings = measure_sysctl_jitter(100)
    ent = extract_timing_entropy(sys_timings, "sysctl")
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
    
    # Distribution analysis
    arr = np.frombuffer(bytes(all_entropy), dtype=np.uint8)
    hist = np.histogram(arr, bins=256, range=(0,256))[0]
    chi2 = np.sum((hist - len(arr)/256)**2 / (len(arr)/256))
    print(f"  Chi-squared: {chi2:.1f} (ideal ~255)")
    
    return {'total_bytes': len(all_entropy), 'sha256': sha}

if __name__ == '__main__':
    run()
