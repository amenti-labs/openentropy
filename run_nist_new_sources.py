#!/usr/bin/env python3
"""Run NIST test battery on all new entropy sources."""

import sys
import time
import numpy as np

sys.path.insert(0, '.')

from esoteric_entropy.test_suite import run_all_tests, calculate_quality_score
from esoteric_entropy.sources.silicon import (
    DRAMRowBufferSource, CacheContentionSource,
    PageFaultTimingSource, SpeculativeExecutionSource,
)
from esoteric_entropy.sources.ioregistry import IORegistryEntropySource
from esoteric_entropy.sources.cross_domain import (
    CPUIOBeatSource, CPUMemoryBeatSource, MultiDomainBeatSource,
)
from esoteric_entropy.sources.compression import CompressionTimingSource, HashTimingSource
from esoteric_entropy.sources.novel import (
    DispatchQueueSource, DyldTimingSource, VMPageTimingSource, SpotlightTimingSource,
)

NEW_SOURCES = [
    DRAMRowBufferSource,
    CacheContentionSource,
    PageFaultTimingSource,
    SpeculativeExecutionSource,
    IORegistryEntropySource,
    CPUIOBeatSource,
    CPUMemoryBeatSource,
    MultiDomainBeatSource,
    CompressionTimingSource,
    HashTimingSource,
    DispatchQueueSource,
    DyldTimingSource,
    VMPageTimingSource,
    SpotlightTimingSource,
]

results_summary = []

for SourceClass in NEW_SOURCES:
    src = SourceClass()
    print(f"\n{'='*60}")
    print(f"SOURCE: {src.name}")
    print(f"{'='*60}")

    if not src.is_available():
        print("  NOT AVAILABLE — skipping")
        results_summary.append((src.name, "N/A", 0, 0, 0))
        continue

    t0 = time.time()
    try:
        data = src.collect(5000)
    except Exception as e:
        print(f"  COLLECT ERROR: {e}")
        results_summary.append((src.name, "ERROR", 0, 0, 0))
        continue
    collect_time = time.time() - t0

    if len(data) < 100:
        print(f"  Too few samples: {len(data)}")
        results_summary.append((src.name, "INSUFFICIENT", len(data), 0, 0))
        continue

    print(f"  Collected {len(data)} samples in {collect_time:.1f}s")

    # Quick quality check
    quality = src.entropy_quality()
    print(f"  Quick quality: {quality.get('grade', '?')} (shannon={quality.get('shannon_entropy', 0):.3f}, "
          f"compress={quality.get('compression_ratio', 0):.3f})")

    # Full NIST battery
    test_results = run_all_tests(data)
    score = calculate_quality_score(test_results)
    passed = sum(1 for r in test_results if r.passed)
    total = len(test_results)

    print(f"  NIST: {passed}/{total} passed, score={score:.1f}/100")

    # Show failures
    for r in test_results:
        if not r.passed:
            print(f"    FAIL: {r.name} (grade={r.grade}, {r.details[:60]})")

    results_summary.append((src.name, quality.get('grade', '?'), len(data), score, passed))

print(f"\n\n{'='*70}")
print("FINAL SUMMARY — All New Sources")
print(f"{'='*70}")
print(f"{'Source':<25} {'Grade':>5} {'Samples':>8} {'NIST Score':>10} {'Passed':>7}")
print("-" * 70)
for name, grade, samples, score, passed in results_summary:
    print(f"  {name:<23} {grade:>5} {samples:>8} {score:>9.1f} {passed:>6}/31")
