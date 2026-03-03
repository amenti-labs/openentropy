---
title: 'Verdict System'
description: 'PASS/WARN/FAIL thresholds for forensic and chaos metrics'
---

Each source report includes automated verdicts for forensic and chaos metrics.
Verdicts are computed in `openentropy_core::verdict`.

## Verdict Values

- `PASS`: in expected range
- `WARN`: borderline or potentially concerning
- `FAIL`: outside acceptable range
- `N/A`: metric unavailable or invalid

## Forensic Verdict Thresholds

| Metric | PASS | WARN | FAIL |
|--------|------|------|------|
| Autocorrelation | `max |r| <= 0.05` | `<= 0.15` | `> 0.15` |
| Spectral flatness | `>= 0.75` | `>= 0.50` | `< 0.50` |
| Bit bias | low overall + no significant bit | any significant bit | high overall bias |
| Distribution (KS p) | `>= 0.01` | `>= 0.001` | `< 0.001` |
| Stationarity | stationary and low F | not stationary | high F |
| Runs | near expected | moderate drift | severe drift |

## Chaos Verdict Thresholds

| Metric | PASS | WARN | FAIL |
|--------|------|------|------|
| Hurst | `0.4..0.6` | `0.3..0.7` | outside warn band |
| Lyapunov | small absolute value | moderate absolute value | large absolute value |
| Correlation dimension | high | moderate | low |
| BiEntropy | very high | high | low |
| Compression | near incompressible | mildly compressible | compressible |

## Reading Results

Treat verdicts as triage, not absolute proof. A single fail can reflect sample
size, transient conditions, or one sensitive metric. Confirm with larger samples
and `deep` profile runs before making hard decisions.

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Forensic Analysis](/openentropy/concepts/analysis-forensic/)
- [Chaos Theory Analysis](/openentropy/concepts/analysis-chaos/)
