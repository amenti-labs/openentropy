"""Platform detection — discover available entropy sources."""

from __future__ import annotations

import platform as _platform

from esoteric_entropy.sources import ALL_SOURCES
from esoteric_entropy.sources.base import EntropySource


def detect_available_sources() -> list[EntropySource]:
    """Instantiate and return all sources available on this machine."""
    available: list[EntropySource] = []
    for cls in ALL_SOURCES:
        try:
            src = cls()
            if src.is_available():
                available.append(src)
        except Exception:
            continue
    return available


def platform_info() -> dict:
    """Return basic platform metadata."""
    return {
        "system": _platform.system(),
        "machine": _platform.machine(),
        "platform": _platform.platform(),
        "python": _platform.python_version(),
    }
