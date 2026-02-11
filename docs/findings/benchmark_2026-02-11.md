# Esoteric Entropy Benchmark — 2026-02-11

Generated: 2026-02-11T15:31:21.938562

## Summary

| Source | Status | Grade | Quality | Shannon | Min-Entropy | Samples | Time |
|--------|--------|-------|---------|---------|-------------|---------|------|
| network_timing | success | A | 82.8 | 3.923 | 3.152 | 160 | 20.4s |
| cpu_thermal | unknown | B | 67.8 | 3.169 | 2.265 | 500 | 0.0s |
| accelerometer | unknown | B | 66.1 | 3.110 | 2.343 | 6,000 | 0.0s |
| disk_io | unknown | B | 65.9 | 3.336 | 2.081 | 2,500 | 0.0s |
| camera_dark | unknown | B | 63.1 | 1.976 | 1.599 | 921,600 | 0.0s |
| memory_timing | success | C | 54.2 | 2.909 | 2.097 | 102,500 | 0.1s |
| jitter | unknown | D | 33.6 | 3.024 | 1.432 | 1,500 | 0.0s |
| accelerometer_noise | success | - | - | - | - | - | 0.1s |
| camera_dark_current | success | - | - | - | - | - | 0.2s |
| cpu_thermal_jitter | success | - | - | - | - | - | 0.1s |
| disk_io_jitter | success | - | - | - | - | - | 0.1s |
| mic_thermal_noise | error | - | - | - | - | - | 0.1s |
| sensor_jitter | success | - | - | - | - | - | 0.5s |
| wifi_rssi_entropy | error | - | - | - | - | - | 32.3s |

## Detailed Reports

### network_timing (Grade: A)

- **Samples:** 160
- **Unique values:** 16
- **Shannon entropy:** 3.9227
- **Min-entropy:** 3.1520
- **Chi² uniformity:** p=0.2955 ✓
- **Serial correlation:** r=0.0437 ✓
- **Runs test:** ✓ random
- **Spectral flatness:** 0.5351 ✓
- **Autocorrelation:** 2 significant lags

### cpu_thermal (Grade: B)

- **Samples:** 500
- **Unique values:** 13
- **Shannon entropy:** 3.1687
- **Min-entropy:** 2.2653
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=0.0173 ✓
- **Runs test:** ✓ random
- **Spectral flatness:** 0.5885 ✓
- **Autocorrelation:** 3 significant lags

### accelerometer (Grade: B)

- **Samples:** 6,000
- **Unique values:** 16
- **Shannon entropy:** 3.1105
- **Min-entropy:** 2.3425
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=0.0068 ✓
- **Runs test:** ✓ random
- **Spectral flatness:** 0.5650 ✓
- **Autocorrelation:** 0 significant lags

### disk_io (Grade: B)

- **Samples:** 2,500
- **Unique values:** 16
- **Shannon entropy:** 3.3365
- **Min-entropy:** 2.0807
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=0.0057 ✓
- **Runs test:** ✓ random
- **Spectral flatness:** 0.5446 ✓
- **Autocorrelation:** 9 significant lags

### camera_dark (Grade: B)

- **Samples:** 921,600
- **Unique values:** 4
- **Shannon entropy:** 1.9757
- **Min-entropy:** 1.5993
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=-0.0007 ✓
- **Runs test:** ✗ non-random
- **Spectral flatness:** 0.5613 ✓
- **Autocorrelation:** 4 significant lags

### memory_timing (Grade: C)

- **Samples:** 102,500
- **Unique values:** 16
- **Shannon entropy:** 2.9090
- **Min-entropy:** 2.0970
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=0.0076 ✓
- **Runs test:** ✗ non-random
- **Spectral flatness:** 0.5536 ✓
- **Autocorrelation:** 50 significant lags

### jitter (Grade: D)

- **Samples:** 1,500
- **Unique values:** 16
- **Shannon entropy:** 3.0242
- **Min-entropy:** 1.4318
- **Chi² uniformity:** p=0.0000 ✗
- **Serial correlation:** r=0.4618 ✗
- **Runs test:** ✗ non-random
- **Spectral flatness:** 0.3082 ✗
- **Autocorrelation:** 50 significant lags

## Notes

- Quality score is 0-100 (weighted average of all tests)
- Grade: A≥80, B≥60, C≥40, D≥20, F<20
- Entropy values in bits
- ✓ = passes test at 1% significance level
- Platform: macOS 24.6.0 (arm64)
