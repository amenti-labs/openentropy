#!/usr/bin/env python3
"""
NVMe SMART Jitter — entropy from storage timing and SMART attributes.

NAND cells have quantum tunneling effects in floating gates. Read timing
varies with cell voltage margins, temperature, and wear. SMART attributes
fluctuate due to background garbage collection and wear leveling.
"""
import subprocess
import time
import hashlib
import os
import re
import tempfile
import numpy as np
from collections import defaultdict

def read_smart_attributes():
    """Read SMART attributes from NVMe drive."""
    try:
        result = subprocess.run(['smartctl', '-a', '/dev/disk0'],
                              capture_output=True, text=True, timeout=10)
        attrs = {}
        for line in result.stdout.split('\n'):
            # Parse key: value pairs
            m = re.match(r'^(.+?):\s+([\d,.]+)', line.strip())
            if m:
                key = m.group(1).strip()
                try:
                    val = float(m.group(2).replace(',', ''))
                    attrs[key] = val
                except ValueError:
                    pass
        return attrs
    except FileNotFoundError:
        print("[NVMe] smartctl not found — install with: brew install smartmontools")
        return {}
    except Exception as e:
        print(f"[NVMe] SMART read error: {e}")
        return {}

def sample_smart_jitter(n_samples=30, interval_s=0.5):
    """Sample SMART attributes repeatedly, find fluctuating ones."""
    print(f"[NVMe] Sampling SMART attributes {n_samples} times...")
    all_samples = defaultdict(list)
    for i in range(n_samples):
        attrs = read_smart_attributes()
        for key, val in attrs.items():
            all_samples[key].append(val)
        time.sleep(interval_s)
        if (i+1) % 10 == 0:
            print(f"  Sample {i+1}/{n_samples}")
    return dict(all_samples)

def time_random_reads(n_reads=500, block_size=4096):
    """Time random file reads to extract I/O jitter."""
    print(f"[NVMe] Timing {n_reads} random reads ({block_size}B blocks)...")
    
    # Create a temp file to read from
    tmp = tempfile.NamedTemporaryFile(delete=False)
    tmp.write(os.urandom(1024 * 1024))  # 1MB
    tmp.close()
    
    timings = []
    try:
        fd = os.open(tmp.name, os.O_RDONLY)
        file_size = os.path.getsize(tmp.name)
        
        for i in range(n_reads):
            offset = np.random.randint(0, file_size - block_size)
            os.lseek(fd, offset, os.SEEK_SET)
            
            start = time.perf_counter_ns()
            data = os.read(fd, block_size)
            elapsed = time.perf_counter_ns() - start
            timings.append(elapsed)
        
        os.close(fd)
    finally:
        os.unlink(tmp.name)
    
    return timings

def time_direct_disk_reads(n_reads=100):
    """Time direct reads from /dev/disk0 (needs sudo)."""
    print(f"[NVMe] Timing {n_reads} direct disk reads...")
    timings = []
    for i in range(n_reads):
        start = time.perf_counter_ns()
        result = subprocess.run(
            ['sudo', 'dd', 'if=/dev/disk0', 'bs=512', 'count=1'],
            capture_output=True, timeout=5
        )
        elapsed = time.perf_counter_ns() - start
        if result.returncode == 0:
            timings.append(elapsed)
    return timings

def time_fsync_jitter(n_ops=200):
    """Time fsync operations — depends on NVMe firmware state."""
    print(f"[NVMe] Timing {n_ops} fsync operations...")
    tmp = tempfile.NamedTemporaryFile(delete=False)
    timings = []
    try:
        for i in range(n_ops):
            tmp.write(b'x' * 64)
            start = time.perf_counter_ns()
            os.fsync(tmp.fileno())
            elapsed = time.perf_counter_ns() - start
            timings.append(elapsed)
    finally:
        tmp.close()
        os.unlink(tmp.name)
    return timings

def extract_timing_entropy(timings, label=""):
    """Extract entropy from timing measurements."""
    if len(timings) < 10:
        return np.array([], dtype=np.uint8)
    arr = np.array(timings, dtype=np.float64)
    detrended = arr - np.convolve(arr, np.ones(5)/5, mode='same')
    jitter = detrended[2:-2]
    if np.std(jitter) == 0:
        return np.array([], dtype=np.uint8)
    normalized = (jitter - jitter.min()) / (jitter.max() - jitter.min() + 1e-30)
    quantized = (normalized * 255).astype(np.uint8)
    print(f"  [{label}] Mean: {np.mean(arr)/1e3:.1f}µs, Jitter StdDev: {np.std(jitter)/1e3:.1f}µs")
    return quantized

def run(output_file='explore/entropy_nvme_smart.bin'):
    print("=" * 60)
    print("NVMe SMART JITTER — Storage Timing & Attribute Entropy")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # SMART attribute sampling
    print("\n[Phase 1] SMART attribute sampling...")
    smart_data = sample_smart_jitter(n_samples=20, interval_s=0.3)
    fluctuating = {}
    for key, values in smart_data.items():
        if len(set(values)) > 1:
            fluctuating[key] = values
            std = np.std(values)
            print(f"  Fluctuating: {key} (std={std:.4f})")
    
    if fluctuating:
        for key, values in fluctuating.items():
            arr = np.array(values)
            noise = arr - np.mean(arr)
            if np.std(noise) > 0:
                norm = ((noise - noise.min()) / (noise.max() - noise.min() + 1e-30) * 255).astype(np.uint8)
                all_entropy.extend(norm.tobytes())
    else:
        print("  No fluctuating SMART attributes found (normal for short sampling)")
    
    # Random read timing
    print("\n[Phase 2] Random read timing...")
    read_timings = time_random_reads(500, 4096)
    ent = extract_timing_entropy(read_timings, "RandomRead")
    all_entropy.extend(ent.tobytes())
    
    # fsync timing
    print("\n[Phase 3] fsync timing...")
    fsync_timings = time_fsync_jitter(200)
    ent = extract_timing_entropy(fsync_timings, "fsync")
    all_entropy.extend(ent.tobytes())
    
    # Direct disk reads (try, needs sudo)
    print("\n[Phase 4] Direct disk reads...")
    try:
        dd_timings = time_direct_disk_reads(50)
        if dd_timings:
            ent = extract_timing_entropy(dd_timings, "DirectDisk")
            all_entropy.extend(ent.tobytes())
    except Exception as e:
        print(f"  Skipped: {e}")
    
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
