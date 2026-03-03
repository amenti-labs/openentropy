---
title: 'Chaos Theory Analysis'
description: 'Distinguishing deterministic structure from random behavior'
---

Chaos analysis helps distinguish genuinely random behavior from deterministic
systems that only look random.

Implemented in `openentropy_core::chaos`.

## Hurst Exponent

Measures long-range dependence (R/S analysis).

- `H ~= 0.5`: random-walk-like
- `H > 0.5`: persistent trend behavior
- `H < 0.5`: anti-persistent behavior

## Lyapunov Exponent

Measures sensitivity to initial conditions.

- `lambda ~= 0`: no clear deterministic chaos signature
- `lambda > 0`: chaotic divergence
- `lambda < 0`: convergent behavior

## Correlation Dimension

Measures attractor dimensionality.

- High `D2` suggests high-dimensional/random-like behavior
- Low `D2` can indicate deterministic low-dimensional structure

## BiEntropy

Measures entropy persistence through derivative levels of the bitstream.

- Higher values indicate stronger disorder and less structure

## Epiplexity

Compression-ratio complexity metric.

- Ratio near `1.0` indicates incompressible/random-like data
- Lower ratios imply compressible structure

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Verdict System](/openentropy/concepts/analysis-verdicts/)
