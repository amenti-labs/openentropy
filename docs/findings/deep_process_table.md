# 🔬 Esoteric Entropy — Randomness Test Report

**Generated:** 2026-02-11 19:27:47
**Machine:** erais-Mac-mini.local (arm64, Darwin 24.6.0)
**Python:** 3.14.3
**Tests in battery:** 31

## Summary

| Rank | Source | Score | Grade | Passed | Samples |
|------|--------|-------|-------|--------|---------|
| 1 | process_table | 32.3 | ⚠️ D | 10/31 | 10,000 |

---

## Detailed Results

### process_table
**Score: 32.3/100** | **Grade: D** | **Passed: 10/31** | **Samples: 10,000**

| Test | Result | Grade | P-Value | Statistic | Details |
|------|--------|-------|---------|-----------|---------|
| Monobit Frequency | ❌ | ❌ F | 0.000000 | 5.3104 | S=-1502, n=80000 |
| Block Frequency | ✅ | ✅ A | 0.674676 | 608.4688 | blocks=625, M=128 |
| Byte Frequency | ❌ | ❌ F | 0.000000 | 9074.3552 | n=10000, expected_per_bin=39.1 |
| Runs Test | ❌ | ❌ F | 0.000000 | 0.0000 | Pre-test failed: proportion=0.4906 |
| Longest Run of Ones | ❌ | ❌ F | 0.000000 | 4919.6698 | blocks=10000, M=8 |
| Serial Test | ❌ | ❌ F | 0.000000 | 449.5028 | m=4, n_bits=80000 |
| Approximate Entropy | ❌ | ❌ F | 0.000000 | 600.6670 | ApEn=0.996246, m=3 |
| DFT Spectral | ✅ | ✅ A | 0.183447 | 1.3302 | peaks_below_threshold=38041/40000 |
| Spectral Flatness | ❌ | ❌ F | N/A | 0.0816 | flatness=0.0816 (1.0=white noise) |
| Shannon Entropy | ✅ | ✅ A | N/A | 7.7563 | 7.7563 / 8.0 bits (97.0%) |
| Min-Entropy | ❌ | ⚠️ C | N/A | 4.3248 | 4.3248 / 8.0 bits (54.1%) |
| Permutation Entropy | ✅ | ✅ A | N/A | 0.9988 | PE=4.5794/4.5850 = 0.9988 |
| Compression Ratio | ✅ | ✅ A | N/A | 0.9759 | 9759/10000 = 0.9759 |
| Kolmogorov Complexity | ✅ | ✅ A | N/A | 0.9759 | K≈0.9759, spread=0.0030 |
| Autocorrelation | ❌ | ❌ F | 0.000000 | 0.0898 | violations=14/50, max|r|=0.0898 |
| Serial Correlation | ❌ | ❌ F | 0.000000 | 0.0684 | r=0.068385, z=6.8385 |
| Lag-N Correlation | ❌ | ⚠️ D | N/A | 0.0684 | lag1=0.0684, lag2=0.0392, lag4=0.0122, lag8=-0.0123, lag16=-0.0177, lag32=-0.0063 |
| Cross-Correlation | ❌ | ❌ F | 0.000000 | 0.0742 | r=0.074155 (even vs odd bytes) |
| Kolmogorov-Smirnov | ❌ | ❌ F | 0.000000 | 0.0666 | D=0.066645, n=10000 |
| Anderson Darling Test | ❌ | ❌ F | N/A | 0.0000 | Error: 'SignificanceResult' object has no attribute 'critical_values' |
| Overlapping Template | ❌ | ❌ F | 0.000000 | 9.8272 | count=4327, expected=5000 |
| Non-overlapping Template | ❌ | ❌ F | 0.000026 | 4.2049 | count=4777, expected=5000 |
| Maurer's Universal | ✅ | ✅ A | 0.474296 | 5.2068 | fn=5.2068, expected=5.2177, L=6 |
| Binary Matrix Rank | ❌ | ❌ F | 0.000000 | 192.0831 | N=78, full=78, full-1=0 |
| Linear Complexity | ❌ | ❌ F | 0.000000 | 69.0760 | N=160, mean_complexity=250.3 |
| Cumulative Sums | ❌ | ❌ F | 0.000000 | 1502.0000 | max|S|=1502.0, n=80000 |
| Random Excursions | ✅ | ✅ B | N/A | 7.0000 | Only 7 cycles (need 500 for reliable test) |
| Birthday Spacing | ✅ | ✅ A | 1.000000 | 4908.0000 | duplicates=4908, lambda=477040.97, m=5000 |
| Bit Avalanche | ❌ | ❌ F | 0.000000 | 3.9207 | mean_diff=3.921/8 bits, expected=4.0 |
| Monte Carlo Pi | ✅ | ⚠️ C | N/A | 3.2776 | π≈3.277600, error=4.3292% |
| Mean & Variance | ❌ | ❌ F | 0.000000 | 8.5182 | mean=121.20 (exp 127.5), var=5368.7 (exp 5461.2) |

---
