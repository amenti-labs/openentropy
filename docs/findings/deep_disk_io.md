# 🔬 Esoteric Entropy — Randomness Test Report

**Generated:** 2026-02-11 19:27:23
**Machine:** erais-Mac-mini.local (arm64, Darwin 24.6.0)
**Python:** 3.14.3
**Tests in battery:** 31

## Summary

| Rank | Source | Score | Grade | Passed | Samples |
|------|--------|-------|-------|--------|---------|
| 1 | disk_io | 62.9 | ✅ B | 17/31 | 10,000 |

---

## Detailed Results

### disk_io
**Score: 62.9/100** | **Grade: B** | **Passed: 17/31** | **Samples: 10,000**

| Test | Result | Grade | P-Value | Statistic | Details |
|------|--------|-------|---------|-----------|---------|
| Monobit Frequency | ✅ | ✅ A | 0.909922 | 0.1131 | S=32, n=80000 |
| Block Frequency | ✅ | ✅ A | 0.979704 | 554.7500 | blocks=625, M=128 |
| Byte Frequency | ❌ | ❌ F | 0.000000 | 24010.8288 | n=10000, expected_per_bin=39.1 |
| Runs Test | ✅ | ✅ A | 0.381591 | 0.8750 | runs=39826, expected=40001 |
| Longest Run of Ones | ❌ | ❌ F | 0.000000 | 6426.5699 | blocks=10000, M=8 |
| Serial Test | ❌ | ⚠️ C | 0.001184 | 25.6956 | m=4, n_bits=80000 |
| Approximate Entropy | ❌ | ❌ F | 0.000011 | 37.0086 | ApEn=0.999769, m=3 |
| DFT Spectral | ✅ | ✅ A | 1.000000 | 0.0000 | peaks_below_threshold=38000/40000 |
| Spectral Flatness | ❌ | ❌ F | N/A | 0.0796 | flatness=0.0796 (1.0=white noise) |
| Shannon Entropy | ❌ | ⚠️ C | N/A | 6.5894 | 6.5894 / 8.0 bits (82.4%) |
| Min-Entropy | ❌ | ⚠️ C | N/A | 5.5395 | 5.5395 / 8.0 bits (69.2%) |
| Permutation Entropy | ✅ | ✅ A | N/A | 0.9994 | PE=4.5821/4.5850 = 0.9994 |
| Compression Ratio | ❌ | ⚠️ C | N/A | 0.8419 | 8419/10000 = 0.8419 |
| Kolmogorov Complexity | ❌ | ⚠️ C | N/A | 0.8419 | K≈0.8419, spread=0.0014 |
| Autocorrelation | ✅ | ✅ A | 0.456187 | 0.0257 | violations=2/50, max|r|=0.0257 |
| Serial Correlation | ✅ | ✅ A | 0.468711 | 0.0072 | r=-0.007246, z=-0.7246 |
| Lag-N Correlation | ✅ | ✅ B | N/A | 0.0126 | lag1=-0.0072, lag2=-0.0126, lag4=-0.0000, lag8=0.0067, lag16=0.0080, lag32=-0.0056 |
| Cross-Correlation | ✅ | ✅ A | 0.991904 | 0.0001 | r=-0.000144 (even vs odd bytes) |
| Kolmogorov-Smirnov | ❌ | ⚠️ C | 0.001022 | 0.0194 | D=0.019449, n=10000 |
| Anderson Darling Test | ❌ | ❌ F | N/A | 0.0000 | Error: 'SignificanceResult' object has no attribute 'critical_values' |
| Overlapping Template | ❌ | ⚠️ D | 0.000954 | 3.3037 | count=5226, expected=5000 |
| Non-overlapping Template | ✅ | ✅ A | 0.835683 | 0.2074 | count=4989, expected=5000 |
| Maurer's Universal | ✅ | ✅ A | 0.407541 | 5.2051 | fn=5.2051, expected=5.2177, L=6 |
| Binary Matrix Rank | ❌ | ❌ F | 0.000000 | 192.0831 | N=78, full=78, full-1=0 |
| Linear Complexity | ❌ | ❌ F | 0.000000 | 88.6051 | N=160, mean_complexity=250.3 |
| Cumulative Sums | ✅ | ✅ A | 0.319220 | 280.0000 | max|S|=280.0, n=80000 |
| Random Excursions | ✅ | ✅ B | N/A | 177.0000 | Only 177 cycles (need 500 for reliable test) |
| Birthday Spacing | ✅ | ✅ A | 1.000000 | 4800.0000 | duplicates=4800, lambda=477317.86, m=5000 |
| Bit Avalanche | ✅ | ✅ B | 0.030475 | 4.0306 | mean_diff=4.031/8 bits, expected=4.0 |
| Monte Carlo Pi | ✅ | ✅ A | N/A | 3.1160 | π≈3.116000, error=0.8146% |
| Mean & Variance | ✅ | ✅ A | 0.342424 | 0.9494 | mean=128.20 (exp 127.5), var=5493.5 (exp 5461.2) |

---
