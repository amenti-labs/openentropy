---
title: 'Cross-Correlation'
description: 'Inter-source independence checks using pairwise correlation'
---

Cross-correlation evaluates dependence between multiple entropy sources.

Implemented in `openentropy_core::analysis` via
`cross_correlation_matrix()`.

## What It Computes

- Pairwise Pearson correlation for all source pairs
- Flag count for pairs whose absolute correlation exceeds threshold

| Metric | Description |
|--------|-------------|
| `pairs` | Correlation coefficient for each source pair |
| `flagged_count` | Number of pairs with `|r| > 0.3` |

## Interpretation

Independent sources should show low pairwise correlation. Elevated
cross-correlation can indicate shared hardware pathways, scheduling
coupling, or environmental coupling that reduces effective independence.

## Usage Notes

- Requires two or more sources
- Enabled by `--profile deep` or `--cross-correlation`

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Forensic Analysis](/openentropy/concepts/analysis-forensic/)
