#!/usr/bin/env python3
"""
Entropy conditioning pipeline.
Transforms raw, potentially biased entropy into high-quality random bits.

Methods:
- Von Neumann debiasing
- XOR folding
- SHA-256 conditioning (NIST approved)
- Toeplitz hashing (information-theoretic extraction)
"""
import numpy as np
import hashlib
import struct
import os
import sys


def von_neumann_debias(bits):
    """Von Neumann debiasing: remove bias from a bit stream.
    
    Takes pairs of bits:
      (0,1) → 0
      (1,0) → 1
      (0,0), (1,1) → discard
    
    Output rate: ~25% of input for unbiased, less for biased.
    Guarantees unbiased output regardless of input bias.
    """
    bits = np.asarray(bits).flatten()
    # Ensure binary
    bits = (bits % 2).astype(np.uint8)
    n = len(bits) - (len(bits) % 2)
    pairs = bits[:n].reshape(-1, 2)
    mask = pairs[:, 0] != pairs[:, 1]
    output = pairs[mask, 0]
    return output, {
        'input_bits': len(bits),
        'output_bits': len(output),
        'efficiency': len(output) / len(bits) if len(bits) > 0 else 0,
        'pairs_discarded': int(np.sum(~mask)),
    }


def xor_fold(data, fold_factor=2):
    """XOR folding: combine N values into 1 by XOR.
    
    Increases entropy density at the cost of throughput.
    fold_factor=2: XOR pairs, fold_factor=4: XOR quads, etc.
    """
    data = np.asarray(data).flatten()
    n = len(data) - (len(data) % fold_factor)
    chunks = data[:n].reshape(-1, fold_factor)
    result = chunks[:, 0].copy()
    for i in range(1, fold_factor):
        result = np.bitwise_xor(result.astype(np.int64), chunks[:, i].astype(np.int64))
    return result.astype(np.uint8), {
        'input_samples': len(data),
        'output_samples': len(result),
        'fold_factor': fold_factor,
    }


def sha256_condition(data, output_bytes=32):
    """SHA-256 conditioning (NIST SP 800-90B approved).
    
    Hashes input data to produce cryptographically conditioned output.
    Input should have at least 256 bits of entropy for full-strength output.
    """
    data = np.asarray(data)
    raw = data.astype(np.uint8).tobytes()
    
    # If data is larger than one hash worth, use iterative hashing
    outputs = []
    block_size = 256  # bytes per hash input block
    total_needed = output_bytes
    
    if len(raw) <= block_size:
        h = hashlib.sha256(raw).digest()
        return np.frombuffer(h[:output_bytes], dtype=np.uint8), {
            'input_bytes': len(raw),
            'output_bytes': output_bytes,
            'method': 'single_hash',
        }
    
    # Multi-block: hash with counter for each output block
    result = b''
    counter = 0
    while len(result) < total_needed:
        block = struct.pack('>I', counter) + raw
        result += hashlib.sha256(block).digest()
        counter += 1
    
    return np.frombuffer(result[:output_bytes], dtype=np.uint8), {
        'input_bytes': len(raw),
        'output_bytes': output_bytes,
        'method': 'counter_mode',
        'blocks_hashed': counter,
    }


def toeplitz_hash(data, output_bits=128, seed=None):
    """Toeplitz hashing — information-theoretic randomness extraction.
    
    Uses a Toeplitz matrix (constant along diagonals) to extract
    near-uniform bits from a weakly random source. Requires a
    uniform seed (can come from a trusted source).
    
    This is a strong extractor when seed is independent of data.
    """
    data = np.asarray(data).flatten()
    n = len(data)
    
    # Convert data to bit vector
    bit_data = np.unpackbits(data.astype(np.uint8))
    input_bits = len(bit_data)
    
    if input_bits < output_bits:
        output_bits = input_bits // 2
    
    # Generate Toeplitz matrix from seed
    if seed is None:
        # In practice, seed should be truly random and independent
        rng = np.random.RandomState(42)  # deterministic for reproducibility
        seed_bits = rng.randint(0, 2, size=input_bits + output_bits - 1)
    else:
        seed = np.asarray(seed).flatten()
        seed_bits = np.unpackbits(seed.astype(np.uint8))
        seed_bits = seed_bits[:input_bits + output_bits - 1]
        if len(seed_bits) < input_bits + output_bits - 1:
            # Extend seed if too short
            rng = np.random.RandomState(int.from_bytes(seed.tobytes()[:4], 'big'))
            extra = rng.randint(0, 2, size=input_bits + output_bits - 1 - len(seed_bits))
            seed_bits = np.concatenate([seed_bits, extra])
    
    # Toeplitz matrix-vector multiplication in GF(2)
    output = np.zeros(output_bits, dtype=np.uint8)
    for i in range(output_bits):
        row = seed_bits[i:i + input_bits]
        output[i] = np.sum(row * bit_data[:len(row)]) % 2
    
    # Pack bits back to bytes
    n_bytes = (output_bits + 7) // 8
    padded = np.zeros(n_bytes * 8, dtype=np.uint8)
    padded[:output_bits] = output
    result = np.packbits(padded)
    
    return result, {
        'input_bits': input_bits,
        'output_bits': output_bits,
        'compression_ratio': output_bits / input_bits if input_bits > 0 else 0,
        'seed_source': 'provided' if seed is not None else 'deterministic_default',
    }


