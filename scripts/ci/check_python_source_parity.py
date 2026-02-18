#!/usr/bin/env python3
"""CI check: Python bindings expose the full Rust source set."""

from __future__ import annotations

import openentropy


def _name_set(rows: list[dict]) -> set[str]:
    return {row["name"] for row in rows}


def main() -> None:
    detected = openentropy.detect_available_sources()
    pool = openentropy.EntropyPool.auto()
    infos = pool.sources()
    names = set(pool.source_names())

    detected_names = _name_set(detected)
    info_names = _name_set(infos)

    assert detected_names, "No entropy sources detected"
    assert pool.source_count == len(names), (
        f"source_count mismatch: source_count={pool.source_count}, source_names={len(names)}"
    )
    assert detected_names == info_names, (
        "detect_available_sources names differ from EntropyPool.sources names:\n"
        f"detect_only={sorted(detected_names - info_names)}\n"
        f"pool_only={sorted(info_names - detected_names)}"
    )
    assert detected_names == names, (
        "detect_available_sources names differ from EntropyPool.source_names:\n"
        f"detect_only={sorted(detected_names - names)}\n"
        f"pool_only={sorted(names - detected_names)}"
    )

    required_health_keys = {"name", "healthy", "bytes", "entropy", "min_entropy", "time", "failures"}
    pool.collect_all(parallel=False, timeout=5.0)
    health = pool.health_report()
    if health["sources"]:
        missing = required_health_keys - set(health["sources"][0].keys())
        assert not missing, f"health_report source entries missing keys: {sorted(missing)}"

    print(f"Python source parity OK across {len(detected_names)} sources")


if __name__ == "__main__":
    main()
