#!/usr/bin/env python3
"""
EMI Audio Coupling — harvest electromagnetic interference through the audio ADC.

With no microphone or with the built-in mic in a silent room, the audio ADC
picks up radiated EMI from CPU/GPU switching. This is computation-correlated
noise — different from thermal noise.
"""
import numpy as np
import hashlib
import struct
import time
import subprocess
import os

try:
    import sounddevice as sd
except ImportError:
    sd = None

try:
    from scipy import signal as scipy_signal
    from scipy.fft import fft, fftfreq
except ImportError:
    scipy_signal = None

def record_audio(duration_s=2.0, sample_rate=48000, channels=1):
    """Record audio from default input."""
    if sd is None:
        print("[EMI] sounddevice not available")
        return None
    print(f"[EMI] Recording {duration_s}s at {sample_rate}Hz...")
    try:
        data = sd.rec(int(duration_s * sample_rate), samplerate=sample_rate,
                      channels=channels, dtype='float32', blocking=True)
        return data.flatten()
    except Exception as e:
        print(f"[EMI] Recording error: {e}")
        return None

def analyze_spectrum(data, sample_rate=48000):
    """FFT analysis to find EMI spectral peaks."""
    N = len(data)
    yf = np.abs(fft(data))[:N//2]
    xf = fftfreq(N, 1/sample_rate)[:N//2]
    
    # Find peaks
    noise_floor = np.median(yf)
    peak_threshold = noise_floor * 5
    peaks = []
    for i in range(1, len(yf)-1):
        if yf[i] > peak_threshold and yf[i] > yf[i-1] and yf[i] > yf[i+1]:
            peaks.append((xf[i], yf[i], yf[i]/noise_floor))
    
    peaks.sort(key=lambda x: -x[2])
    return xf, yf, peaks[:20], noise_floor

def extract_entropy_between_peaks(data, sample_rate, peaks):
    """Extract entropy from noise BETWEEN characteristic EMI peaks."""
    if scipy_signal is None:
        # Simple: just use LSBs
        return extract_lsb_entropy(data)
    
    # Notch filter out the known EMI peaks
    filtered = data.copy()
    for freq, _, snr in peaks[:10]:
        if freq > 10 and freq < sample_rate/2 - 10:
            b, a = scipy_signal.iirnotch(freq, Q=30, fs=sample_rate)
            filtered = scipy_signal.filtfilt(b, a, filtered)
    
    return extract_lsb_entropy(filtered)

def extract_lsb_entropy(data, bits=8):
    """Extract LSBs from audio samples as entropy."""
    # Normalize to full range
    if np.max(np.abs(data)) == 0:
        return np.array([], dtype=np.uint8)
    data_norm = data / (np.max(np.abs(data)) + 1e-30)
    # Quantize to 16-bit equivalent and take lower bits
    quantized = ((data_norm + 1) * 32767).astype(np.int32)
    lsbs = (quantized & 0xFF).astype(np.uint8)
    return lsbs

def cpu_load_burst(duration_s=0.5):
    """Create a short CPU load burst to change EMI signature."""
    end = time.time() + duration_s
    x = 0
    while time.time() < end:
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF

def run(output_file='explore/entropy_emi_audio.bin'):
    """Main exploration routine."""
    print("=" * 60)
    print("EMI AUDIO COUPLING — Electromagnetic Interference Harvester")
    print("=" * 60)
    
    if sd is None:
        print("[FAIL] sounddevice not installed")
        return None
    
    sample_rate = 48000
    duration = 3.0
    
    # Record during idle
    print("\n[Phase 1] Recording during CPU idle...")
    idle_data = record_audio(duration, sample_rate)
    if idle_data is None or len(idle_data) == 0:
        print("[FAIL] Could not record audio")
        return None
    
    idle_rms = np.sqrt(np.mean(idle_data**2))
    print(f"  Idle RMS: {idle_rms:.8f}")
    print(f"  Idle peak: {np.max(np.abs(idle_data)):.8f}")
    
    # Record during CPU load
    print("\n[Phase 2] Recording during CPU load burst...")
    # Start load in background
    import threading
    load_thread = threading.Thread(target=cpu_load_burst, args=(duration + 0.5,))
    load_thread.start()
    time.sleep(0.2)
    load_data = record_audio(duration, sample_rate)
    load_thread.join()
    
    if load_data is not None:
        load_rms = np.sqrt(np.mean(load_data**2))
        print(f"  Load RMS: {load_rms:.8f}")
        print(f"  Load peak: {np.max(np.abs(load_data)):.8f}")
        print(f"  RMS difference: {abs(load_rms - idle_rms):.8f} ({abs(load_rms-idle_rms)/max(idle_rms,1e-30)*100:.1f}%)")
    
    # Spectral analysis
    if scipy_signal is not None:
        print("\n[Phase 3] Spectral analysis...")
        xf, yf_idle, peaks_idle, floor_idle = analyze_spectrum(idle_data, sample_rate)
        print(f"  Noise floor: {floor_idle:.8f}")
        if peaks_idle:
            print(f"  Top EMI peaks (idle):")
            for freq, mag, snr in peaks_idle[:10]:
                print(f"    {freq:8.1f} Hz  SNR: {snr:6.1f}x")
        
        if load_data is not None:
            _, yf_load, peaks_load, floor_load = analyze_spectrum(load_data, sample_rate)
            print(f"\n  Top EMI peaks (CPU load):")
            for freq, mag, snr in peaks_load[:10]:
                print(f"    {freq:8.1f} Hz  SNR: {snr:6.1f}x")
            
            # Compare spectra
            spectral_diff = np.mean(np.abs(yf_load - yf_idle))
            print(f"\n  Spectral difference (idle vs load): {spectral_diff:.8f}")
    else:
        peaks_idle = []
    
    # Extract entropy
    print("\n[Phase 4] Extracting entropy...")
    all_entropy = bytearray()
    
    # From idle recording
    ent_idle = extract_entropy_between_peaks(idle_data, sample_rate, peaks_idle) if peaks_idle else extract_lsb_entropy(idle_data)
    all_entropy.extend(ent_idle.tobytes())
    
    # From load recording
    if load_data is not None:
        ent_load = extract_entropy_between_peaks(load_data, sample_rate, peaks_idle) if peaks_idle else extract_lsb_entropy(load_data)
        all_entropy.extend(ent_load.tobytes())
    
    # XOR idle and load for additional decorrelation
    if load_data is not None:
        min_len = min(len(ent_idle), len(ent_load))
        xored = np.bitwise_xor(ent_idle[:min_len], ent_load[:min_len])
        all_entropy.extend(xored.tobytes())
    
    # Save
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes")
    print(f"  SHA256: {sha[:32]}...")
    print(f"  Saved to: {output_file}")
    
    # Compression test
    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")
    
    # Byte distribution
    hist = np.histogram(np.frombuffer(bytes(all_entropy), dtype=np.uint8), bins=256, range=(0,256))[0]
    chi2 = np.sum((hist - len(all_entropy)/256)**2 / (len(all_entropy)/256 + 1e-30))
    print(f"  Chi-squared (uniform): {chi2:.1f} (ideal ~255)")
    
    return {
        'total_bytes': len(all_entropy),
        'idle_rms': float(idle_rms),
        'load_rms': float(load_rms) if load_data is not None else None,
        'peaks': len(peaks_idle),
        'sha256': sha,
    }

if __name__ == '__main__':
    run()
