#!/usr/bin/env python3
"""Phase 2: Probe novel entropy sources nobody has explored."""

import ctypes
import os
import time
import numpy as np

libsystem = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
libsystem.mach_absolute_time.restype = ctypes.c_uint64


def quick_entropy(data, label):
    """Quick Shannon entropy check."""
    if len(data) < 10:
        print(f"  [{label}] SKIP — only {len(data)} samples")
        return 0.0
    arr = np.array(data, dtype=np.uint8)
    vals, counts = np.unique(arr, return_counts=True)
    probs = counts / len(arr)
    ent = float(-np.sum(probs * np.log2(probs + 1e-15)))
    import zlib
    ratio = len(zlib.compress(arr.tobytes())) / len(arr)
    print(f"  [{label}] {len(data)} samples, Shannon={ent:.3f}/8.0, compress={ratio:.3f}, unique={len(vals)}/256")
    return ent


def probe_mach_port_timing(n=2000):
    """Mach port IPC round-trip jitter."""
    print("\n=== Mach Port Timing ===")
    # Use mach_task_self / mach_host_self as lightweight IPC
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
    lib.mach_task_self.restype = ctypes.c_uint32
    lib.mach_host_self.restype = ctypes.c_uint32

    timings = []
    for _ in range(n):
        t0 = libsystem.mach_absolute_time()
        _ = lib.mach_task_self()
        _ = lib.mach_host_self()
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "mach_port")


def probe_dyld_timing(n=1000):
    """dlopen/dlsym timing for shared library resolution."""
    print("\n=== dyld/Loader Timing ===")
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')

    libs_to_probe = [
        '/usr/lib/libz.dylib',
        '/usr/lib/libc++.dylib',
        '/usr/lib/libobjc.dylib',
        '/usr/lib/libSystem.B.dylib',
    ]
    timings = []
    for i in range(n):
        target = libs_to_probe[i % len(libs_to_probe)]
        t0 = libsystem.mach_absolute_time()
        try:
            h = ctypes.CDLL(target)
        except:
            pass
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "dyld")


def probe_xattr_timing(n=2000):
    """Filesystem extended attribute timing."""
    print("\n=== Filesystem xattr Timing ===")
    import subprocess
    # Use a file that exists
    test_file = '/usr/bin/true'
    timings = []
    for _ in range(n):
        t0 = libsystem.mach_absolute_time()
        try:
            os.listxattr(test_file)
        except:
            pass
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "xattr")


def probe_spotlight_timing(n=500):
    """Spotlight mdls query timing."""
    print("\n=== Spotlight Index Timing ===")
    import subprocess
    timings = []
    targets = ['/usr/bin/true', '/usr/bin/false', '/usr/bin/env', '/usr/bin/which']
    for i in range(n):
        t0 = libsystem.mach_absolute_time()
        subprocess.run(['/usr/bin/mdls', '-name', 'kMDItemFSName', targets[i % len(targets)]],
                      capture_output=True, timeout=5)
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "spotlight")


def probe_cg_window_timing(n=1000):
    """CoreGraphics window list timing."""
    print("\n=== CoreGraphics/WindowServer Timing ===")
    try:
        from Quartz import CGWindowListCopyWindowInfo, kCGWindowListOptionAll, kCGNullWindowID
    except ImportError:
        print("  [cg_window] SKIP — Quartz not available")
        return 0.0

    timings = []
    for _ in range(n):
        t0 = libsystem.mach_absolute_time()
        _ = CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID)
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "cg_window")


def probe_pipe_timing(n=3000):
    """Pipe buffer read/write timing."""
    print("\n=== Pipe Buffer Timing ===")
    timings = []
    for i in range(n):
        r, w = os.pipe()
        data = bytes([i & 0xFF]) * ((i % 64) + 1)
        t0 = libsystem.mach_absolute_time()
        os.write(w, data)
        _ = os.read(r, len(data))
        t1 = libsystem.mach_absolute_time()
        os.close(r)
        os.close(w)
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "pipe")


def probe_kqueue_timing(n=2000):
    """kqueue/kevent timing jitter."""
    print("\n=== kqueue/kevent Timing ===")
    import select
    timings = []
    for _ in range(n):
        kq = select.kqueue()
        t0 = libsystem.mach_absolute_time()
        # Poll with zero timeout
        try:
            events = kq.control([], 0, 0)
        except:
            pass
        t1 = libsystem.mach_absolute_time()
        kq.close()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "kqueue")


def probe_vm_page_timing(n=2000):
    """vm_page_info / mach VM timing."""
    print("\n=== VM Page Info Timing ===")
    import mmap
    timings = []
    for i in range(n):
        t0 = libsystem.mach_absolute_time()
        # mmap/munmap crosses VM subsystem
        m = mmap.mmap(-1, 4096)
        m[0] = i & 0xFF
        m.close()
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)
    return quick_entropy(timings, "vm_page")


def probe_dispatch_queue_timing(n=2000):
    """GCD dispatch queue scheduling jitter."""
    print("\n=== Dispatch Queue Contention ===")
    import threading
    import queue

    timings = []
    q = queue.Queue()

    def worker():
        while True:
            item = q.get()
            if item is None:
                break
            q.task_done()

    # Start worker threads to create contention
    threads = [threading.Thread(target=worker, daemon=True) for _ in range(4)]
    for t in threads:
        t.start()

    for i in range(n):
        t0 = libsystem.mach_absolute_time()
        q.put(i)
        q.join()
        t1 = libsystem.mach_absolute_time()
        timings.append((t1 - t0) & 0xFF)

    for _ in threads:
        q.put(None)

    return quick_entropy(timings, "dispatch_queue")


if __name__ == '__main__':
    print("=" * 60)
    print("NOVEL ENTROPY SOURCE EXPLORATION")
    print("=" * 60)

    results = {}
    results['mach_port'] = probe_mach_port_timing()
    results['dyld'] = probe_dyld_timing()
    results['xattr'] = probe_xattr_timing()
    results['spotlight'] = probe_spotlight_timing(200)
    results['cg_window'] = probe_cg_window_timing()
    results['pipe'] = probe_pipe_timing()
    results['kqueue'] = probe_kqueue_timing()
    results['vm_page'] = probe_vm_page_timing()
    results['dispatch_queue'] = probe_dispatch_queue_timing()

    print("\n" + "=" * 60)
    print("SUMMARY — Novel Sources (threshold: >3 bits/byte)")
    print("=" * 60)
    for name, ent in sorted(results.items(), key=lambda x: -x[1]):
        status = "✅ PROMISING" if ent > 3.0 else "❌ weak"
        print(f"  {name:<20} {ent:.3f}/8.0  {status}")
