#!/usr/bin/env python3
"""
Deep Benchmark — run ALL deep explorers, collect results, produce ranked report.
"""
import sys
import os
import time
import traceback
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from analysis.deep_entropy_tests import full_test_suite, print_results, generate_heatmap_text

EXPLORERS = [
    ('SMC Sensor Galaxy', 'explore.smc_sensor_galaxy', 'explore/entropy_smc_galaxy.bin'),
    ('EMI Audio Coupling', 'explore.emi_audio_coupling', 'explore/entropy_emi_audio.bin'),
    ('GPU Compute Jitter', 'explore.gpu_compute_jitter', 'explore/entropy_gpu_jitter.bin'),
    ('NVMe SMART Jitter', 'explore.nvme_smart_jitter', 'explore/entropy_nvme_smart.bin'),
    ('Mach Timing Deep', 'explore.mach_timing_deep', 'explore/entropy_mach_timing.bin'),
    ('BLE Ambient Noise', 'explore.ble_ambient_noise', 'explore/entropy_ble_noise.bin'),
    ('IORegistry Deep', 'explore.ioregistry_deep', 'explore/entropy_ioregistry.bin'),
    ('Camera Quantum', 'explore.camera_quantum', 'explore/entropy_camera_quantum.bin'),
    ('Cross-Domain Beat', 'explore.cross_domain_beat', 'explore/entropy_cross_domain.bin'),
]

def run_all():
    print("=" * 70)
    print("  DEEP ENTROPY BENCHMARK — All Sources")
    print(f"  {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print("=" * 70)
    
    results = []
    explorer_results = {}
    
    for name, module_path, output_file in EXPLORERS:
        print(f"\n{'#'*60}")
        print(f"# Running: {name}")
        print(f"{'#'*60}")
        
        start = time.time()
        try:
            mod = __import__(module_path, fromlist=['run'])
            res = mod.run(output_file=output_file)
            elapsed = time.time() - start
            
            explorer_results[name] = {
                'status': 'success' if res else 'no_data',
                'result': res,
                'elapsed': elapsed,
            }
            
            # Run entropy tests on output
            if os.path.exists(output_file) and os.path.getsize(output_file) > 0:
                test_results = full_test_suite(output_file, label=name)
                print_results(test_results)
                results.append(test_results)
            
        except Exception as e:
            elapsed = time.time() - start
            print(f"\n[ERROR] {name}: {e}")
            traceback.print_exc()
            explorer_results[name] = {
                'status': 'error',
                'error': str(e),
                'elapsed': elapsed,
            }
    
    # Also test reference (os.urandom)
    ref_data = os.urandom(10000)
    ref_file = 'explore/entropy_reference.bin'
    with open(ref_file, 'wb') as f:
        f.write(ref_data)
    ref_results = full_test_suite(ref_file, label="os.urandom (reference)")
    print_results(ref_results)
    results.append(ref_results)
    
    # Generate report
    print("\n\n" + "=" * 70)
    print("  ENTROPY QUALITY HEATMAP")
    print("=" * 70)
    
    # Sort by entropy quality
    results.sort(key=lambda r: r['byte_entropy']['efficiency'], reverse=True)
    heatmap = generate_heatmap_text(results)
    print(heatmap)
    
    # Save report
    report = generate_report(results, explorer_results)
    os.makedirs('docs/findings', exist_ok=True)
    report_file = f"docs/findings/deep_benchmark_{datetime.now().strftime('%Y-%m-%d')}.md"
    with open(report_file, 'w') as f:
        f.write(report)
    print(f"\n📄 Report saved to: {report_file}")
    
    return results

def generate_report(test_results, explorer_results):
    """Generate markdown report."""
    now = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
    
    lines = [
        f"# Deep Entropy Benchmark — {now}",
        "",
        "## Summary",
        "",
        f"| Source | Bytes | Entropy | Compression | Chi² | Perm.Ent | Status |",
        f"|--------|------:|--------:|------------:|-----:|---------:|--------|",
    ]
    
    for r in test_results:
        ent = r['byte_entropy']['efficiency'] * 100
        comp = r['compression']['ratio'] * 100 if r['compression']['ratio'] else 0
        chi2 = r['chi_squared']['chi2'] if r['chi_squared']['chi2'] else 0
        pe = r['permutation'].get('normalized_pe', 0) or 0
        score = (ent/100 + min(comp/100, 1) + (1 if chi2 < 293 else 0) + pe) / 4
        status = '🟢' if score > 0.85 else '🟡' if score > 0.7 else '🔴'
        lines.append(f"| {r['label']} | {r['size_bytes']} | {ent:.1f}% | {comp:.1f}% | {chi2:.0f} | {pe:.3f} | {status} |")
    
    lines.extend([
        "",
        "## Explorer Status",
        "",
        "| Explorer | Status | Time |",
        "|----------|--------|------|",
    ])
    
    for name, info in explorer_results.items():
        status = '✅' if info['status'] == 'success' else '⚠️' if info['status'] == 'no_data' else '❌'
        err = f" ({info.get('error', '')[:50]})" if info['status'] == 'error' else ''
        lines.append(f"| {name} | {status} {info['status']}{err} | {info['elapsed']:.1f}s |")
    
    lines.extend([
        "",
        "## Methodology",
        "",
        "Each source was sampled independently and tested with:",
        "- **Byte Entropy**: Shannon entropy of byte distribution (max 8.0 bits)",
        "- **Compression Ratio**: zlib level 9 (1.0 = incompressible = ideal)",
        "- **Chi-Squared**: Uniformity test (< 293 at p=0.05 for 255 df)",
        "- **Permutation Entropy**: Ordinal pattern complexity (normalized, 1.0 = ideal)",
        "- **Approximate Entropy**: Regularity measure (higher = more random)",
        "- **Cumulative Sums**: Bias detection in bit sequence",
        "- **Runs Test**: Sequential pattern detection",
    ])
    
    return '\n'.join(lines)

if __name__ == '__main__':
    run_all()
