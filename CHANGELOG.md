# Changelog

## 0.1.0 — 2026-02-11

Initial release.

### Features
- 15 entropy source implementations (timing, sysctl, vmstat, network, disk, memory, GPU, process, audio, camera, sensor, bluetooth)
- Sysctl kernel counter source — auto-discovers 50+ fluctuating keys on macOS
- Multi-source entropy pool with SHA-256 conditioning and health monitoring
- Statistical test suite (Shannon, min-entropy, chi-squared, permutation entropy, compression)
- Conditioning algorithms (Von Neumann debiasing, XOR folding, SHA-256)
- CLI tool: `scan`, `probe`, `bench`, `stream`, `report`, `pool`
- Platform auto-detection for macOS (Linux partial support)
- Thread-safe pool with graceful degradation
