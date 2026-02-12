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


@main.command()
@click.option("--bytes", "n_bytes", default=256, help="Number of bytes to output.")
def stream(n_bytes: int) -> None:
    """Stream mixed entropy to stdout."""
    from esoteric_entropy.pool import EntropyPool

    pool = EntropyPool.auto()
    data = pool.get_random_bytes(n_bytes)
    sys.stdout.buffer.write(data)


@main.command()
@click.option("--samples", default=10000, help="Number of bytes to collect per source.")
@click.option("--source", "source_name", default=None, help="Test a single source.")
@click.option("--output", "output_path", default=None, help="Output path for report.")
def report(samples: int, source_name: str | None, output_path: str | None) -> None:
    """Full NIST-inspired randomness test battery with Markdown report."""
    from datetime import datetime
    from esoteric_entropy.platform import detect_available_sources
    from esoteric_entropy.test_suite import run_all_tests, calculate_quality_score
    from esoteric_entropy.report import generate_full_report

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

    # Print summary
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
