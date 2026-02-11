#!/usr/bin/env python3
"""
Camera Quantum — photon shot noise at the quantum level.

At low light, photon arrival follows Poisson statistics — genuine quantum
randomness. Pixel-to-pixel variance IS quantum noise. We capture frames
and extract entropy from LSBs and frame-to-frame differences.
"""
import subprocess
import tempfile
import time
import hashlib
import os
import numpy as np

def capture_frame_imagesnap(output_path, warmup=False):
    """Capture a frame using imagesnap."""
    try:
        cmd = ['/opt/homebrew/bin/imagesnap', '-q', output_path]
        if warmup:
            cmd.extend(['-w', '1.0'])  # warmup time
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        return os.path.exists(output_path) and os.path.getsize(output_path) > 0
    except Exception as e:
        print(f"  imagesnap error: {e}")
        return False

def capture_frame_ffmpeg(output_path):
    """Capture a frame using ffmpeg."""
    try:
        result = subprocess.run([
            'ffmpeg', '-y', '-f', 'avfoundation', '-framerate', '30',
            '-i', '0', '-frames:v', '1', '-f', 'rawvideo',
            '-pix_fmt', 'rgb24', output_path
        ], capture_output=True, text=True, timeout=10)
        return os.path.exists(output_path) and os.path.getsize(output_path) > 0
    except Exception as e:
        print(f"  ffmpeg error: {e}")
        return False

def load_image_as_array(path):
    """Load image as numpy array."""
    try:
        from PIL import Image
        img = Image.open(path)
        return np.array(img)
    except ImportError:
        pass
    
    # Fallback: convert to raw PPM with sips
    ppm_path = path + '.ppm'
    try:
        subprocess.run(['/usr/bin/sips', '-s', 'format', 'ppm', path, '--out', ppm_path],
                      capture_output=True, timeout=10)
        with open(ppm_path, 'rb') as f:
            # Parse PPM header
            magic = f.readline().strip()
            while True:
                line = f.readline().strip()
                if not line.startswith(b'#'):
                    break
            w, h = map(int, line.split())
            maxval = int(f.readline().strip())
            data = np.frombuffer(f.read(), dtype=np.uint8)
            return data.reshape(h, w, 3)
    except Exception:
        pass
    finally:
        try: os.unlink(ppm_path)
        except: pass
    
    # Last resort: just read raw bytes
    with open(path, 'rb') as f:
        data = f.read()
    return np.frombuffer(data, dtype=np.uint8)

def capture_multiple_frames(n_frames=10):
    """Capture multiple frames for analysis."""
    print(f"[Camera] Capturing {n_frames} frames...")
    frames = []
    
    for i in range(n_frames):
        tmp = tempfile.NamedTemporaryFile(suffix='.jpg', delete=False)
        tmp.close()
        
        success = capture_frame_imagesnap(tmp.name, warmup=(i==0))
        if success:
            arr = load_image_as_array(tmp.name)
            if arr is not None and len(arr) > 0:
                frames.append(arr)
                if i == 0:
                    print(f"  Frame shape: {arr.shape}, dtype: {arr.dtype}")
        
        try: os.unlink(tmp.name)
        except: pass
        
        time.sleep(0.1)
    
    print(f"  Captured {len(frames)} frames")
    return frames

def analyze_spatial_noise(frame):
    """Analyze pixel-to-pixel variance (spatial noise)."""
    if frame.ndim < 2:
        return {}
    
    results = {}
    if frame.ndim == 3:
        for c, name in enumerate(['R', 'G', 'B']):
            channel = frame[:,:,c].astype(np.float64)
            # Local variance (3x3 blocks)
            h, w = channel.shape
            block_vars = []
            for y in range(0, h-3, 3):
                for x in range(0, w-3, 3):
                    block = channel[y:y+3, x:x+3]
                    block_vars.append(np.var(block))
            results[name] = {
                'mean': np.mean(channel),
                'std': np.std(channel),
                'local_var_mean': np.mean(block_vars),
                'local_var_std': np.std(block_vars),
            }
    else:
        results['gray'] = {'mean': np.mean(frame), 'std': np.std(frame)}
    
    return results