def compare_conditioning(data, label="data"):
    """Compare all conditioning methods on the same input data.
    
    Returns a comparison report with entropy stats for each method.
    """
    sys_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if sys_path not in sys.path:
        sys.path.insert(0, sys_path)
    from analysis.entropy_tests import shannon_entropy, min_entropy
    
    data = np.asarray(data)
    results = {'source': label, 'input_samples': len(data.flat)}
    
    # Raw
    raw_sh = shannon_entropy(data)
    raw_me = min_entropy(data)
    results['raw'] = {
        'shannon': raw_sh['shannon_entropy'],
        'min_entropy': raw_me['min_entropy'],
        'samples': len(data.flat),
    }
    
    # Von Neumann
    binary = (data.flatten() % 2).astype(np.uint8)
    vn_out, vn_info = von_neumann_debias(binary)
    if len(vn_out) > 10:
        vn_sh = shannon_entropy(vn_out)
        results['von_neumann'] = {
            'shannon': vn_sh['shannon_entropy'],
            'efficiency': vn_info['efficiency'],
            'output_bits': len(vn_out),
        }
    
    # XOR fold (2x)
    xor_out, xor_info = xor_fold(data, fold_factor=2)
    if len(xor_out) > 10:
        xor_sh = shannon_entropy(xor_out)
        xor_me = min_entropy(xor_out)
        results['xor_fold_2x'] = {
            'shannon': xor_sh['shannon_entropy'],
            'min_entropy': xor_me['min_entropy'],
            'output_samples': len(xor_out),
        }
    
    # SHA-256
    sha_out, sha_info = sha256_condition(data, output_bytes=min(256, len(data.flat)))
    sha_sh = shannon_entropy(sha_out)
    sha_me = min_entropy(sha_out)
    results['sha256'] = {
        'shannon': sha_sh['shannon_entropy'],
        'min_entropy': sha_me['min_entropy'],
        'output_bytes': len(sha_out),
    }
    
    # Toeplitz
    small = data.flatten()[:256].astype(np.uint8)
    if len(small) >= 16:
        toe_out, toe_info = toeplitz_hash(small, output_bits=64)
        toe_sh = shannon_entropy(toe_out)
        results['toeplitz'] = {
            'shannon': toe_sh['shannon_entropy'],
            'output_bits': toe_info['output_bits'],
            'compression': toe_info['compression_ratio'],
        }
    
    return results


if __name__ == '__main__':
    print("=== Conditioning Pipeline Demo ===\n")
    
    # Test with biased data
    print("Input: heavily biased data (70% zeros)")
    biased = np.random.choice([0, 1], size=10000, p=[0.7, 0.3]).astype(np.uint8)
    
    print(f"\n--- Von Neumann Debiasing ---")
    vn_out, vn_info = von_neumann_debias(biased)
    print(f"  Input: {vn_info['input_bits']} bits, Output: {vn_info['output_bits']} bits")
    print(f"  Efficiency: {vn_info['efficiency']*100:.1f}%")
    if len(vn_out) > 0:
        print(f"  Output bias: {np.mean(vn_out):.4f} (ideal: 0.5)")
    
    print(f"\n--- XOR Folding (2x) ---")
    xor_out, xor_info = xor_fold(biased, fold_factor=2)
    print(f"  Input: {xor_info['input_samples']}, Output: {xor_info['output_samples']}")
    print(f"  Output bias: {np.mean(xor_out):.4f}")
    
    print(f"\n--- XOR Folding (4x) ---")
    xor4_out, xor4_info = xor_fold(biased, fold_factor=4)
    print(f"  Input: {xor4_info['input_samples']}, Output: {xor4_info['output_samples']}")
    print(f"  Output bias: {np.mean(xor4_out):.4f}")
    
    print(f"\n--- SHA-256 Conditioning ---")
    sha_out, sha_info = sha256_condition(biased, output_bytes=32)
    print(f"  Input: {sha_info['input_bytes']} bytes → {sha_info['output_bytes']} bytes")
    print(f"  Output mean: {np.mean(sha_out):.1f} (ideal: 127.5)")
    
    print(f"\n--- Toeplitz Hashing ---")
    toe_out, toe_info = toeplitz_hash(biased[:256], output_bits=64)
    print(f"  Compression: {toe_info['compression_ratio']:.4f}")
    
    print(f"\n--- Full Comparison ---")
    import json
    comparison = compare_conditioning(biased, "biased_70_30")
    print(json.dumps(comparison, indent=2, default=str))
