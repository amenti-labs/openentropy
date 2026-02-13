#!/usr/bin/env python3
"""Basic entropy collection with openentropy.

Creates an entropy pool, collects random bytes, and prints health stats.

Usage:
    pip install maturin && maturin develop --release
    python examples/python/basic.py
"""

from openentropy import EntropyPool, detect_available_sources, version

print(f"openentropy v{version()}")

# Discover what's available
sources = detect_available_sources()
print(f"\n{len(sources)} entropy sources detected:")
for s in sources:
    print(f"  - {s['name']}: {s['description']} ({s['category']})")

# Create pool with all available sources
pool = EntropyPool.auto()
print(f"\nPool created with {pool.source_count} sources")

# Collect entropy (parallel for speed)
collected = pool.collect_all(parallel=True, timeout=10.0)
print(f"Collected {collected} raw bytes")

# Get conditioned random bytes
data = pool.get_random_bytes(64)
print(f"\n64 random bytes (hex): {data.hex()}")

# Health report
report = pool.health_report()
print(f"\nHealth: {report['healthy']}/{report['total']} sources healthy")
print(f"Raw bytes in pool: {report['raw_bytes']}")
print(f"Output bytes produced: {report['output_bytes']}")

for s in report["sources"]:
    status = "✓" if s["healthy"] else "✗"
    print(f"  {status} {s['name']}: {s['bytes']} bytes, H={s['entropy']:.3f}, {s['time']:.3f}s")
