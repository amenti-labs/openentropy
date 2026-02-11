#!/usr/bin/env python3
"""
Harvest entropy from microphone thermal noise.

With no audio input (or muted mic), the ADC still produces
Johnson-Nyquist thermal noise from the input impedance.
This is genuine physical randomness.
"""
import numpy as np
import sounddevice as sd
import sys

SAMPLE_RATE = 44100
DURATION_SEC = 1.0
BITS_PER_SAMPLE = 16

def capture_thermal_noise(duration=DURATION_SEC, sr=SAMPLE_RATE):
    """Capture raw audio samples — thermal noise when input is silent."""
    print(f"Recording {duration}s of thermal noise at {sr}Hz...")
    samples = sd.rec(int(duration * sr), samplerate=sr, channels=1, dtype='int16')
    sd.wait()
    return samples.flatten()

def extract_entropy(samples, method='lsb'):
    """Extract entropy bits from raw samples."""
    if method == 'lsb':
        # Least significant bits are dominated by thermal noise
        bits = np.bitwise_and(samples.astype(np.int16), 0x03)  # bottom 2 bits
        return bits
    elif method == 'von_neumann':
        # Von Neumann debiasing on LSBs
        lsbs = np.bitwise_and(samples.astype(np.int16), 1)
        pairs = lsbs[:len(lsbs)//2*2].reshape(-1, 2)
        mask = pairs[:, 0] != pairs[:, 1]
        return pairs[mask, 0]

def measure_quality(bits):
    """Basic entropy quality stats."""
    unique, counts = np.unique(bits, return_counts=True)
    probs = counts / len(bits)
    entropy = -np.sum(probs * np.log2(probs + 1e-10))
    print(f"  Samples: {len(bits)}")
    print(f"  Unique values: {len(unique)}")
    print(f"  Shannon entropy: {entropy:.4f} bits")
    print(f"  Distribution: {dict(zip(unique, counts))}")
    return entropy

if __name__ == '__main__':
    print("=== Microphone Thermal Noise Entropy ===\n")
    samples = capture_thermal_noise()
    
    print(f"\nRaw sample stats:")
    print(f"  Mean: {np.mean(samples):.2f}")
    print(f"  Std:  {np.std(samples):.2f}")
    print(f"  Min:  {np.min(samples)}, Max: {np.max(samples)}")
    
    print(f"\nLSB extraction (2 bits):")
    lsb_bits = extract_entropy(samples, 'lsb')
    measure_quality(lsb_bits)
    
    print(f"\nVon Neumann debiased:")
    vn_bits = extract_entropy(samples, 'von_neumann')
    measure_quality(vn_bits)
    
    # Save raw entropy
    outfile = 'entropy_mic_thermal.bin'
    vn_bits.astype(np.uint8).tofile(outfile)
    print(f"\nSaved {len(vn_bits)} debiased bits to {outfile}")
