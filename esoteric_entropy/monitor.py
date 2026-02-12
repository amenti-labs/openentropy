"""Live entropy monitor — interactive TUI dashboard.

Features:
- Toggle sources on/off with keyboard
- Live line chart of entropy values (0-1) per source
- RNG number display
- Sparklines, health table, pool stats
- Configurable refresh rate
"""

from __future__ import annotations

import hashlib
import struct
import threading
import time
from collections import deque

import numpy as np

from rich.text import Text

# ── Sparkline / viz helpers ──

SPARK = "▁▂▃▄▅▆▇█"
COLORS = [
    "red", "green", "blue", "yellow", "magenta", "cyan",
    "bright_red", "bright_green", "bright_blue", "bright_yellow",
    "bright_magenta", "bright_cyan", "orange1", "deep_pink1",
    "spring_green1", "dodger_blue1", "gold1", "medium_purple1",
    "dark_orange", "chartreuse1", "steel_blue1", "hot_pink",
    "turquoise2", "salmon1", "orchid1", "khaki1",
    "slate_blue1", "pale_green1",
]


def _sparkline(values: list[float], width: int = 24) -> str:
    if not values:
        return ""
    recent = values[-width:]
    mn, mx = min(recent), max(recent)
    rng = mx - mn if mx > mn else 1.0
    return "".join(SPARK[min(int((v - mn) / rng * 7), 7)] for v in recent)


def _bytes_to_01(data: bytes) -> float:
    """Hash bytes to a float in [0, 1)."""
    h = hashlib.sha256(data).digest()[:8]
    return struct.unpack("<Q", h)[0] / (2**64)


def _entropy_bar_text(value: float, max_val: float = 8.0, width: int = 12) -> str:
    ratio = min(value / max_val, 1.0)
    filled = int(ratio * width)
    return "█" * filled + "░" * (width - filled)


# ── Terminal chart using plotext ──

def _render_chart(
    history: dict[str, deque],
    enabled: set[str],
    width: int = 80,
    height: int = 15,
) -> str:
    """Render a terminal line chart of source entropy (0-1) over time."""
    import plotext as plt

    plt.clear_figure()
    plt.plotsize(width, height)
    plt.theme("dark")
    plt.title("Entropy History (hashed → 0-1)")
    plt.xlabel("Sample")
    plt.ylabel("Value")
    plt.ylim(0, 1)

    color_list = [
        "red", "green", "blue", "yellow", "magenta", "cyan",
        "orange", "red+", "green+", "blue+", "yellow+", "magenta+",
    ]

    plotted = 0
    for i, (name, values) in enumerate(sorted(history.items())):
        if name not in enabled or len(values) < 2:
            continue
        y = list(values)
        x = list(range(len(y)))
        color = color_list[plotted % len(color_list)]
        plt.plot(x, y, label=name[:16], color=color)
        plotted += 1

    if plotted == 0:
        plt.plot([0], [0.5], label="(no data)")

    return plt.build()


# ── Textual TUI App ──

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.reactive import reactive
from textual.widgets import DataTable, Footer, Header, Static


class SourceToggle(Static):
    """Clickable source toggle widget."""
    pass


class ChartWidget(Static):
    """Live plotext chart rendered as rich text."""
    pass


class RNGDisplay(Static):
    """Shows current RNG values."""
    pass


class PoolStatus(Static):
    """Pool health status bar."""
    pass


class InfoPanel(Static):
    """Source info panel — shows physics explanation."""
    pass


