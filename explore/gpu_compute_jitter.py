#!/usr/bin/env python3
"""
GPU Compute Jitter — timing entropy from GPU operations.

GPU shader execution is non-deterministic due to thermal throttling,
memory arbitration, and warp scheduling. We measure jitter in completion
times of GPU-involved operations.
"""
import subprocess
import time
import hashlib
import struct
import os
import tempfile
import numpy as np

def time_sips_operations(n_ops=200):
    """Time sips (GPU-accelerated image processing) repeatedly."""
    print(f"[GPU] Timing {n_ops} sips operations...")
    
    # Create a test image
    tmp = tempfile.NamedTemporaryFile(suffix='.png', delete=False)
    tmp_out = tempfile.NamedTemporaryFile(suffix='.png', delete=False)
    tmp.close()
    tmp_out.close()
    
    # Create test image with sips
    subprocess.run(['/usr/bin/sips', '-z', '64', '64', '/System/Library/Desktop Pictures/Solid Colors/Black.png',
                    '--out', tmp.name], capture_output=True, timeout=10)
    
    # If that fails, create with Python
    if not os.path.exists(tmp.name) or os.path.getsize(tmp.name) == 0:
        # Create a simple PNG manually
        try:
            from PIL import Image
            img = Image.new('RGB', (64, 64), color='red')
            img.save(tmp.name)
        except ImportError:
            # Create minimal test image via screencapture
            subprocess.run(['/usr/sbin/screencapture', '-x', '-R', '0,0,64,64', tmp.name],
                          capture_output=True, timeout=5)
    
    timings = []
    for i in range(n_ops):
        start = time.perf_counter_ns()
        subprocess.run(['/usr/bin/sips', '-z', '32', '32', tmp.name, '--out', tmp_out.name],
                      capture_output=True, timeout=5)
        elapsed = time.perf_counter_ns() - start
        timings.append(elapsed)
    
    os.unlink(tmp.name)
    try: os.unlink(tmp_out.name)
    except: pass
    
    return timings

def time_numpy_gpu_ops(n_ops=500):
    """Time numpy operations that may use Accelerate/GPU."""
    print(f"[GPU] Timing {n_ops} numpy matrix operations...")
    timings = []
    for i in range(n_ops):
        a = np.random.randn(256, 256).astype(np.float32)
        b = np.random.randn(256, 256).astype(np.float32)
        start = time.perf_counter_ns()
        c = np.dot(a, b)  # Uses Accelerate framework on macOS
        elapsed = time.perf_counter_ns() - start
        timings.append(elapsed)
    return timings

def time_metal_shader(n_ops=100):
    """Try to time Metal compute operations via pyobjc."""
    try:
        import Metal
        device = Metal.MTLCreateSystemDefaultDevice()
        if device is None:
            print("[GPU] No Metal device found")
            return []
        
        print(f"[GPU] Metal device: {device.name()}")
        queue = device.newCommandQueue()
        
        # Simple compute shader
        source = """
        #include <metal_stdlib>
        using namespace metal;
        kernel void entropy_kernel(device float *data [[buffer(0)]],
                                   uint id [[thread_position_in_grid]]) {
            float x = data[id];
            for (int i = 0; i < 100; i++) {
                x = sin(x * 1.1 + 0.1);
            }
            data[id] = x;
        }
        """
        
        library, error = device.newLibraryWithSource_options_error_(source, None, None)
        if error:
            print(f"[GPU] Metal compile error: {error}")
            return []
        
        func = library.newFunctionWithName_("entropy_kernel")
        pipeline, error = device.newComputePipelineStateWithFunction_error_(func, None)
        if error:
            print(f"[GPU] Pipeline error: {error}")
            return []
        
        # Create buffer
        import ctypes
        buf_size = 1024 * 4  # 1024 floats
        data = np.random.randn(1024).astype(np.float32)
        metal_buf = device.newBufferWithBytes_length_options_(
            data.tobytes(), buf_size, Metal.MTLResourceStorageModeShared)
        
        timings = []
        for i in range(n_ops):
            start = time.perf_counter_ns()
            cmd_buf = queue.commandBuffer()
            encoder = cmd_buf.computeCommandEncoder()
            encoder.setComputePipelineState_(pipeline)
            encoder.setBuffer_offset_atIndex_(metal_buf, 0, 0)
            
            threads_per_group = Metal.MTLSizeMake(min(pipeline.maxTotalThreadsPerThreadgroup(), 1024), 1, 1)
            grid_size = Metal.MTLSizeMake(1024, 1, 1)
            encoder.dispatchThreads_threadsPerThreadgroup_(grid_size, threads_per_group)
            encoder.endEncoding()
            cmd_buf.commit()
            cmd_buf.waitUntilCompleted()
            elapsed = time.perf_counter_ns() - start
            timings.append(elapsed)
        
        print(f"[GPU] Metal compute: {len(timings)} dispatches timed")
        return timings
    except ImportError:
        print("[GPU] Metal pyobjc not available")
        return []
    except Exception as e:
        print(f"[GPU] Metal error: {e}")
        return []

def extract_timing_entropy(timings, label=""):
    """Extract entropy from timing jitter."""
    if len(timings) < 10:
        return np.array([], dtype=np.uint8)
    
    arr = np.array(timings, dtype=np.float64)
    # Detrend
    detrended = arr - np.convolve(arr, np.ones(5)/5, mode='same')
    jitter = detrended[2:-2]  # trim convolution edges
    
    if np.std(jitter) == 0:
        return np.array([], dtype=np.uint8)
    
    # Normalize and quantize
    normalized = (jitter - jitter.min()) / (jitter.max() - jitter.min() + 1e-30)
    quantized = (normalized * 255).astype(np.uint8)
    
    print(f"  [{label}] Mean: {np.mean(arr)/1e6:.3f}ms, StdDev: {np.std(arr)/1e6:.3f}ms, Jitter: {np.std(jitter)/1e6:.3f}ms")
    
    return quantized

def run(output_file='explore/entropy_gpu_jitter.bin'):
    print("=" * 60)
    print("GPU COMPUTE JITTER — Timing Entropy from GPU Operations")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # Metal shader timing
    print("\n[Phase 1] Metal GPU compute timing...")
    metal_timings = time_metal_shader(200)
    if metal_timings:
        ent = extract_timing_entropy(metal_timings, "Metal")
        all_entropy.extend(ent.tobytes())
    
    # Numpy/Accelerate timing
    print("\n[Phase 2] NumPy/Accelerate matrix multiply timing...")
    np_timings = time_numpy_gpu_ops(500)
    ent = extract_timing_entropy(np_timings, "NumPy")
    all_entropy.extend(ent.tobytes())
    
    # sips GPU timing
    print("\n[Phase 3] sips GPU image processing timing...")
    sips_timings = time_sips_operations(100)
    if sips_timings:
        ent = extract_timing_entropy(sips_timings, "sips")
        all_entropy.extend(ent.tobytes())
    
    if not all_entropy:
        print("[FAIL] No timing data collected")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes")
    print(f"  SHA256: {sha[:32]}...")
    print(f"  Saved to: {output_file}")
    
    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")
    
    return {'total_bytes': len(all_entropy), 'sha256': sha}

if __name__ == '__main__':
    run()
