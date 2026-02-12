"""CLI for esoteric-entropy."""

from __future__ import annotations

import sys
import time

import click
import numpy as np

from esoteric_entropy import __version__


@click.group()
@click.version_option(__version__)
def main() -> None:
    """🔬 esoteric-entropy — your computer is a quantum noise observatory."""


# ────────────────────────────────────────────────────────────
# Discovery & benchmarking
# ────────────────────────────────────────────────────────────


@main.command()
def scan() -> None:
    """Discover available entropy sources on this machine."""
    from esoteric_entropy.platform import detect_available_sources, platform_info

    info = platform_info()
    click.echo(f"Platform: {info['system']} {info['machine']} (Python {info['python']})")
    click.echo()

    sources = detect_available_sources()
    click.echo(f"Found {len(sources)} available entropy source(s):\n")
    for src in sources:
        click.echo(f"  ✅ {src.name:<25} {src.description}")
    if not sources:
        click.echo("  (none found)")


@main.command()
@click.argument("source_name")
def probe(source_name: str) -> None:
    """Test a specific source and show quality stats."""
    from esoteric_entropy.platform import detect_available_sources

    sources = detect_available_sources()
    matches = [s for s in sources if source_name in s.name]
    if not matches:
        click.echo(f"Source '{source_name}' not found. Run 'scan' to list sources.")
        sys.exit(1)

    src = matches[0]
    click.echo(f"Probing: {src.name}")
    click.echo(f"  {src.description}")
    click.echo()

    t0 = time.monotonic()
    quality = src.entropy_quality()
    elapsed = time.monotonic() - t0

    click.echo(f"  Grade:           {quality.get('grade', '?')}")
    click.echo(f"  Samples:         {quality.get('samples', 0):,}")
    click.echo(f"  Shannon entropy: {quality.get('shannon_entropy', 0):.4f} / 8.0 bits")
    click.echo(f"  Compression:     {quality.get('compression_ratio', 0):.4f}")
    click.echo(f"  Unique values:   {quality.get('unique_values', 0)}")
    click.echo(f"  Time:            {elapsed:.3f}s")


@main.command()
def bench() -> None:
    """Benchmark all available sources with a ranked report."""
    from esoteric_entropy.platform import detect_available_sources

    sources = detect_available_sources()
    click.echo(f"Benchmarking {len(sources)} sources...\n")

    results = []
    for src in sources:
        try:
            t0 = time.monotonic()
            q = src.entropy_quality()
            q["time"] = round(time.monotonic() - t0, 3)
            q["name"] = src.name
            results.append(q)
            grade = q.get("grade", "?")
            click.echo(f"  {grade} {src.name:<25} H={q.get('shannon_entropy',0):.3f}  {q['time']:.2f}s")
        except Exception as e:
            click.echo(f"  ✗ {src.name:<25} error: {e}")

    results.sort(key=lambda r: r.get("quality_score", 0), reverse=True)
    click.echo(f"\n{'='*60}")
    click.echo(f"{'Source':<25} {'Grade':>5} {'Score':>6} {'Shannon':>8} {'Compress':>9}")
    click.echo("-" * 60)
    for r in results:
        click.echo(
            f"{r['name']:<25} {r.get('grade','?'):>5} "
            f"{r.get('quality_score',0):>6.1f} "
            f"{r.get('shannon_entropy',0):>7.3f} "
            f"{r.get('compression_ratio',0):>8.3f}"
        )


# ────────────────────────────────────────────────────────────
# Stream — continuous entropy to stdout
# ────────────────────────────────────────────────────────────


@main.command()
@click.option("--rate", default=0, type=int, help="Bytes/sec rate limit (0 = unlimited).")
@click.option("--format", "fmt", type=click.Choice(["raw", "hex", "base64"]), default="raw",
              help="Output format.")
