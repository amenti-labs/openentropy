"""Live entropy monitor — interactive TUI dashboard.

Shows real-time entropy collection from all sources with:
- Source health table with live entropy rates
- Pool throughput meter
- Entropy bit visualization (sparkline)
- Rolling quality scores
"""

from __future__ import annotations

import signal
import sys
import threading
import time
from collections import deque

import numpy as np

from rich.console import Console
from rich.layout import Layout
from rich.live import Live
from rich.panel import Panel
from rich.progress import BarColumn, Progress, SpinnerColumn, TextColumn
from rich.table import Table
from rich.text import Text


# ── Sparkline characters ──
SPARK = "▁▂▃▄▅▆▇█"


def _sparkline(values: list[float], width: int = 30) -> str:
    """Render a sparkline string from values."""
    if not values:
        return ""
    recent = values[-width:]
    mn, mx = min(recent), max(recent)
    rng = mx - mn if mx > mn else 1.0
    return "".join(SPARK[min(int((v - mn) / rng * 7), 7)] for v in recent)


def _entropy_bar(value: float, max_val: float = 8.0, width: int = 16) -> Text:
    """Colored bar for entropy value."""
    ratio = min(value / max_val, 1.0)
    filled = int(ratio * width)
    if ratio > 0.8:
        color = "green"
    elif ratio > 0.5:
        color = "yellow"
    elif ratio > 0.2:
        color = "red"
    else:
        color = "bright_black"
    bar = "█" * filled + "░" * (width - filled)
    return Text(bar, style=color)


def _hex_dump(data: bytes, width: int = 32) -> Text:
    """Colorized hex dump of entropy bytes."""
    text = Text()
    for i, b in enumerate(data[:width * 4]):
        # Color by value range for visual density
        if b < 32:
            style = "bright_black"
        elif b < 96:
            style = "blue"
        elif b < 160:
            style = "green"
        elif b < 224:
            style = "yellow"
        else:
            style = "red"
        text.append(f"{b:02x}", style=style)
        if (i + 1) % 2 == 0:
            text.append(" ")
        if (i + 1) % width == 0 and i < width * 4 - 1:
            text.append("\n")
    return text


