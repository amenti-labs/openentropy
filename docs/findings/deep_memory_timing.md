# 🔬 Esoteric Entropy — Randomness Test Report

**Generated:** 2026-02-11 19:27:31
**Machine:** erais-Mac-mini.local (arm64, Darwin 24.6.0)
**Python:** 3.14.3
**Tests in battery:** 31

## Summary

| Rank | Source | Score | Grade | Passed | Samples |
|------|--------|-------|-------|--------|---------|
| 1 | memory_timing | 38.7 | ⚠️ D | 9/31 | 10,000 |

---

## Detailed Results

### memory_timing
**Score: 38.7/100** | **Grade: D** | **Passed: 9/31** | **Samples: 10,000**

| Test | Result | Grade | P-Value | Statistic | Details |
|------|--------|-------|---------|-----------|---------|
| Monobit Frequency | ❌ | ❌ F | 0.000002 | 4.7659 | S=1348, n=80000 |
| Block Frequency | ✅ | ✅ A | 0.964958 | 562.5000 | blocks=625, M=128 |
| Byte Frequency | ❌ | ❌ F | 0.000000 | 42001.6896 | n=10000, expected_per_bin=39.1 |
| Runs Test | ❌ | ❌ F | 0.000000 | 0.0000 | Pre-test failed: proportion=0.5084 |
| Longest Run of Ones | ❌ | ❌ F | 0.000000 | 7804.1013 | blocks=10000, M=8 |
| Serial Test | ❌ | ❌ F | 0.000000 | 671.9812 | m=4, n_bits=80000 |
| Approximate Entropy | ❌ | ❌ F | 0.000000 | 977.1942 | ApEn=0.993893, m=3 |
| DFT Spectral | ✅ | ✅ A | 0.284322 | -1.0707 | peaks_below_threshold=37967/40000 |
| Spectral Flatness | ❌ | ❌ F | N/A | 0.0761 | flatness=0.0761 (1.0=white noise) |
| Shannon Entropy | ❌ | ⚠️ C | N/A | 5.8876 | 5.8876 / 8.0 bits (73.6%) |
| Min-Entropy | ❌ | ⚠️ C | N/A | 4.5096 | 4.5096 / 8.0 bits (56.4%) |
| Permutation Entropy | ✅ | ✅ A | N/A | 0.9979 | PE=4.5753/4.5850 = 0.9979 |
| Compression Ratio | ❌ | ⚠️ C | N/A | 0.7494 | 7494/10000 = 0.7494 |
| Kolmogorov Complexity | ❌ | ⚠️ C | N/A | 0.7494 | K≈0.7494, spread=0.0018 |
| Autocorrelation | ❌ | ⚠️ C | 0.001140 | 0.0313 | violations=8/50, max|r|=0.0313 |
| Serial Correlation | ✅ | ✅ A | 0.575661 | 0.0056 | r=0.005597, z=0.5597 |
| Lag-N Correlation | ❌ | ⚠️ C | N/A | 0.0283 | lag1=0.0056, lag2=0.0216, lag4=0.0237, lag8=0.0283, lag16=0.0104, lag32=0.0053 |
| Cross-Correlation | ✅ | ✅ A | 0.746141 | 0.0046 | r=0.004579 (even vs odd bytes) |
| Kolmogorov-Smirnov | ❌ | ❌ F | 0.000000 | 0.0677 | D=0.067727, n=10000 |
| Anderson Darling Test | ❌ | ❌ F | N/A | 0.0000 | Error: 'SignificanceResult' object has no attribute 'critical_values' |
| Overlapping Template | ✅ | ✅ A | 0.597109 | 0.5286 | count=5036, expected=5000 |
| Non-overlapping Template | ❌ | ❌ F | 0.000001 | 4.8272 | count=5256, expected=5000 |
| Maurer's Universal | ❌ | ❌ F | 0.000000 | 4.9741 | fn=4.9741, expected=5.2177, L=6 |
| Binary Matrix Rank | ❌ | ❌ F | 0.000000 | 185.2245 | N=78, full=77, full-1=1 |
| Linear Complexity | ❌ | ❌ F | 0.000000 | 64.9986 | N=160, mean_complexity=250.2 |
| Cumulative Sums | ❌ | ❌ F | 0.000001 | 1371.0000 | max|S|=1371.0, n=80000 |
| Random Excursions | ✅ | ✅ B | N/A | 431.0000 | Only 431 cycles (need 500 for reliable test) |
| Birthday Spacing | ✅ | ✅ A | 1.000000 | 4902.0000 | duplicates=4902, lambda=476837.16, m=5000 |
| Bit Avalanche | ✅ | ✅ B | 0.017840 | 3.9665 | mean_diff=3.966/8 bits, expected=4.0 |
| Monte Carlo Pi | ❌ | ⚠️ C | N/A | 2.9480 | π≈2.948000, error=6.1622% |
| Mean & Variance | ❌ | ❌ F | 0.000000 | 9.1388 | mean=134.25 (exp 127.5), var=5604.0 (exp 5461.2) |

---