@click.option("--sources", "source_filter", default=None, help="Comma-separated source name filter.")
@click.option("--bytes", "n_bytes", default=0, type=int, help="Total bytes (0 = infinite).")
def stream(rate: int, fmt: str, source_filter: str | None, n_bytes: int) -> None:
    """Stream entropy to stdout.

    Examples:

        esoteric-entropy stream --format raw | dd of=/tmp/entropy.bin bs=1024 count=100

        esoteric-entropy stream --format hex --bytes 256

        esoteric-entropy stream --rate 1024 --format raw > /dev/null
    """
    import base64

    from esoteric_entropy.pool import EntropyPool

    pool = _make_pool(source_filter)
    chunk_size = min(rate, 4096) if rate > 0 else 4096
    total = 0

    try:
        while True:
            if 0 < n_bytes <= total:
                break
            want = chunk_size if n_bytes == 0 else min(chunk_size, n_bytes - total)
            data = pool.get_random_bytes(want)

            if fmt == "raw":
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            elif fmt == "hex":
                sys.stdout.write(data.hex())
                sys.stdout.flush()
            elif fmt == "base64":
                sys.stdout.write(base64.b64encode(data).decode())
                sys.stdout.flush()

            total += len(data)

            if rate > 0:
                time.sleep(len(data) / rate)
    except (BrokenPipeError, KeyboardInterrupt):
        pass


# ────────────────────────────────────────────────────────────
# Device — named pipe (FIFO) entropy feeder
# ────────────────────────────────────────────────────────────


@main.command()
@click.argument("path", default="/tmp/esoteric-rng")
@click.option("--buffer-size", default=4096, help="Write buffer size in bytes.")
@click.option("--sources", "source_filter", default=None, help="Comma-separated source name filter.")
def device(path: str, buffer_size: int, source_filter: str | None) -> None:
    """Create a named pipe (FIFO) that continuously provides entropy.

    Use with ollama-auxrng:

        esoteric-entropy device /tmp/esoteric-rng &

        OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
    """
    import os
    import signal

    pool = _make_pool(source_filter)

    # Create FIFO
    if os.path.exists(path):
        if not _is_fifo(path):
            click.echo(f"Error: {path} exists and is not a FIFO.", err=True)
            sys.exit(1)
    else:
        os.mkfifo(path)
        click.echo(f"Created FIFO: {path}")

    click.echo(f"Feeding entropy to {path} (buffer={buffer_size}B)")
    click.echo("Press Ctrl+C to stop.")

    def _cleanup(signum, frame):
        try:
            os.unlink(path)
        except OSError:
            pass
        sys.exit(0)

    signal.signal(signal.SIGTERM, _cleanup)
    signal.signal(signal.SIGINT, _cleanup)

    try:
        while True:
            # open() blocks until a reader connects
            with open(path, "wb") as fifo:
                try:
                    while True:
                        data = pool.get_random_bytes(buffer_size)
                        fifo.write(data)
                        fifo.flush()
                except BrokenPipeError:
                    continue  # reader disconnected, wait for next
    except KeyboardInterrupt:
        pass
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


# ────────────────────────────────────────────────────────────
# Server — HTTP API (ANU QRNG compatible)
# ────────────────────────────────────────────────────────────


@main.command()
@click.option("--port", default=8042, help="Port to listen on.")
@click.option("--host", default="127.0.0.1", help="Bind address.")
@click.option("--sources", "source_filter", default=None, help="Comma-separated source name filter.")
def server(port: int, host: str, source_filter: str | None) -> None:
    """Start an HTTP entropy server (ANU QRNG API compatible).

    Endpoints:

        GET /api/v1/random?length=N&type=hex16|uint8|uint16

        GET /health

        GET /sources

        GET /pool/status

    Compatible with quantum-llama.cpp QRNG backend.
    """
    from esoteric_entropy.http_server import run_server

    pool = _make_pool(source_filter)
    click.echo(f"🔬 Esoteric Entropy Server v{__version__}")
    click.echo(f"   Listening on http://{host}:{port}")
    click.echo(f"   Sources: {len(pool.sources)}")
    click.echo(f"   API: /api/v1/random?length=N&type=hex16|uint8|uint16")
    click.echo()
    run_server(pool, host=host, port=port)


# ────────────────────────────────────────────────────────────
# Report & Pool (existing)
# ────────────────────────────────────────────────────────────


