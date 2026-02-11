#!/usr/bin/env python3
"""
Harvest entropy from accelerometer / motion sensor noise.

On macOS, the Sudden Motion Sensor (SMS) / accelerometer can be accessed
via IOKit. Apple Silicon Macs may not have the traditional SMS, but
do have motion coprocessors.

This script attempts multiple access methods:
1. IOKit SMCMotionSensor (older Macs)
2. system_profiler for sensor data
3. CoreMotion via PyObjC (if available)
4. Fallback: synthetic MEMS noise model

MEMS accelerometers at rest exhibit Brownian noise from thermal
agitation of the proof mass — this is genuine physical randomness.
"""
import subprocess
import sys
import time
import numpy as np
import struct
import os


def try_sms_iokit():
    """Try to read Sudden Motion Sensor via IOKit (older Macs)."""
    try:
        import ctypes
        import ctypes.util
        
        iokit = ctypes.cdll.LoadLibrary(ctypes.util.find_library('IOKit'))
        cf = ctypes.cdll.LoadLibrary(ctypes.util.find_library('CoreFoundation'))
        
        # This only works on older Macs with SMS hardware
        # Apple Silicon doesn't have traditional SMS
        return None
    except Exception:
        return None


def try_coremotion_pyobjc():
    """Try CoreMotion via PyObjC (requires pyobjc-framework-CoreMotion)."""
    try:
        from CoreMotion import CMMotionManager
        manager = CMMotionManager.alloc().init()
        if manager.isAccelerometerAvailable():
            manager.setAccelerometerUpdateInterval_(0.01)  # 100Hz
            manager.startAccelerometerUpdates()
            time.sleep(0.1)
            
            samples = []
            for _ in range(500):
                data = manager.accelerometerData()
                if data:
                    acc = data.acceleration()
                    samples.append([acc.x, acc.y, acc.z])
                time.sleep(0.01)
            
            manager.stopAccelerometerUpdates()
            return np.array(samples) if samples else None
    except ImportError:
        pass
    except Exception as e:
        print(f"CoreMotion error: {e}")
    return None


def try_system_profiler():
    """Check what motion/sensor hardware is available."""
    try:
        result = subprocess.run(
            ['system_profiler', 'SPSPIDataType'],
            capture_output=True, text=True, timeout=10
        )
        sensors = []
        for line in result.stdout.split('\n'):
            lower = line.lower()
            if any(w in lower for w in ['accelerometer', 'motion', 'gyro', 'sensor']):
                sensors.append(line.strip())
        return sensors
    except Exception:
        return []


def synthetic_mems_noise(n_samples=1000, sample_rate=100):
    """Generate synthetic MEMS accelerometer noise for testing.
    
    Models:
    - Brownian noise (1/f² spectral density) — dominant at low freq
    - White noise floor (~200 μg/√Hz typical for consumer MEMS)
    - Quantization noise (12-bit ADC typical)
    - Bias instability drift
    
    Returns values in g units.
    """
    dt = 1.0 / sample_rate
    
    # White noise floor (200 μg/√Hz)
    noise_density = 200e-6  # g/√Hz
    white_noise = np.random.normal(0, noise_density * np.sqrt(sample_rate), (n_samples, 3))
    
    # Brownian / random walk component
    brown = np.cumsum(np.random.normal(0, 1e-6, (n_samples, 3)), axis=0)
    
    # Bias instability (slow drift)
    drift = np.cumsum(np.random.normal(0, 1e-8, (n_samples, 3)), axis=0)
    
    # Gravity vector (Z axis at rest) + noise
    gravity = np.zeros((n_samples, 3))
    gravity[:, 2] = 1.0  # 1g on Z axis
    
    # Quantization (12-bit, ±2g range)
    full_scale = 4.0  # ±2g
    lsb = full_scale / 4096
    
    raw = gravity + white_noise + brown + drift
    quantized = np.round(raw / lsb) * lsb
    
    return quantized


def extract_entropy(accel_data):
    """Extract entropy from accelerometer data LSBs."""
    # Convert to integer representation (simulating ADC output)
    # Assuming 12-bit ADC, ±2g range
    lsb = 4.0 / 4096
    int_data = np.round(accel_data / lsb).astype(np.int64)
    
    # Extract bottom 4 bits
    lsb_noise = np.bitwise_and(int_data, 0x0F)
    
    return lsb_noise, int_data


if __name__ == '__main__':
    print("=== Accelerometer Noise Entropy Explorer ===\n")
    
    # Check hardware
    print("Checking available sensors...")
    sensors = try_system_profiler()
    if sensors:
        print(f"  Found: {sensors}")
    else:
        print("  No motion sensors found via system_profiler")
    
    # Try real accelerometer
    print("\nAttempting CoreMotion access...")
    real_data = try_coremotion_pyobjc()
    
    if real_data is not None:
        print(f"  Got {len(real_data)} real accelerometer samples!")
        data = real_data
        source = "real_accelerometer"
    else:
        print("  CoreMotion not available (need pyobjc-framework-CoreMotion)")
        print("  Using synthetic MEMS noise model...")
        data = synthetic_mems_noise(n_samples=2000)
        source = "synthetic_mems"
    
    print(f"\nSource: {source}")
    print(f"Samples: {len(data)}")
    print(f"Shape: {data.shape}")
    
    # Stats per axis
    axes = ['X', 'Y', 'Z']
    for i, axis in enumerate(axes):
        col = data[:, i]
        print(f"\n  {axis}-axis:")
        print(f"    Mean: {np.mean(col):.6f} g")
        print(f"    Std:  {np.std(col):.6f} g")
        print(f"    Range: [{np.min(col):.6f}, {np.max(col):.6f}]")
    
    # Extract entropy
    lsb_noise, int_data = extract_entropy(data)
    print(f"\nLSB(4bit) noise extraction:")
    for i, axis in enumerate(axes):
        vals = lsb_noise[:, i]
        unique, counts = np.unique(vals, return_counts=True)
        probs = counts / len(vals)
        ent = -np.sum(probs * np.log2(probs + 1e-15))
        print(f"  {axis}: Shannon entropy = {ent:.4f} / 4.0 bits ({len(unique)} unique values)")
    
    # Save
    outfile = 'entropy_accelerometer.bin'
    lsb_noise.flatten().astype(np.uint8).tofile(outfile)
    print(f"\nSaved {lsb_noise.size} LSB samples to {outfile}")
    
    # Requirements note
    print(f"\n--- Requirements for Real Data ---")
    print(f"  pip3 install pyobjc-framework-CoreMotion")
    print(f"  Note: May need special entitlements on macOS for motion sensor access")
    print(f"  MacBooks have accelerometers; Mac Mini/Pro may not have user-accessible ones")