class EntropyMonitorApp(App):
    """Interactive entropy monitor TUI."""

    TITLE = "🔬 Esoteric Entropy Monitor"
    CSS = """
    Screen {
        layout: grid;
        grid-size: 2 4;
        grid-columns: 1fr 1fr;
        grid-rows: auto 1fr auto auto;
    }
    #source-table {
        column-span: 1;
        height: 100%;
        border: solid $primary-background;
    }
    #chart {
        column-span: 1;
        height: 100%;
        border: solid $secondary-background;
    }
    #rng-display {
        column-span: 1;
        height: auto;
        min-height: 5;
        border: solid $accent;
    }
    #pool-status {
        column-span: 1;
        height: auto;
        min-height: 5;
        border: solid $success;
    }
    #info-panel {
        column-span: 2;
        height: auto;
        min-height: 3;
        max-height: 8;
        border: solid $warning;
        display: none;
    }
    #info-panel.visible {
        display: block;
    }
    DataTable {
        height: 100%;
    }
    """

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("space", "toggle_source", "Toggle"),
        Binding("i", "show_info", "Info"),
        Binding("a", "enable_all", "All On"),
        Binding("n", "disable_all", "All Off"),
        Binding("f", "cycle_speed", "Speed"),
        Binding("r", "force_refresh", "Refresh"),
    ]

    refresh_rate = reactive(1.0)

    def __init__(
        self,
        initial_rate: float = 1.0,
        sources_filter: list[str] | None = None,
    ):
        super().__init__()
        self._initial_rate = initial_rate
        self._sources_filter = sources_filter
        self._pool = None
        self._enabled: set[str] = set()
        self._source_names: list[str] = []
        self._source_objects: dict[str, object] = {}  # name -> EntropySource instance
        self._source_history: dict[str, deque] = {}  # name -> deque of 0-1 values
        self._source_shannon: dict[str, deque] = {}  # name -> deque of shannon values
        self._source_states: dict[str, dict] = {}
        self._total_bytes = 0
        self._total_collections = 0
        self._last_rng_int = 0
        self._last_rng_float = 0.0
        self._last_rng_hex = ""
        self._last_quality: dict = {}
        self._stop = threading.Event()
        self._collector_thread = None
        self._start_time = 0.0
        self._speeds = [2.0, 1.0, 0.5, 0.25]
        self._speed_idx = 1  # start at 1.0s

    def compose(self) -> ComposeResult:
        yield Header()
        yield DataTable(id="source-table")
        yield ChartWidget(id="chart")
        yield RNGDisplay(id="rng-display")
        yield PoolStatus(id="pool-status")
        yield InfoPanel(id="info-panel")
        yield Footer()

    def on_mount(self) -> None:
        from esoteric_entropy.pool import EntropyPool

        self._pool = EntropyPool.auto()
        if self._sources_filter:
            filt = set(self._sources_filter)
            self._pool._sources = [
                s for s in self._pool._sources
                if any(f in s.source.name for f in filt)
            ]

        self._source_names = [s.source.name for s in self._pool.sources]
        self._source_objects = {s.source.name: s.source for s in self._pool.sources}
        self._enabled = set(self._source_names)

        for name in self._source_names:
            self._source_history[name] = deque(maxlen=120)
            self._source_shannon[name] = deque(maxlen=60)

        # Setup table
        table = self.query_one("#source-table", DataTable)
        table.cursor_type = "row"
        table.add_columns("", "Source", "Shannon", "Bar", "Bytes", "Time", "Spark", "0→1")

        for name in self._source_names:
            table.add_row(
                "✅", name, "-.--", "░" * 12, "0", "-", "", "-.---",
                key=name,
            )

        self._start_time = time.monotonic()
        self.refresh_rate = self._initial_rate

        # Start collector
        self._collector_thread = threading.Thread(target=self._collector_loop, daemon=True)
        self._collector_thread.start()

        # Start UI refresh timer
        self.set_interval(0.5, self._update_ui)

    def _collector_loop(self) -> None:
        """Background collection loop."""
        while not self._stop.is_set():
            self._collect_cycle()
            self._stop.wait(self.refresh_rate)

    def _collect_cycle(self) -> None:
        """Run one collection cycle."""
        if not self._pool:
            return

        # Only collect from enabled sources
        from esoteric_entropy.sources.base import EntropySource

        results_lock = threading.Lock()
        raw_chunks: list[bytes] = []

        def _collect_source(ss):
            if ss.source.name not in self._enabled:
                return
            try:
                t0 = time.monotonic()
                data = ss.source.collect(n_samples=200)
                elapsed = time.monotonic() - t0
                if len(data) > 0:
                    ss.total_bytes += len(data)
                    ss.last_collect_time = elapsed
                    ss.last_entropy = EntropySource._quick_shannon(data)
                    ss.healthy = ss.last_entropy > 1.0

                    # Hash to 0-1 for chart
                    val01 = _bytes_to_01(data.tobytes()[:64])
                    self._source_history[ss.source.name].append(val01)
                    self._source_shannon[ss.source.name].append(ss.last_entropy)

                    with results_lock:
                        raw_chunks.append(data.tobytes())
                else:
                    ss.failures += 1
                    ss.healthy = False
            except Exception:
                ss.failures += 1
                ss.healthy = False

        # Parallel collection
        threads = []
        for ss in self._pool.sources:
            t = threading.Thread(target=_collect_source, args=(ss,), daemon=True)
            t.start()
            threads.append(t)

        deadline = time.monotonic() + 8.0
        for t in threads:
            remaining = max(0.1, deadline - time.monotonic())
            t.join(timeout=remaining)

        # Feed pool buffer
        raw = bytearray()
        for chunk in raw_chunks:
            raw.extend(chunk)
        with self._pool._lock:
            self._pool._buffer.extend(raw)

        self._total_collections += 1

        # Generate conditioned RNG output
        out = self._pool.get_random_bytes(32)
        self._total_bytes += 32
        self._last_rng_hex = out.hex()
        self._last_rng_int = int.from_bytes(out[:8], "little")
        self._last_rng_float = _bytes_to_01(out)

        # Quality
        arr = np.frombuffer(self._pool.get_random_bytes(512), dtype=np.uint8)
        self._last_quality = EntropySource._quick_quality(arr, "pool")
        self._total_bytes += 512

        # Update state dict
        for ss in self._pool.sources:
            self._source_states[ss.source.name] = {
                "healthy": ss.healthy,
                "entropy": ss.last_entropy,
                "bytes": ss.total_bytes,
                "time": ss.last_collect_time,
                "failures": ss.failures,
            }

    def _update_ui(self) -> None:
        """Update all UI widgets."""
        self._update_table()
        self._update_chart()
        self._update_rng()
        self._update_pool()

    def _update_table(self) -> None:
        table = self.query_one("#source-table", DataTable)
        for name in self._source_names:
            state = self._source_states.get(name, {})
            enabled = name in self._enabled
            healthy = state.get("healthy", False)
            shannon = state.get("entropy", 0.0)
            nbytes = state.get("bytes", 0)
            collect_time = state.get("time", 0.0)

            icon = "✅" if enabled and healthy else "⏸️" if not enabled else "❌"
            bar = _entropy_bar_text(shannon) if enabled else "░" * 12
            spark = _sparkline(list(self._source_shannon.get(name, [])))

            if collect_time < 0.01:
                time_str = "<10ms"
            elif collect_time < 1.0:
                time_str = f"{collect_time*1000:.0f}ms"
            else:
                time_str = f"{collect_time:.1f}s"

            hist = self._source_history.get(name, deque())
            val01 = f"{hist[-1]:.3f}" if hist else "-.---"

            try:
                table.update_cell(name, table.columns[0].key, icon)
                table.update_cell(name, table.columns[2].key, f"{shannon:.2f}" if enabled else "off")
                table.update_cell(name, table.columns[3].key, bar)
                table.update_cell(name, table.columns[4].key, f"{nbytes:,}")
                table.update_cell(name, table.columns[5].key, time_str)
                table.update_cell(name, table.columns[6].key, spark)
                table.update_cell(name, table.columns[7].key, val01)
            except Exception:
                pass

    def _update_chart(self) -> None:
        chart_widget = self.query_one("#chart", ChartWidget)
        try:
            size = chart_widget.size
            w = max(40, size.width - 4)
            h = max(8, size.height - 2)
            chart_str = _render_chart(self._source_history, self._enabled, width=w, height=h)
            chart_widget.update(chart_str)
        except Exception as e:
            chart_widget.update(f"[dim]Chart loading... ({e})[/dim]")

    def _update_rng(self) -> None:
        rng_widget = self.query_one("#rng-display", RNGDisplay)
        elapsed = time.monotonic() - self._start_time
        rate = self._total_bytes / elapsed if elapsed > 0 else 0

        text = Text()
        text.append("  🎲 RNG Output\n", style="bold magenta")
        text.append(f"  Int:   ", style="dim")
        text.append(f"{self._last_rng_int}\n", style="bold green")
        text.append(f"  Float: ", style="dim")
        text.append(f"{self._last_rng_float:.15f}\n", style="bold cyan")
        text.append(f"  Hex:   ", style="dim")
        text.append(f"{self._last_rng_hex[:32]}...\n", style="bold yellow")
        text.append(f"  Rate:  {rate:,.0f} B/s", style="dim")
        text.append(f"  │  Cycle: {self._total_collections}", style="dim")
        text.append(f"  │  Speed: {self.refresh_rate:.1f}s", style="dim")

        rng_widget.update(text)

    def _update_pool(self) -> None:
        pool_widget = self.query_one("#pool-status", PoolStatus)
        q = self._last_quality
        grade = q.get("grade", "?")
        score = q.get("quality_score", 0)
        shannon = q.get("shannon_entropy", 0)

        grade_colors = {"A": "green", "B": "blue", "C": "yellow", "D": "red", "F": "bright_red"}
        gc = grade_colors.get(grade, "white")

        enabled_count = len(self._enabled)
        healthy = sum(1 for n in self._enabled if self._source_states.get(n, {}).get("healthy"))
        elapsed = time.monotonic() - self._start_time

        text = Text()
        text.append("  🏊 Pool Status\n", style="bold green")
        text.append(f"  Grade: ", style="dim")
        text.append(f"{grade}", style=f"bold {gc}")
        text.append(f"  Score: {score:.0f}/100", style="dim")
        text.append(f"  Shannon: {shannon:.2f}/8.0\n")
        text.append(f"  Sources: {healthy}/{enabled_count} healthy", style="dim")
        text.append(f"  │  Total: {self._total_bytes:,} B", style="dim")
        text.append(f"  │  Uptime: {int(elapsed)}s", style="dim")

        pool_widget.update(text)

    # ── Actions ──

    def action_toggle_source(self) -> None:
        """Toggle the currently selected source."""
        table = self.query_one("#source-table", DataTable)
        if table.cursor_row is None:
            return
        row_key = table.get_row_at(table.cursor_row)
        # Get the source name from row key
        try:
            name = list(self._source_states.keys())[table.cursor_row]
        except (IndexError, KeyError):
            name = self._source_names[table.cursor_row] if table.cursor_row < len(self._source_names) else None
        if not name:
            return

        if name in self._enabled:
            self._enabled.discard(name)
            self.notify(f"Disabled: {name}", severity="warning")
        else:
            self._enabled.add(name)
            self.notify(f"Enabled: {name}", severity="information")

    def action_show_info(self) -> None:
        """Show/hide physics info for selected source."""
        info_panel = self.query_one("#info-panel", InfoPanel)
        table = self.query_one("#source-table", DataTable)

        if info_panel.has_class("visible"):
            info_panel.remove_class("visible")
            return

        if table.cursor_row is None or table.cursor_row >= len(self._source_names):
            return

        name = self._source_names[table.cursor_row]
        src = self._source_objects.get(name)
        if not src:
            return

        text = Text()
        text.append(f"  📖 {name}", style="bold magenta")
        cat = getattr(src, "category", "other")
        text.append(f"  [{cat}]\n", style="dim")
        text.append(f"  {src.description}\n\n", style="italic")
        physics = getattr(src, "physics", "")
        if physics:
            text.append(f"  ⚛️  ", style="bold cyan")
            text.append(physics, style="")
        else:
            text.append("  (no physics description available)", style="dim")

        info_panel.update(text)
        info_panel.add_class("visible")

    def action_enable_all(self) -> None:
        self._enabled = set(self._source_names)
        self.notify("All sources enabled")

    def action_disable_all(self) -> None:
        self._enabled.clear()
        self.notify("All sources disabled", severity="warning")

    def action_cycle_speed(self) -> None:
        self._speed_idx = (self._speed_idx + 1) % len(self._speeds)
        self.refresh_rate = self._speeds[self._speed_idx]
        self.notify(f"Refresh: {self.refresh_rate:.1f}s")

    def action_force_refresh(self) -> None:
        threading.Thread(target=self._collect_cycle, daemon=True).start()
        self.notify("Collecting...")

    def on_unmount(self) -> None:
        self._stop.set()


# ── Entry point for CLI ──

class EntropyMonitor:
    """Wrapper for CLI integration."""

    def __init__(self, refresh_rate: float = 1.0, sources_filter: list[str] | None = None):
        self.refresh_rate = refresh_rate
        self.sources_filter = sources_filter

    def run(self) -> None:
        app = EntropyMonitorApp(
            initial_rate=self.refresh_rate,
            sources_filter=self.sources_filter,
        )
        app.run()
