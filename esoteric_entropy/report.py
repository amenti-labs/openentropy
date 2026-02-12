"""Report generator for esoteric-entropy randomness test battery."""

from __future__ import annotations

import os
import platform
import time
from datetime import datetime
from pathlib import Path

import numpy as np

from esoteric_entropy.test_suite import TestResult, run_all_tests, calculate_quality_score


def _grade_icon(grade: str) -> str:
    return {"A": "✅", "B": "✅", "C": "⚠️", "D": "⚠️", "F": "❌"}.get(grade, "❓")


def _pass_icon(passed: bool) -> str:
    return "✅" if passed else "❌"


def generate_source_report(name: str, data: np.ndarray, results: list[TestResult]) -> str:
    """Generate markdown section for a single source."""
    score = calculate_quality_score(results)
    passed = sum(1 for r in results if r.passed)
    total = len(results)
    grade = "A" if score >= 80 else "B" if score >= 60 else "C" if score >= 40 else "D" if score >= 20 else "F"

    lines = [
        f"### {name}",
        f"**Score: {score:.1f}/100** | **Grade: {grade}** | **Passed: {passed}/{total}** | **Samples: {len(data):,}**\n",
        "| Test | Result | Grade | P-Value | Statistic | Details |",
        "|------|--------|-------|---------|-----------|---------|",
    ]

    for r in results:
        p_str = f"{r.p_value:.6f}" if r.p_value is not None else "N/A"
        lines.append(
            f"| {r.name} | {_pass_icon(r.passed)} | {_grade_icon(r.grade)} {r.grade} "
            f"| {p_str} | {r.statistic:.4f} | {r.details} |"
        )
    lines.append("")
    return "\n".join(lines)


def generate_full_report(
    source_results: dict[str, tuple[np.ndarray, list[TestResult]]],
    output_path: str | Path | None = None,
) -> str:
    """Generate complete markdown report for all sources."""
    now = datetime.now()

    # Summary table
    summary_rows = []
    for name, (data, results) in sorted(
        source_results.items(),
        key=lambda x: calculate_quality_score(x[1][1]),
        reverse=True,
    ):
        score = calculate_quality_score(results)
        passed = sum(1 for r in results if r.passed)
        total = len(results)
        grade = "A" if score >= 80 else "B" if score >= 60 else "C" if score >= 40 else "D" if score >= 20 else "F"
        summary_rows.append((name, score, grade, passed, total, len(data)))

    lines = [
        f"# 🔬 Esoteric Entropy — Randomness Test Report",
        f"",
        f"**Generated:** {now.strftime('%Y-%m-%d %H:%M:%S')}",
        f"**Machine:** {platform.node()} ({platform.machine()}, {platform.system()} {platform.release()})",
        f"**Python:** {platform.python_version()}",
        f"**Tests in battery:** {len(source_results[list(source_results.keys())[0]][1]) if source_results else 0}",
        f"",
        f"## Summary",
        f"",
        f"| Rank | Source | Score | Grade | Passed | Samples |",
        f"|------|--------|-------|-------|--------|---------|",
    ]

    for i, (name, score, grade, passed, total, samples) in enumerate(summary_rows, 1):
        lines.append(
            f"| {i} | {name} | {score:.1f} | {_grade_icon(grade)} {grade} "
            f"| {passed}/{total} | {samples:,} |"
        )

    lines.append("")
    lines.append("---")
    lines.append("")
    lines.append("## Detailed Results")
    lines.append("")

    for name, (data, results) in sorted(
        source_results.items(),
        key=lambda x: calculate_quality_score(x[1][1]),
        reverse=True,
    ):
        lines.append(generate_source_report(name, data, results))
        lines.append("---\n")

    report = "\n".join(lines)

    if output_path:
        path = Path(output_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(report)

    return report
