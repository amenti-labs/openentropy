# Deep Entropy Benchmark — 2026-02-11 15:43:26

## Summary

| Source | Bytes | Entropy | Compression | Chi² | Perm.Ent | Status |
|--------|------:|--------:|------------:|-----:|---------:|--------|
| os.urandom (reference) | 10000 | 99.8% | 100.1% | 250 | 1.000 | 🟢 |
| NVMe SMART Jitter | 792 | 75.2% | 80.8% | 3396 | 0.974 | 🔴 |
| GPU Compute Jitter | 788 | 49.3% | 50.8% | 38793 | 0.786 | 🔴 |

## Explorer Status

| Explorer | Status | Time |
|----------|--------|------|
| SMC Sensor Galaxy | ⚠️ no_data | 6.8s |
| EMI Audio Coupling | ⚠️ no_data | 1.4s |
| GPU Compute Jitter | ✅ success | 2.9s |
| NVMe SMART Jitter | ✅ success | 10.1s |
| Mach Timing Deep | ❌ error ([Errno 2] No such file or directory: 'sysctl') | 0.0s |
| BLE Ambient Noise | ❌ error ([Errno 2] No such file or directory: 'system_profi) | 8.5s |
| IORegistry Deep | ❌ error ([Errno 2] No such file or directory: 'ioreg') | 0.0s |
| Camera Quantum | ⚠️ no_data | 102.5s |
| Cross-Domain Beat | ❌ error ([Errno 2] No such file or directory: 'screencaptur) | 0.1s |

## Methodology

Each source was sampled independently and tested with:
- **Byte Entropy**: Shannon entropy of byte distribution (max 8.0 bits)
- **Compression Ratio**: zlib level 9 (1.0 = incompressible = ideal)
- **Chi-Squared**: Uniformity test (< 293 at p=0.05 for 255 df)
- **Permutation Entropy**: Ordinal pattern complexity (normalized, 1.0 = ideal)
- **Approximate Entropy**: Regularity measure (higher = more random)
- **Cumulative Sums**: Bias detection in bit sequence
- **Runs Test**: Sequential pattern detection