class EntropyMonitor:
    """Live TUI entropy monitor."""

    def __init__(self, refresh_rate: float = 1.0, sources_filter: list[str] | None = None):
        self.refresh_rate = refresh_rate
        self.sources_filter = sources_filter
        self.console = Console()
        self._stop = threading.Event()

        # State
        self._source_history: dict[str, deque] = {}  # name -> deque of shannon values
        self._pool_throughput: deque = deque(maxlen=60)  # bytes/sec history
        self._total_bytes = 0
        self._total_collections = 0
        self._start_time = 0.0
        self._last_pool_bytes: bytes = b""
        self._last_quality: dict = {}
        self._source_states: list[dict] = []

    def _build_source_table(self) -> Table:
        """Build the source health table."""
        table = Table(
            title="⚡ Entropy Sources",
            show_header=True,
            header_style="bold cyan",
            border_style="bright_black",
            expand=True,
            padding=(0, 1),
        )
        table.add_column("Source", style="bold", ratio=3, no_wrap=True)
        table.add_column("Shannon", justify="right", ratio=1)
        table.add_column("Entropy", ratio=2)
        table.add_column("Bytes", justify="right", ratio=1)
        table.add_column("Time", justify="right", ratio=1)
        table.add_column("Sparkline", ratio=3)

        for ss in self._source_states:
            name = ss["name"]
            shannon = ss.get("entropy", 0.0)
            nbytes = ss.get("bytes", 0)
            collect_time = ss.get("time", 0.0)
            healthy = ss.get("healthy", False)

            # Sparkline from history
            hist = self._source_history.get(name, deque(maxlen=30))
            spark = _sparkline(list(hist))

            # Status icon
            icon = "✅" if healthy else "❌"
            name_text = f"{icon} {name}"

            # Entropy bar
            bar = _entropy_bar(shannon)

            # Time formatting
            if collect_time < 0.01:
                time_str = "<10ms"
            elif collect_time < 1.0:
                time_str = f"{collect_time*1000:.0f}ms"
            else:
                time_str = f"{collect_time:.1f}s"

            table.add_row(
                name_text,
                f"[{'green' if shannon > 5 else 'yellow' if shannon > 3 else 'red'}]{shannon:.2f}[/]",
                bar,
                f"{nbytes:,}",
                time_str,
                f"[bright_black]{spark}[/]",
            )

        return table

    def _build_pool_panel(self) -> Panel:
        """Build the pool status panel."""
        elapsed = time.monotonic() - self._start_time if self._start_time else 0
        rate = self._total_bytes / elapsed if elapsed > 0 else 0

        # Quality metrics
        q = self._last_quality
        shannon = q.get("shannon_entropy", 0.0)
        grade = q.get("grade", "?")
        score = q.get("quality_score", 0.0)

        grade_color = {
            "A": "green", "B": "blue", "C": "yellow", "D": "red", "F": "bright_red"
        }.get(grade, "white")

        # Throughput sparkline
        tp_spark = _sparkline(list(self._pool_throughput), width=40)

        text = Text()
        text.append("  Grade: ", style="bold")
        text.append(f"{grade}", style=f"bold {grade_color}")
        text.append(f"  Score: {score:.0f}/100", style="dim")
        text.append(f"  Shannon: {shannon:.2f}/8.0\n")
        text.append(f"  Total: {self._total_bytes:,} bytes")
        text.append(f"  Rate: {rate:,.0f} B/s")
        text.append(f"  Collections: {self._total_collections}\n")
        text.append(f"  Throughput: ", style="dim")
        text.append(f"{tp_spark}", style="cyan")

        return Panel(text, title="🏊 Conditioned Pool", border_style="green")

    def _build_entropy_viz(self) -> Panel:
        """Build the live entropy byte visualization."""
        if self._last_pool_bytes:
            hex_text = _hex_dump(self._last_pool_bytes, width=32)
        else:
            hex_text = Text("[waiting for first collection...]", style="dim")

        return Panel(hex_text, title="🔬 Live Entropy Bytes", border_style="magenta")

    def _build_layout(self) -> Layout:
        """Build the full dashboard layout."""
        layout = Layout()
        layout.split_column(
            Layout(name="header", size=3),
            Layout(name="body"),
            Layout(name="footer", size=7),
        )

        # Header
        elapsed = time.monotonic() - self._start_time if self._start_time else 0
        healthy = sum(1 for s in self._source_states if s.get("healthy"))
        total = len(self._source_states)
        header = Text()
        header.append("  🔬 ESOTERIC ENTROPY MONITOR", style="bold magenta")
        header.append(f"  │  {healthy}/{total} sources healthy", style="cyan")
        header.append(f"  │  uptime {int(elapsed)}s", style="dim")
        header.append(f"  │  [q] quit", style="bright_black")
        layout["header"].update(Panel(header, border_style="bright_black"))

        # Body: sources table
        layout["body"].update(self._build_source_table())

        # Footer: pool + viz side by side
        layout["footer"].split_row(
            Layout(self._build_pool_panel(), name="pool", ratio=2),
            Layout(self._build_entropy_viz(), name="viz", ratio=1),
        )

        return layout

    def _collect_cycle(self, pool) -> None:
        """Run one collection cycle."""
        from esoteric_entropy.sources.base import EntropySource

        pool.collect_all(parallel=True, timeout=8.0)
        self._total_collections += 1

        # Update source states
        self._source_states = []
        for ss in pool.sources:
            name = ss.source.name
            self._source_states.append({
                "name": name,
                "healthy": ss.healthy,
                "entropy": ss.last_entropy,
                "bytes": ss.total_bytes,
                "time": ss.last_collect_time,
                "failures": ss.failures,
            })

            if name not in self._source_history:
                self._source_history[name] = deque(maxlen=60)
            if ss.last_entropy > 0:
                self._source_history[name].append(ss.last_entropy)

        # Get conditioned output
        out = pool.get_random_bytes(256)
        self._last_pool_bytes = out
        self._total_bytes += len(out)

        # Quality check
        arr = np.frombuffer(out, dtype=np.uint8)
        self._last_quality = EntropySource._quick_quality(arr, "pool")

        # Throughput tracking
        self._pool_throughput.append(sum(s.total_bytes for s in pool.sources))

    def run(self) -> None:
        """Run the live monitor."""
        from esoteric_entropy.pool import EntropyPool

        self.console.clear()
        self._start_time = time.monotonic()

        # Build pool, optionally filtering sources
        pool = EntropyPool.auto()
        if self.sources_filter:
            filt = set(self.sources_filter)
            pool._sources = [s for s in pool._sources if s.source.name in filt]

        if not pool._sources:
            self.console.print("[red]No sources available![/]")
            return

        # Initial collection
        self._collect_cycle(pool)

        # Background collector
        def _collector():
            while not self._stop.is_set():
                try:
                    self._collect_cycle(pool)
                except Exception:
                    pass
                self._stop.wait(self.refresh_rate)

        collector = threading.Thread(target=_collector, daemon=True)
        collector.start()

        # Handle Ctrl+C
        def _sigint(sig, frame):
            self._stop.set()

        old_handler = signal.signal(signal.SIGINT, _sigint)

        try:
            with Live(
                self._build_layout(),
                console=self.console,
                refresh_per_second=2,
                screen=True,
            ) as live:
                while not self._stop.is_set():
                    live.update(self._build_layout())
                    self._stop.wait(0.5)
        except KeyboardInterrupt:
            pass
        finally:
            self._stop.set()
            signal.signal(signal.SIGINT, old_handler)
            self.console.clear()
            self.console.print("[green]Monitor stopped.[/]")
            # Print final stats
            elapsed = time.monotonic() - self._start_time
            self.console.print(f"  Total bytes: {self._total_bytes:,}")
            self.console.print(f"  Collections: {self._total_collections}")
            self.console.print(f"  Uptime: {elapsed:.0f}s")
            healthy = sum(1 for s in self._source_states if s.get("healthy"))
            self.console.print(f"  Sources: {healthy}/{len(self._source_states)} healthy")