@main.command()
@click.option("--samples", default=10000, help="Number of bytes to collect per source.")
@click.option("--source", "source_name", default=None, help="Test a single source.")
@click.option("--output", "output_path", default=None, help="Output path for report.")
def report(samples: int, source_name: str | None, output_path: str | None) -> None:
    """Full NIST-inspired randomness test battery with Markdown report."""
    from datetime import datetime

    from esoteric_entropy.platform import detect_available_sources
    from esoteric_entropy.report import generate_full_report
    from esoteric_entropy.test_suite import calculate_quality_score, run_all_tests

    sources = detect_available_sources()
    if source_name:
        sources = [s for s in sources if source_name.lower() in s.name.lower()]
        if not sources:
            click.echo(f"Source '{source_name}' not found.")
            sys.exit(1)

    click.echo(f"🔬 Running full test battery on {len(sources)} source(s), {samples:,} samples each...\n")

    source_results = {}
    for src in sources:
        try:
            click.echo(f"  Collecting from {src.name}...", nl=False)
            t0 = time.monotonic()
            data = src.collect(samples)
            click.echo(f" {len(data):,} bytes", nl=False)
            results = run_all_tests(data)
            elapsed = time.monotonic() - t0
            score = calculate_quality_score(results)
            passed = sum(1 for r in results if r.passed)
            click.echo(f" → {score:.0f}/100 ({passed}/{len(results)} passed) [{elapsed:.1f}s]")
            source_results[src.name] = (data, results)
        except Exception as e:
            click.echo(f" ✗ error: {e}")

    if not source_results:
        click.echo("No sources produced data.")
        sys.exit(1)

    if output_path is None:
        from pathlib import Path

        docs = Path(__file__).parent.parent / "docs" / "findings"
        output_path = str(docs / f"randomness_report_{datetime.now():%Y-%m-%d}.md")

    report_text = generate_full_report(source_results, output_path)
    click.echo(f"\n📄 Report saved to: {output_path}")

    click.echo(f"\n{'='*60}")
    click.echo(f"{'Source':<25} {'Score':>6} {'Grade':>6} {'Pass':>8}")
    click.echo(f"{'-'*60}")
    for name, (data, results) in sorted(
        source_results.items(),
        key=lambda x: calculate_quality_score(x[1][1]),
        reverse=True,
    ):
        score = calculate_quality_score(results)
        grade = "A" if score >= 80 else "B" if score >= 60 else "C" if score >= 40 else "D" if score >= 20 else "F"
        passed = sum(1 for r in results if r.passed)
        click.echo(f"  {name:<23} {score:>5.1f} {grade:>6} {passed:>4}/{len(results)}")


@main.command()
@click.option("--refresh", default=1.0, type=float, help="Refresh rate in seconds.")
@click.option("--sources", "source_filter", default=None, help="Comma-separated source name filter.")
def monitor(refresh: float, source_filter: str | None) -> None:
    """Live interactive entropy dashboard.

    Shows real-time source health, pool throughput, entropy visualization,
    and rolling quality scores. Press Ctrl+C to stop.

    Examples:

        esoteric-entropy monitor

        esoteric-entropy monitor --refresh 0.5

        esoteric-entropy monitor --sources timing,silicon
    """
    from esoteric_entropy.monitor import EntropyMonitor

    sources = source_filter.split(",") if source_filter else None
    mon = EntropyMonitor(refresh_rate=refresh, sources_filter=sources)
    mon.run()


@main.command()
def pool() -> None:
    """Run the entropy pool and output quality metrics."""
    from esoteric_entropy.pool import EntropyPool

    p = EntropyPool.auto()
    click.echo(f"Pool created with {len(p.sources)} sources")
    click.echo("Collecting entropy...")

    raw = p.collect_all()
    click.echo(f"Raw entropy: {raw:,} bytes")

    output = p.get_random_bytes(1024)
    arr = np.frombuffer(output, dtype=np.uint8)
    from esoteric_entropy.sources.base import EntropySource as _ES

    h = _ES._quick_shannon(arr)
    click.echo("\nConditioned output: 1024 bytes")
    click.echo(f"  Shannon entropy: {h:.4f} / 8.0 bits/byte")

    p.print_health()


# ────────────────────────────────────────────────────────────
# Helpers
# ────────────────────────────────────────────────────────────


def _make_pool(source_filter: str | None = None):
    """Build an EntropyPool, optionally filtering sources by name."""
    from esoteric_entropy.pool import EntropyPool

    if source_filter is None:
        return EntropyPool.auto()

    from esoteric_entropy.platform import detect_available_sources

    names = {n.strip().lower() for n in source_filter.split(",")}
    pool = EntropyPool()
    for src in detect_available_sources():
        if any(n in src.name.lower() for n in names):
            pool.add_source(src)
    if not pool.sources:
        click.echo(f"Warning: no sources matched filter '{source_filter}'", err=True)
        return EntropyPool.auto()
    return pool


def _is_fifo(path: str) -> bool:
    import os
    import stat

    try:
        return stat.S_ISFIFO(os.stat(path).st_mode)
    except OSError:
        return False
