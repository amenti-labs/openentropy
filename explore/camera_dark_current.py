#!/usr/bin/env python3
"""
Harvest entropy from camera sensor dark current noise.

Camera sensors exhibit dark current — thermally generated electrons that
create noise even with no light. The LSBs of pixel values in a dark frame
contain genuine physical randomness from:
- Shot noise (Poisson statistics of electron generation)
- Read noise (amplifier thermal noise)
- Hot pixels (radiation damage sites)

Works best with lens cap on or in a dark room, but ambient light LSBs
also contain sensor noise overlaid on the scene.
"""
import numpy as np
import subprocess
import sys
import os
import time
import json


def capture_frame_macos():
    """Capture a frame using macOS imagesnap or ffmpeg."""
    tmpfile = '/tmp/esoteric_dark_frame.jpg'
    
    # Try imagesnap first
    try:
        result = subprocess.run(
            ['/opt/homebrew/bin/imagesnap', '-w', '1', tmpfile],
            capture_output=True, text=True, timeout=10
        )
        if os.path.exists(tmpfile):
            return tmpfile
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    
    # Try ffmpeg with AVFoundation
    try:
        result = subprocess.run(
            ['ffmpeg', '-f', 'avfoundation', '-framerate', '30',
             '-i', '0', '-frames:v', '1', '-y', tmpfile],
            capture_output=True, text=True, timeout=10
        )
        if os.path.exists(tmpfile):
            return tmpfile
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    
    return None


def load_image(path):
    """Load image as numpy array. Try multiple backends."""
    try:
        import cv2
        img = cv2.imread(path)
        if img is not None:
            return img
    except ImportError:
        pass
    
    try:
        from PIL import Image
        img = np.array(Image.open(path))
        return img
    except ImportError:
        pass
    
    raise ImportError("Need opencv-python or Pillow: pip3 install opencv-python-headless Pillow")


def extract_lsb_noise(image, n_bits=2):
    """Extract LSB noise from image pixels.
    
    Args:
        image: numpy array (H, W, C) uint8
        n_bits: number of least significant bits to extract
    """
    mask = (1 << n_bits) - 1
    lsb = np.bitwise_and(image.astype(np.uint8), mask)
    return lsb


def analyze_dark_frame(image, label="dark_frame"):
    """Analyze a single frame for entropy content."""
    results = {'label': label, 'shape': list(image.shape)}
    
    # Per-channel analysis
    if len(image.shape) == 3:
        channel_names = ['Blue', 'Green', 'Red'] if image.shape[2] == 3 else ['Ch0', 'Ch1', 'Ch2', 'Ch3']
        for i, name in enumerate(channel_names[:image.shape[2]]):
            ch = image[:, :, i]
            lsb1 = np.bitwise_and(ch, 1)
            lsb2 = np.bitwise_and(ch, 3)
            
            results[name] = {
                'mean': float(np.mean(ch)),
                'std': float(np.std(ch)),
                'lsb1_bias': float(np.mean(lsb1)),  # ideal: 0.5
                'lsb2_entropy': float(shannon_entropy_quick(lsb2)),
                'unique_values': int(len(np.unique(ch))),
            }
    else:
        ch = image
        lsb2 = np.bitwise_and(ch, 3)
        results['Gray'] = {
            'mean': float(np.mean(ch)),
            'std': float(np.std(ch)),
            'lsb1_bias': float(np.mean(np.bitwise_and(ch, 1))),
            'lsb2_entropy': float(shannon_entropy_quick(lsb2)),
        }
    
    # Overall LSB extraction
    lsb = extract_lsb_noise(image, n_bits=2)
    results['total_lsb_samples'] = int(lsb.size)
    results['bits_available'] = int(lsb.size * 2)  # 2 bits per sample
    
    return results, lsb.flatten()


def shannon_entropy_quick(data):
    """Quick Shannon entropy calculation."""
    data = np.asarray(data).flatten()
    _, counts = np.unique(data, return_counts=True)
    probs = counts / len(data)
    return float(-np.sum(probs * np.log2(probs + 1e-15)))


def generate_synthetic_dark_frame(height=480, width=640):
    """Generate a synthetic dark frame for testing when no camera is available.
    
    Models:
    - Base dark current (Poisson, ~5 electrons)
    - Read noise (Gaussian, ~3 ADU)
    - Hot pixels (~0.1% of pixels, much brighter)
    """
    # Dark current (Poisson noise)
    dark_current = np.random.poisson(5, (height, width, 3)).astype(np.float64)
    
    # Read noise (Gaussian)
    read_noise = np.random.normal(0, 3, (height, width, 3))
    
    # Hot pixels
    hot_mask = np.random.random((height, width, 3)) < 0.001
    hot_values = np.random.randint(50, 255, (height, width, 3))
    
    frame = dark_current + read_noise
    frame[hot_mask] = hot_values[hot_mask]
    frame = np.clip(frame, 0, 255).astype(np.uint8)
    
    return frame


if __name__ == '__main__':
    print("=== Camera Dark Current Entropy Explorer ===\n")
    
    # Try to capture a real frame
    print("Attempting camera capture...")
    frame_path = capture_frame_macos()
    
    if frame_path:
        print(f"Captured frame: {frame_path}")
        try:
            image = load_image(frame_path)
            print(f"Image shape: {image.shape}")
            results, lsb_data = analyze_dark_frame(image, "real_capture")
        except ImportError as e:
            print(f"Image loading failed: {e}")
            print("Falling back to synthetic dark frame...")
            image = generate_synthetic_dark_frame()
            results, lsb_data = analyze_dark_frame(image, "synthetic_dark")
    else:
        print("No camera available — using synthetic dark frame model")
        image = generate_synthetic_dark_frame()
        results, lsb_data = analyze_dark_frame(image, "synthetic_dark")
    
    print(f"\nFrame: {results['label']} ({results['shape']})")
    for key, val in results.items():
        if isinstance(val, dict):
            print(f"\n  {key}:")
            for k, v in val.items():
                print(f"    {k}: {v}")
    
    print(f"\n  Total LSB samples: {results['total_lsb_samples']:,}")
    print(f"  Bits available: {results['bits_available']:,}")
    
    # Save entropy data
    outfile = 'entropy_camera_dark.bin'
    lsb_data.astype(np.uint8).tofile(outfile)
    print(f"\nSaved {len(lsb_data)} LSB samples to {outfile}")
    
    # Quick entropy check
    ent = shannon_entropy_quick(lsb_data)
    print(f"Overall LSB(2bit) Shannon entropy: {ent:.4f} / 2.0 bits")
