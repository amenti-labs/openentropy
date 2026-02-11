#!/usr/bin/env python3
"""
Benchmark runner for esoteric-entropy project.

Discovers all explore/*.py scripts, runs each one, collects entropy output,
runs statistical tests, and produces a ranked report card.
"""
import subprocess
import sys
import os
import glob
import time
import json
import numpy as np
from datetime import datetime

# Add project root to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from analysis.entropy_tests import full_report, print_report


def discover_explorers():
    """Find all exploration scripts."""
    pattern = os.path.join(os.path.dirname(__file__), 'explore', '*.py')
    scripts = sorted(glob.glob(pattern))
    return scripts


def run_explorer(script_path, timeout=120):
    """Run an explorer script and capture its output and entropy file."""
    name = os.path.splitext(os.path.basename(script_path))[0]
    print(f"\n{'='*60}")
    print(f"  Running: {name}")
    print(f"{'='*60}")
    
    start = time.time()
    try:
        result = subprocess.run(
            [sys.executable, script_path],
            capture_output=True, text=True,
            timeout=timeout,
            cwd=os.path.dirname(script_path) or '.',
        )
        elapsed = time.time() - start
        
        output = result.stdout + result.stderr
        print(output[:2000])  # Print first 2000 chars
        
        return {
            'name': name,
            'status': 'success' if result.returncode == 0 else 'error',
            'returncode': result.returncode,
            'elapsed': elapsed,
            'output': output,
        }
    except subprocess.TimeoutExpired:
        elapsed = time.time() - start
        print(f"  TIMEOUT after {timeout}s")
        return {
            'name': name,
            'status': 'timeout',
            'elapsed': elapsed,
            'output': '',
        }
    except Exception as e:
        elapsed = time.time() - start
        print(f"  ERROR: {e}")
        return {
            'name': name,
            'status': 'error',
            'elapsed': elapsed,
            'output': str(e),
        }


def find_entropy_file(explorer_name, search_dirs=None):
    """Find the entropy .bin file produced by an explorer."""
    if search_dirs is None:
        search_dirs = [
            os.path.join(os.path.dirname(__file__), 'explore'),
            os.path.dirname(__file__),
            '.',
        ]
    
    patterns = [
        f'entropy_{explorer_name}*.bin',
        f'entropy_*{explorer_name.split("_")[0]}*.bin',
        f'{explorer_name}*.bin',
    ]
    
    for d in search_dirs:
        for pattern in patterns:
            matches = glob.glob(os.path.join(d, pattern))
            if matches:
                return matches[0]
    
    return None


def load_entropy_data(filepath):
    """Load entropy data from a binary file."""
    try:
        data = np.fromfile(filepath, dtype=np.uint8)
        if len(data) > 0:
            return data
    except Exception:
        pass
    return None


