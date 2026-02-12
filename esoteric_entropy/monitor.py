"""Live entropy monitor — interactive TUI dashboard.

TODO: Rebuild TUI from scratch. The previous implementation had
incompatibilities with modern Textual (7.x) that caused the UI timer
to die after the first update and keyboard navigation to break.

Key requirements for the rebuild:
- Use Vertical/Horizontal containers (not CSS grid)
- Keep pool init and collection in background threads
- Batch DataTable cell updates to avoid flooding the event loop
- Store column keys from add_columns() (Textual 7.x dict-based columns)
- Ensure DataTable gets focus on mount for keyboard navigation
- Guard get_random_bytes() to avoid triggering synchronous collect_all()
"""

from __future__ import annotations


class EntropyMonitor:
    """Wrapper for CLI integration."""

    def __init__(self, refresh_rate: float = 1.0, sources_filter: list[str] | None = None):
        self.refresh_rate = refresh_rate
        self.sources_filter = sources_filter

    def run(self) -> None:
        print("The interactive TUI monitor is currently being rebuilt.")
        print()
        print("In the meantime, use these alternatives:")
        print("  esoteric-entropy scan       # discover sources")
        print("  esoteric-entropy bench      # benchmark all sources")
        print("  esoteric-entropy pool       # pool health metrics")
        print("  esoteric-entropy report     # full NIST test battery")
        print("  esoteric-entropy stream     # continuous entropy to stdout")
