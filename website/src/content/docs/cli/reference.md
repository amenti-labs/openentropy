---
title: 'CLI Reference'
description: 'Complete command reference for the openentropy CLI'
---

Complete command reference for `openentropy-cli`.

If you want guided workflows first, see:

- [Security Validation](/openentropy/guides/security-validation/)
- [Research Methodology](/openentropy/guides/research-methodology/)

## Analysis Profiles

Profiles are convenience presets that configure multiple flags at once.
Value flags (for example `--samples`, `--conditioning`) override profile defaults.
Boolean flags are additive (OR semantics): profile-enabled booleans stay enabled.
Profiles are available on `analyze`, `sessions`, and `compare`.

| Profile | Audience | Samples | Conditioning | Entropy | NIST Report | Cross-Corr | Trials | Chaos |
|---------|----------|---------|-------------|---------|-------------|------------|--------|-------|
| `quick` | Any | 10,000 | raw | — | — | — | — | — |
| `standard` | Any (default) | 50,000 | raw | — | — | — | — | — |
| `deep` | Research | 100,000 | raw | ✓ | — | ✓ | ✓ | ✓ |
| `security` | Security | 50,000 | sha256 | ✓ | ✓ | — | — | — |

The table reflects `analyze` defaults. `sessions` uses the profile's analysis
toggles (`entropy`, `trials`, and whether analysis is implied). `compare` uses
profile defaults only for min-entropy output.

## Workflows

### Security — Validate Entropy for Cryptographic Use

| Goal | Command |
|------|---------|
| Full security audit | `openentropy analyze --profile security` |
| NIST report to file | `openentropy analyze --profile security --output audit.md` |
| Rank sources by min-entropy | `openentropy bench --rank-by min_entropy` |
| Stream conditioned bytes | `openentropy stream --conditioning sha256` |
| Validate recorded session | `openentropy sessions <path> --profile security` |

Security analysis uses SHA-256 conditioning, enables the min-entropy
breakdown, and runs the NIST SP 800-22 test battery with pass/fail
grading and p-values. This validates that conditioned output is suitable
for seeding CSPRNGs or generating cryptographic keys.

**Key flags:** `--entropy`, `--report`, `--conditioning sha256`

### Research — Characterize Raw Hardware Noise

| Goal | Command |
|------|---------|
| Deep noise characterization | `openentropy analyze --profile deep` |
| Record a session | `openentropy record <src> --duration 5m` |
| Analyze with PEAR trials | `openentropy sessions <path> --profile deep` |
| Compare two sessions | `openentropy compare <a> <b> --profile deep` |
| Calibrate before recording | `openentropy record <src> --calibrate --duration 5m` |

Research analysis uses raw conditioning to preserve the hardware noise
signal, enables cross-source correlation analysis, and runs PEAR-style
200-bit trial analysis with Z-scores and cumulative deviation tracking.

**Key flags:** `--trials`, `--cross-correlation`, `--conditioning raw`, `--calibrate`

## `scan` — Discover sources

```bash
openentropy scan
openentropy scan --telemetry
```

## `bench` — Benchmark sources

```bash
openentropy bench                    # standard profile on fast sources
openentropy bench --profile quick    # faster confidence pass
openentropy bench --profile deep     # higher-confidence benchmark
openentropy bench all                # all sources
openentropy bench clock_jitter       # filter by name
openentropy bench --rank-by throughput
openentropy bench --telemetry
openentropy bench --output bench.json
openentropy bench --no-pool
openentropy bench --qcicada-mode raw
```

`bench --output` JSON includes optional `telemetry_v1` when `--telemetry` is enabled.
Treat telemetry as run context (load, thermal/frequency/memory signals), not as an entropy score.

## `stream` — Continuous output

```bash
openentropy stream --format hex --bytes 256
openentropy stream --format raw --bytes 1024 | your-program
openentropy stream --format base64 --rate 1024            # rate-limited
openentropy stream --conditioning raw --format raw        # no conditioning
openentropy stream --conditioning vonneumann --format hex # debiased only
openentropy stream --conditioning sha256 --format hex     # full conditioning (default)
openentropy stream --qcicada-mode samples --format hex --bytes 64
```

## `monitor` — Interactive TUI dashboard

```bash
openentropy monitor
openentropy monitor --telemetry
```

| Key | Action |
|-----|--------|
| ↑/↓ or j/k | Navigate source list |
| Space/Enter | Select source (starts collecting) |
| g | Cycle chart mode (time series, histogram, random walk, etc.) |
| c | Cycle conditioning mode (SHA-256 → Von Neumann → Raw) |
| n | Cycle sample size |
| +/- | Adjust refresh rate |
| Tab | Compare two sources (select one, move cursor to another, Tab) |
| p | Pause/resume collection |
| r | Start/stop recording |
| s | Export snapshot |
| q/Esc | Quit |