def analyze_temporal_noise(frames):
    """Analyze frame-to-frame differences (temporal noise)."""
    if len(frames) < 2:
        return {}
    
    # Ensure same shape
    shapes = [f.shape for f in frames]
    if len(set(shapes)) > 1:
        min_shape = tuple(min(s[i] for s in shapes) for i in range(len(shapes[0])))
        frames = [f[:min_shape[0], :min_shape[1]] if f.ndim >= 2 else f[:min_shape[0]] for f in frames]
    
    diffs = []
    for i in range(len(frames)-1):
        diff = frames[i+1].astype(np.int16) - frames[i].astype(np.int16)
        diffs.append(diff)
    
    all_diffs = np.concatenate([d.flatten() for d in diffs])
    return {
        'mean_diff': float(np.mean(np.abs(all_diffs))),
        'std_diff': float(np.std(all_diffs)),
        'nonzero_pct': float(np.count_nonzero(all_diffs) / len(all_diffs) * 100),
    }

def extract_camera_entropy(frames):
    """Extract entropy from camera frames."""
    all_entropy = bytearray()
    
    if not frames:
        return bytes()
    
    # LSBs of each frame
    for frame in frames:
        flat = frame.flatten()
        lsbs = flat & 0x03  # bottom 2 bits
        # Pack 4 values per byte
        packed = bytearray()
        for i in range(0, len(lsbs)-3, 4):
            byte = (lsbs[i] << 6) | (lsbs[i+1] << 4) | (lsbs[i+2] << 2) | lsbs[i+3]
            packed.append(byte)
        all_entropy.extend(packed)
    
    # Frame-to-frame XOR
    for i in range(len(frames)-1):
        if frames[i].shape == frames[i+1].shape:
            diff = np.bitwise_xor(frames[i].flatten(), frames[i+1].flatten())
            # Take non-zero diffs as entropy
            nonzero = diff[diff != 0]
            all_entropy.extend(nonzero.astype(np.uint8).tobytes()[:10000])
    
    return bytes(all_entropy)

def run(output_file='explore/entropy_camera_quantum.bin'):
    print("=" * 60)
    print("CAMERA QUANTUM — Photon Shot Noise Entropy")
    print("=" * 60)
    
    # Check for imagesnap
    try:
        subprocess.run(['/opt/homebrew/bin/imagesnap', '-l'], capture_output=True, timeout=5)
    except FileNotFoundError:
        print("[FAIL] imagesnap not found. Install: brew install imagesnap")
        return None
    
    # Capture frames
    print("\n[Phase 1] Capturing frames...")
    frames = capture_multiple_frames(n_frames=10)
    
    if not frames:
        print("[FAIL] Could not capture any frames")
        print("  This may require camera permissions or a connected camera")
        return None
    
    # Spatial noise analysis
    print("\n[Phase 2] Spatial noise analysis...")
    spatial = analyze_spatial_noise(frames[0])
    for channel, stats in spatial.items():
        print(f"  {channel}: mean={stats['mean']:.1f}, std={stats['std']:.2f}, local_var={stats.get('local_var_mean',0):.4f}")
    
    # Temporal noise analysis
    print("\n[Phase 3] Temporal noise analysis...")
    temporal = analyze_temporal_noise(frames)
    if temporal:
        print(f"  Mean frame diff: {temporal['mean_diff']:.2f}")
        print(f"  Diff std: {temporal['std_diff']:.2f}")
        print(f"  Non-zero pixels: {temporal['nonzero_pct']:.1f}%")
    
    # Extract entropy
    print("\n[Phase 4] Extracting entropy from LSBs and frame diffs...")
    entropy_data = extract_camera_entropy(frames)
    
    if not entropy_data:
        print("[FAIL] No entropy extracted")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(entropy_data)
    
    sha = hashlib.sha256(entropy_data).hexdigest()
    print(f"\n[RESULT] Collected {len(entropy_data)} entropy bytes from {len(frames)} frames")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    if len(entropy_data) > 100:
        ratio = len(zlib.compress(entropy_data)) / len(entropy_data)
        print(f"  Compression ratio: {ratio:.3f}")
    
    return {
        'total_bytes': len(entropy_data),
        'frames': len(frames),
        'spatial': spatial,
        'temporal': temporal,
        'sha256': sha,
    }

if __name__ == '__main__':
    run()
