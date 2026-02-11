"""Entropy conditioning / whitening algorithms.

Transforms raw, potentially biased entropy into high-quality random bits.
"""

from __future__ import annotations

import hashlib
import struct

import numpy as np


def von_neumann_debias(bits: np.ndarray) -> tuple[np.ndarray, dict]:
    """Von Neumann debiasing — remove bias from a bit stream.

    Pairs (0,1)→0, (1,0)→1; equal pairs discarded.
    Output rate ~25 % for unbiased input, guarantees unbiased output.
    """
    bits = np.asarray(bits).flatten() % 2
    n = len(bits) - (len(bits) % 2)
    pairs = bits[:n].reshape(-1, 2)
    mask = pairs[:, 0] != pairs[:, 1]
    output = pairs[mask, 0].astype(np.uint8)
    return output, {
        "input_bits": len(bits),
        "output_bits": len(output),
        "efficiency": len(output) / max(len(bits), 1),
    }


def xor_fold(data: np.ndarray, fold_factor: int = 2) -> np.ndarray:
    """XOR-fold *data* by *fold_factor* to increase entropy density."""
    data = np.asarray(data).flatten()
    n = len(data) - (len(data) % fold_factor)
    chunks = data[:n].reshape(-1, fold_factor).astype(np.int64)
    result = chunks[:, 0]
    for i in range(1, fold_factor):
        result = np.bitwise_xor(result, chunks[:, i])
    return result.astype(np.uint8)


def sha256_condition(data: np.ndarray | bytes, output_bytes: int = 32) -> bytes:
    """SHA-256 conditioning (NIST SP 800-90B approved).

    Hash input to produce cryptographically conditioned output.
    """
    raw = bytes(np.asarray(data).astype(np.uint8).tobytes()) if not isinstance(data, bytes) else data
    result = b""
    counter = 0
    while len(result) < output_bytes:
        block = struct.pack(">I", counter) + raw
        result += hashlib.sha256(block).digest()
        counter += 1
    return result[:output_bytes]
