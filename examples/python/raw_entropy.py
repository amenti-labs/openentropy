#!/usr/bin/env python3
"""Get raw unconditioned entropy and compare with conditioned output.

Raw mode preserves the actual hardware noise signal — no SHA-256,
no DRBG, no whitening. Useful for researchers studying device entropy.

Usage:
    python examples/python/raw_entropy.py
"""

from openentropy import EntropyPool

pool = EntropyPool.auto()
pool.collect_all(parallel=True, timeout=10.0)

n = 256

# Raw — XOR-combined source bytes, no conditioning
raw = pool.get_bytes(n, conditioning="raw")
print(f"Raw ({len(raw)} bytes):        {raw[:32].hex()}...")

# Von Neumann — debiased but preserves noise structure
vn = pool.get_bytes(n, conditioning="vonneumann")
print(f"VonNeumann ({len(vn)} bytes):  {vn[:32].hex()}...")

# SHA-256 — full cryptographic conditioning (default)
sha = pool.get_bytes(n, conditioning="sha256")
print(f"SHA-256 ({len(sha)} bytes):    {sha[:32].hex()}...")

# Also available as shorthand:
default = pool.get_random_bytes(n)  # SHA-256 conditioned
raw2 = pool.get_raw_bytes(n)        # raw, unconditioned

print(f"\nget_random_bytes: {default[:16].hex()}...")
print(f"get_raw_bytes:    {raw2[:16].hex()}...")

print("\n⚠️  Raw mode bypasses all conditioning. Use for research only.")