## `stream --fifo` — Named pipe (FIFO)

```bash
openentropy stream --fifo /tmp/openentropy-rng
# Another terminal: head -c 32 /tmp/openentropy-rng | xxd
```

## `server` — HTTP entropy server

```bash
openentropy server --port 8080
openentropy server --port 8080 --allow-raw    # enable raw output
openentropy server --port 8080 --telemetry    # print startup telemetry snapshot
```

> Security: The server binds to `127.0.0.1` (localhost only) by default. It has no
> authentication or rate limiting. Do not expose to untrusted networks without
> adding a reverse proxy with appropriate access controls.

```bash
curl "http://localhost:8080/api/v1/random?length=256&type=uint8"
curl "http://localhost:8080/health"
curl "http://localhost:8080/sources?telemetry=true"
curl "http://localhost:8080/pool/status?telemetry=true"
```

## `analyze` — Statistical source analysis

Run statistical analysis on entropy sources. The analysis system includes
five categories — forensic, entropy breakdown, chaos theory, trial analysis,
and cross-correlation — controlled by profiles or individual flags. See
[Analysis System](/openentropy/concepts/analysis/) for detailed explanations
of each category, interpretation guides, and verdict thresholds.

```bash
openentropy analyze                          # standard forensic analysis
openentropy analyze --profile quick          # fast 10K-sample check
openentropy analyze --profile security       # NIST battery + entropy + sha256
openentropy analyze --profile deep           # 100K samples + entropy + cross-corr
openentropy analyze --profile deep --report  # deep forensic + NIST battery
openentropy analyze --entropy                # include min-entropy breakdown
openentropy analyze --cross-correlation --output analysis.json
openentropy analyze --telemetry --output analysis.json
openentropy analyze --qcicada-mode sha256 --output analysis.json
openentropy analyze --chaos                          # chaos theory analysis (Hurst, Lyapunov, D₂, BiEntropy, epiplexity)
openentropy analyze --profile deep --chaos           # deep profile already enables chaos
```

## `analyze --report` — NIST test battery

```bash
openentropy analyze --report
openentropy analyze --profile security --output report.md
openentropy analyze --report mach_timing --samples 50000
openentropy analyze --report --telemetry --output report.md
```

## `record` — Record entropy sessions

```bash
openentropy record clock_jitter --duration 30s
openentropy record qcicada --duration 5m --tag experiment:baseline --note "5-min baseline"
openentropy record all --duration 1m --analyze --telemetry
openentropy record qcicada --calibrate --duration 5m  # PEAR-style calibration gate before recording
openentropy record qcicada --qcicada-mode raw --duration 5m
```

`--calibrate` runs a PEAR-style calibration check on each source before recording
begins. Sources must pass: `|Z| < 2.0`, bit bias < 0.005, Shannon entropy > 7.9,
Z-score std in `[0.85, 1.15]`. Recording is blocked if any source fails.

## `sessions` — List and analyze recorded sessions

```bash
openentropy sessions                                    # list all sessions
openentropy sessions sessions/<id> --analyze            # full statistical analysis
openentropy sessions sessions/<id> --profile deep       # implies --analyze + entropy + trials
openentropy sessions sessions/<id> --profile security   # implies --analyze + entropy
openentropy sessions sessions/<id> --analyze --entropy  # include min-entropy breakdown
openentropy sessions sessions/<id> --trials             # PEAR-style trial analysis
openentropy sessions sessions/<id> --trials --output results.json
```

`--trials` runs PEAR-style 200-bit trial analysis: per-trial Z-scores, cumulative
deviation tracking, terminal Z, effect size, and two-tailed p-value.
Non-standard profiles (`quick`, `deep`, `security`) imply analysis only when a
session path is provided. In list mode (`openentropy sessions` with no path),
profile flags are ignored.

## `compare` — Differential session analysis

```bash
openentropy compare sessions/<id-a> sessions/<id-b>
openentropy compare sessions/<id-a> sessions/<id-b> --profile deep     # implies --entropy
openentropy compare sessions/<id-a> sessions/<id-b> --profile security # implies --entropy
openentropy compare sessions/<id-a> sessions/<id-b> --entropy
openentropy compare sessions/<id-a> sessions/<id-b> --output comparison.json
```

Runs forensic comparison (Shannon, min-entropy, bit bias, spectral, stationarity),
two-sample tests (KS, chi-squared, Mann-Whitney), temporal anomaly detection,
multi-lag autocorrelation, Markov transitions, digram analysis, run-length
distributions, effect sizes, and PEAR-style trial comparison with Stouffer Z
meta-analysis. For `compare`, profile presets currently control only whether
min-entropy breakdown is enabled by default.
