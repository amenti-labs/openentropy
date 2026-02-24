# Quantum Entropy Sources for OpenEntropy

**Date**: 2026-02-22
**Status**: Active — legitimate quantum sources only

## Summary

This module provides entropy sources with documented quantum physics origins.
Three sources previously in this module were removed after review:

- `ssd_tunneling` — removed (measured filesystem I/O timing, not tunneling events)
- `avalanche_noise` — removed (CPU timing jitter, no actual PN junction access)
- `vacuum_fluctuations` — removed (CPU timing jitter rebranded as zero-point energy)

## Current Sources

| Source | Physics | Quantum Fraction | Hardware |
|--------|---------|------------------|----------|
| `cosmic_muon` | Cosmic ray muon detection | 0.95 | Camera sensor |
| `radioactive_decay` | Nuclear decay detection | 0.99 | Camera sensor |
| `nvme_iokit_sensors` | NVMe controller clock domain crossing | 0.30 | Apple Silicon + IOKit |
| `nvme_smart_thermal` | NVMe temperature ADC noise | 0.35 | Apple Silicon + IOKit |
| `nvme_raw_device` | Raw block device reads (bypass FS) | 0.40 | /dev/rdiskN or /dev/nvme* |
| `nvme_passthrough_linux` | NVMe admin ioctl passthrough | 0.45 | Linux + /dev/nvme0 |
| `multi_source_quantum` | XOR combiner | varies | All above |

## NVMe Direct-Access Sources

These are the most novel part of the project. They progressively strip away
software layers to get closer to the NAND flash physics:

1. **IOKit sensors** — reads NVMe properties through macOS kernel framework
2. **SMART thermal** — polls temperature ADC for Johnson-Nyquist noise in LSBs
3. **Raw device** — `libc::open(/dev/rdiskN)` + `F_NOCACHE` bypasses filesystem
4. **Passthrough** — `ioctl(NVME_IOCTL_ADMIN_CMD)` bypasses filesystem AND block layer

Prior art for flash TRNG (Ray & Milenkovic, IEEE 2018) reads cell threshold
voltages directly. We can't do that from userspace, but stripping software
layers is a novel approach for entropy extraction.

## Important Caveat

**Statistical tests CANNOT certify quantum randomness.**

A PRNG scores 99%+ on NIST tests but is deterministic.
These sources are "quantum" based on physics arguments, not statistical tests.
Only Bell inequality tests can certify quantum randomness.

## Prior Art

- [MRNG (2023)](https://www.mdpi.com/1099-4300/25/6/854) — cosmic ray CMOS detection
- [Sanguinetti et al. (2014)](https://arxiv.org/abs/1405.0435) — phone camera QRNG
- [Ray & Milenkovic (2018)](https://ieeexplore.ieee.org/document/8283603/) — flash memory TRNG
- [HotBits (1996-2022)](https://www.fourmilab.ch/hotbits/) — radioactive decay RNG
