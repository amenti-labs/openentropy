#!/usr/bin/env python3
"""
Harvest entropy from CPU thermal sensor jitter.

CPU die temperature sensors have ADC quantization noise and genuine
thermal fluctuations. The LSBs change unpredictably due to:
- Thermal Johnson noise in the sensor
- Rapid workload changes affecting die temperature
- ADC conversion noise

On macOS: Uses powermetrics (needs sudo) or osx-cpu-temp.
Falls back to timing-based thermal proxy if neither available.
"""
import subprocess
import time
import sys
import os
import numpy as np


def read_powermetrics(n_samples=50, interval_ms=100):
    """Read CPU temperature via powermetrics (requires sudo)."""
    try:
        result = subprocess.run(
            ['sudo', '-n', 'powermetrics', '--samplers', 'smc',
             f'-i{interval_ms}', f'-n{n_samples}'],
            capture_output=True, text=True, timeout=n_samples * interval_ms / 1000 + 10
        )
        
        temps = []
        for line in result.stdout.split('\n'):
            if 'CPU die temperature' in line:
                try:
                    temp = float(line.split(':')[1].strip().replace(' C', ''))
                    temps.append(temp)
                except (ValueError, IndexError):
                    pass
        return np.array(temps) if temps else None
    except (subprocess.TimeoutExpired, FileNotFoundError, PermissionError):
        return None


def read_osx_cpu_temp(n_samples=50, interval=0.1):
    """Read CPU temperature via osx-cpu-temp utility."""
    try:
        temps = []
        for _ in range(n_samples):
            result = subprocess.run(
                ['osx-cpu-temp'],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                # Parse "CPU: 45.2°C" format
                for line in result.stdout.split('\n'):
                    if 'CPU' in line:
                        try:
                            temp = float(line.split(':')[1].strip().replace('°C', '').strip())
                            temps.append(temp)
                        except (ValueError, IndexError):
                            pass
            time.sleep(interval)
        return np.array(temps) if temps else None
    except FileNotFoundError:
        return None


def read_ioreg_thermal(n_samples=50, interval=0.1):
    """Try reading thermal sensors via ioreg."""
    try:
        temps = []
        for _ in range(n_samples):
            result = subprocess.run(
                ['ioreg', '-rc', 'AppleSmartBattery'],
                capture_output=True, text=True, timeout=5
            )
            # Also try thermal sensors
            result2 = subprocess.run(
                ['ioreg', '-n', 'AppleAPCIThermalDriver'],
                capture_output=True, text=True, timeout=5
            )
            for output in [result.stdout, result2.stdout]:
                for line in output.split('\n'):
                    if 'Temperature' in line and '=' in line:
                        try:
                            val = line.split('=')[1].strip().rstrip('}').strip()
                            temp = float(val) / 100.0  # Often in centidegrees
                            if 10 < temp < 120:  # sanity check
                                temps.append(temp)
                        except (ValueError, IndexError):
                            pass
            time.sleep(interval)
        return np.array(temps) if temps else None
    except Exception:
        return None


def thermal_proxy_timing(n_samples=500):
    """Use computation timing as thermal proxy.
    
    CPU thermal state affects execution speed through:
    - Thermal throttling
    - Voltage/frequency scaling
    - Cache behavior changes
    
    Timing variations correlate with thermal state.
    """
    print("  Using computation timing as thermal proxy...")
    timings = []
    
    for _ in range(n_samples):
        # Do a small, thermally-sensitive computation
        t1 = time.perf_counter_ns()
        
        # Mixed operations that stress different units
        x = 0.0
        for i in range(100):
            x += np.sin(i * 0.01) * np.cos(i * 0.02)
        
        t2 = time.perf_counter_ns()
        timings.append(t2 - t1)
    
    return np.array(timings)


def extract_thermal_entropy(temps, label="thermal"):
    """Extract entropy from temperature or timing series."""
    if len(temps) < 10:
        return None, None
    
    # Method 1: LSB of raw values (for float temps, use fractional part)
    if temps.dtype in [np.float64, np.float32]:
        # Extract fractional part, convert to int representation
        frac = (temps * 1000) % 16  # bottom 4 bits of millidegrees
        lsb = frac.astype(np.int64)
    else:
        # Integer timing data — bottom bits
        lsb = np.bitwise_and(temps.astype(np.int64), 0x0F)
    
    # Method 2: Differences (delta encoding)
    deltas = np.diff(temps)
    
    return lsb, deltas


if __name__ == '__main__':
    print("=== CPU Thermal Jitter Entropy Explorer ===\n")
    
    data = None
    source = None
    
    # Try powermetrics
    print("Trying powermetrics (needs sudo)...")
    data = read_powermetrics(n_samples=50)
    if data is not None and len(data) > 0:
        source = "powermetrics"
        print(f"  Got {len(data)} temperature readings")
    
    # Try osx-cpu-temp
    if data is None:
        print("Trying osx-cpu-temp...")
        data = read_osx_cpu_temp(n_samples=50)
        if data is not None and len(data) > 0:
            source = "osx-cpu-temp"
            print(f"  Got {len(data)} temperature readings")
    
    # Try ioreg
    if data is None:
        print("Trying ioreg thermal sensors...")
        data = read_ioreg_thermal(n_samples=50)
        if data is not None and len(data) > 0:
            source = "ioreg"
            print(f"  Got {len(data)} temperature readings")
    
    # Fallback: timing proxy
    if data is None:
        print("No direct thermal access — using timing proxy")
        data = thermal_proxy_timing(n_samples=500)
        source = "timing_proxy"
    
    print(f"\nSource: {source}")
    print(f"Samples: {len(data)}")
    print(f"Mean: {np.mean(data):.4f}")
    print(f"Std: {np.std(data):.4f}")
    print(f"Range: [{np.min(data)}, {np.max(data)}]")
    
    lsb, deltas = extract_thermal_entropy(data)
    
    if lsb is not None:
        unique, counts = np.unique(lsb, return_counts=True)
        probs = counts / len(lsb)
        ent = -np.sum(probs * np.log2(probs + 1e-15))
        print(f"\nLSB(4bit) Shannon entropy: {ent:.4f} / 4.0 bits")
        print(f"Unique values: {len(unique)}")
        
        if deltas is not None and len(deltas) > 0:
            d_lsb = np.bitwise_and(np.abs(deltas).astype(np.int64), 0x0F)
            d_unique, d_counts = np.unique(d_lsb, return_counts=True)
            d_probs = d_counts / len(d_lsb)
            d_ent = -np.sum(d_probs * np.log2(d_probs + 1e-15))
            print(f"Delta LSB entropy: {d_ent:.4f} / 4.0 bits")
        
        outfile = 'entropy_cpu_thermal.bin'
        lsb.astype(np.uint8).tofile(outfile)
        print(f"\nSaved {len(lsb)} samples to {outfile}")
    
    print(f"\n--- Notes ---")
    print(f"  For real thermal data: brew install osx-cpu-temp")
    print(f"  Or: sudo powermetrics --samplers smc -i100 -n50")
    print(f"  Timing proxy still captures thermal effects indirectly")
