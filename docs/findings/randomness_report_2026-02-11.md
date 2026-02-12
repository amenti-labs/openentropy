# 🔬 Esoteric Entropy — Randomness Test Report

**Generated:** 2026-02-11 19:31:58
**Machine:** erais-Mac-mini.local (arm64, Darwin 24.6.0)
**Python:** 3.14.3
**Tests in battery:** 31

## Summary

| Rank | Source | Score | Grade | Passed | Samples |
|------|--------|-------|-------|--------|---------|
| 1 | process_table | 36.3 | ⚠️ D | 11/31 | 5,000 |

---

## Detailed Results

### process_table
**Score: 36.3/100** | **Grade: D** | **Passed: 11/31** | **Samples: 5,000**

| Test | Result | Grade | P-Value | Statistic | Details |
|------|--------|-------|---------|-----------|---------|
| Monobit Frequency | ❌ | ❌ F | 0.000013 | 4.3600 | S=-872, n=40000 |
| Block Frequency | ✅ | ✅ A | 0.769760 | 293.2812 | blocks=312, M=128 |
| Byte Frequency | ❌ | ❌ F | 0.000000 | 4460.2240 | n=5000, expected_per_bin=19.5 |
| Runs Test | ❌ | ❌ F | 0.000000 | 0.0000 | Pre-test failed: proportion=0.4891 |
| Longest Run of Ones | ❌ | ❌ F | 0.000000 | 2579.9657 | blocks=5000, M=8 |
| Serial Test | ❌ | ❌ F | 0.000000 | 99.3728 | m=4, n_bits=20000 |
| Approximate Entropy | ❌ | ❌ F | 0.000000 | 137.6576 | ApEn=0.996559, m=3 |
| DFT Spectral | ✅ | ✅ A | 0.963403 | 0.0459 | peaks_below_threshold=19001/20000 |
| Spectral Flatness | ❌ | ❌ F | N/A | 0.0826 | flatness=0.0826 (1.0=white noise) |
| Shannon Entropy | ✅ | ✅ A | N/A | 7.7464 | 7.7464 / 8.0 bits (96.8%) |
| Min-Entropy | ❌ | ⚠️ C | N/A | 4.4612 | 4.4612 / 8.0 bits (55.8%) |
| Permutation Entropy | ✅ | ✅ A | N/A | 0.9979 | PE=4.5755/4.5850 = 0.9979 |
| Compression Ratio | ✅ | ✅ A | N/A | 0.9846 | 4923/5000 = 0.9846 |
| Kolmogorov Complexity | ✅ | ✅ A | N/A | 0.9846 | K≈0.9846, spread=0.0020 |
| Autocorrelation | ❌ | ❌ F | 0.000000 | 0.0756 | violations=13/50, max|r|=0.0756 |
| Serial Correlation | ❌ | ⚠️ D | 0.000295 | 0.0512 | r=0.051191, z=3.6197 |
| Lag-N Correlation | ❌ | ⚠️ D | N/A | 0.0567 | lag1=0.0512, lag2=0.0567, lag4=-0.0021, lag8=-0.0285, lag16=-0.0228, lag32=-0.0069 |
| Cross-Correlation | ❌ | ❌ F | 0.000010 | 0.0881 | r=0.088131 (even vs odd bytes) |
| Kolmogorov-Smirnov | ❌ | ❌ F | 0.000000 | 0.0660 | D=0.065988, n=5000 |
| Anderson Darling Test | ❌ | ❌ F | N/A | 0.0000 | Error: 'SignificanceResult' object has no attribute 'critical_values' |
| Overlapping Template | ❌ | ❌ F | 0.000000 | 5.8007 | count=2219, expected=2500 |
| Non-overlapping Template | ✅ | ✅ A | 0.128511 | 1.5200 | count=2443, expected=2500 |
| Maurer's Universal | ✅ | ✅ A | 0.501001 | 5.2028 | fn=5.2028, expected=5.2177, L=6 |
| Binary Matrix Rank | ❌ | ❌ F | 0.000000 | 96.0416 | N=39, full=39, full-1=0 |
| Linear Complexity | ❌ | ❌ F | 0.000000 | 115.9600 | N=200, mean_complexity=100.0 |
| Cumulative Sums | ❌ | ❌ F | 0.000012 | 876.0000 | max|S|=876.0, n=40000 |
| Random Excursions | ✅ | ✅ B | N/A | 50.0000 | Only 50 cycles (need 500 for reliable test) |
| Birthday Spacing | ✅ | ✅ A | 1.000000 | 2359.0000 | duplicates=2359, lambda=59627.39, m=2500 |
| Bit Avalanche | ❌ | ❌ F | 0.000004 | 3.9080 | mean_diff=3.908/8 bits, expected=4.0 |
| Monte Carlo Pi | ✅ | ⚠️ C | N/A | 3.2784 | π≈3.278400, error=4.3547% |
| Mean & Variance | ❌ | ❌ F | 0.000000 | 6.8781 | mean=120.31 (exp 127.5), var=5303.3 (exp 5461.2) |

---
