# Sensor Jitter Findings — 2025-02-11

## Clock Jitter
- **Samples:** 1000
- **Mean:** 34.7 ns
- **Std:** 22.6 ns
- **Range:** 0–208 ns
- **LSB(3bit) Shannon entropy:** 1.8229 / 3.0 bits (60.8%)

## Sleep/Interrupt Jitter
- **Samples:** 500
- **Target sleep:** 100μs
- **Mean overshoot:** 773,985 ns (~774μs)
- **Std:** 137,178 ns
- **LSB(4bit) Shannon entropy:** 3.9825 / 4.0 bits (99.6%) ← excellent

## Assessment
- Clock jitter has moderate entropy — LSBs are somewhat biased
- Sleep jitter is an excellent entropy source — near-maximum entropy in bottom 4 bits
- The large sleep overshoot (774μs vs 100μs target) is expected on macOS — scheduler quantum effects
- Sleep jitter rate: ~500 samples * 4 bits / 0.5s ≈ 4 Kbit/s raw (before conditioning)

## Issues
- No CPU temperature data (requires sudo for powermetrics)