def generate_report_markdown(results, reports):
    """Generate a markdown benchmark report."""
    date = datetime.now().strftime('%Y-%m-%d')
    
    lines = [
        f"# Esoteric Entropy Benchmark — {date}\n",
        f"Generated: {datetime.now().isoformat()}\n",
    ]
    
    # Summary table
    lines.append("## Summary\n")
    lines.append("| Source | Status | Grade | Quality | Shannon | Min-Entropy | Samples | Time |")
    lines.append("|--------|--------|-------|---------|---------|-------------|---------|------|")
    
    # Sort by quality score
    ranked = sorted(reports.items(), key=lambda x: x[1].get('quality_score', 0), reverse=True)
    
    for name, report in ranked:
        run_info = next((r for r in results if r['name'] == name), {})
        status = run_info.get('status', 'unknown')
        grade = report.get('grade', '-')
        quality = report.get('quality_score', 0)
        shannon = report.get('shannon', {}).get('shannon_entropy', 0)
        min_ent = report.get('min_entropy', {}).get('min_entropy', 0)
        n = report.get('n_samples', 0)
        elapsed = run_info.get('elapsed', 0)
        
        lines.append(f"| {name} | {status} | {grade} | {quality:.1f} | {shannon:.3f} | {min_ent:.3f} | {n:,} | {elapsed:.1f}s |")
    
    # Add failed explorers
    for result in results:
        if result['name'] not in reports:
            lines.append(f"| {result['name']} | {result['status']} | - | - | - | - | - | {result['elapsed']:.1f}s |")
    
    lines.append("")
    
    # Detailed reports
    lines.append("## Detailed Reports\n")
    for name, report in ranked:
        lines.append(f"### {name} (Grade: {report.get('grade', '?')})\n")
        
        s = report.get('basic_stats', {})
        lines.append(f"- **Samples:** {report.get('n_samples', 0):,}")
        lines.append(f"- **Unique values:** {s.get('n_unique', 0)}")
        lines.append(f"- **Shannon entropy:** {report.get('shannon', {}).get('shannon_entropy', 0):.4f}")
        lines.append(f"- **Min-entropy:** {report.get('min_entropy', {}).get('min_entropy', 0):.4f}")
        
        chi = report.get('chi_squared', {})
        lines.append(f"- **Chi² uniformity:** p={chi.get('p_value', 0):.4f} {'✓' if chi.get('uniform') else '✗'}")
        
        sc = report.get('serial_correlation', {})
        if sc.get('serial_correlation') is not None:
            lines.append(f"- **Serial correlation:** r={sc['serial_correlation']:.4f} {'✓' if sc.get('independent') else '✗'}")
        
        rt = report.get('runs_test', {})
        lines.append(f"- **Runs test:** {'✓ random' if rt.get('random') else '✗ non-random'}")
        
        sp = report.get('spectral', {})
        lines.append(f"- **Spectral flatness:** {sp.get('spectral_flatness', 0):.4f} {'✓' if sp.get('white_noise_like') else '✗'}")
        
        ac = report.get('autocorrelation', {})
        lines.append(f"- **Autocorrelation:** {ac.get('n_significant', 0)} significant lags")
        lines.append("")
    
    # Notes
    lines.append("## Notes\n")
    lines.append("- Quality score is 0-100 (weighted average of all tests)")
    lines.append("- Grade: A≥80, B≥60, C≥40, D≥20, F<20")
    lines.append("- Entropy values in bits")
    lines.append("- ✓ = passes test at 1% significance level")
    lines.append(f"- Platform: macOS {os.uname().release} ({os.uname().machine})")
    lines.append("")
    
    return '\n'.join(lines)


def main():
    print("╔══════════════════════════════════════════╗")
    print("║  Esoteric Entropy Benchmark Runner       ║")
    print("╚══════════════════════════════════════════╝\n")
    
    scripts = discover_explorers()
    print(f"Found {len(scripts)} explorers:")
    for s in scripts:
        print(f"  - {os.path.basename(s)}")
    
    # Run each explorer
    results = []
    for script in scripts:
        result = run_explorer(script, timeout=90)
        results.append(result)
    
    # Collect and analyze entropy files
    print(f"\n{'='*60}")
    print("  Statistical Analysis")
    print(f"{'='*60}\n")
    
    reports = {}
    explore_dir = os.path.join(os.path.dirname(__file__), 'explore')
    
    # Search for all entropy*.bin files
    bin_files = glob.glob(os.path.join(explore_dir, 'entropy_*.bin'))
    bin_files += glob.glob(os.path.join(os.path.dirname(__file__), 'entropy_*.bin'))
    
    for bin_file in set(bin_files):
        data = load_entropy_data(bin_file)
        if data is not None and len(data) > 20:
            name = os.path.splitext(os.path.basename(bin_file))[0]
            # Map back to explorer name
            explorer_name = name.replace('entropy_', '')
            
            print(f"\nAnalyzing: {name} ({len(data)} samples)")
            report = full_report(data, explorer_name)
            print_report(report)
            reports[explorer_name] = report
    
    # Generate markdown report
    date = datetime.now().strftime('%Y-%m-%d')
    report_md = generate_report_markdown(results, reports)
    
    findings_dir = os.path.join(os.path.dirname(__file__), 'docs', 'findings')
    os.makedirs(findings_dir, exist_ok=True)
    report_path = os.path.join(findings_dir, f'benchmark_{date}.md')
    
    with open(report_path, 'w') as f:
        f.write(report_md)
    
    print(f"\n{'='*60}")
    print(f"  Benchmark report saved to: {report_path}")
    print(f"{'='*60}")
    
    # Print ranked summary
    if reports:
        print(f"\n  Ranked Sources:")
        ranked = sorted(reports.items(), key=lambda x: x[1].get('quality_score', 0), reverse=True)
        for i, (name, report) in enumerate(ranked, 1):
            print(f"    {i}. {report['grade']} ({report['quality_score']:.0f}) — {name}")


if __name__ == '__main__':
    main()
