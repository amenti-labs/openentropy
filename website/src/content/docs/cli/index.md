---
title: 'CLI'
description: 'openentropy command-line tool'
---

The openentropy CLI provides command-line access to entropy sources, analysis tools, and real-time monitoring.

## Installation

```bash
cargo install openentropy-cli
```

## Quick Examples

**Discover entropy sources on your machine:**
```bash
openentropy scan
```

**Benchmark all fast sources:**
```bash
openentropy bench
```

**Output 256 random hex bytes:**
```bash
openentropy stream --format hex --bytes 256
```

**Live TUI dashboard with real-time monitoring:**
```bash
openentropy monitor
```

**Security audit with NIST test battery:**
```bash
openentropy analyze --profile security
```

**Research-grade analysis with cross-correlation:**
```bash
openentropy analyze --profile deep
```

## Analysis Profiles

Profiles are convenience presets that configure multiple flags at once. Choose the profile that matches your use case:

| Profile | Audience | Samples | Conditioning | Entropy | NIST Report | Cross-Corr | Trials | Chaos |
|---------|----------|---------|-------------|---------|-------------|------------|--------|-------|
| `quick` | Any | 10,000 | raw | — | — | — | — | — |
| `standard` | Any (default) | 50,000 | raw | — | — | — | — | — |
| `deep` | Research | 100,000 | raw | ✓ | — | ✓ | ✓ | ✓ |
| `security` | Security | 50,000 | sha256 | ✓ | ✓ | — | — | — |

For detailed explanations of each analysis category (forensic, entropy, chaos,
trials, cross-correlation), interpretation guides, and the verdict system, see
[Analysis System](/openentropy/concepts/analysis/).

**Security engineers** use the `security` profile to validate entropy quality and seed CSPRNGs:
```bash
openentropy analyze --profile security --output audit.md
```

**Researchers** use the `deep` profile to study raw noise characteristics:
```bash
openentropy analyze --profile deep --output analysis.json
```

## Full CLI Reference

For complete command documentation, examples, and advanced options, see the [Full CLI Reference](/openentropy/cli/reference/).

## Task Guides

- [Security Validation](/openentropy/guides/security-validation/)
- [Research Methodology](/openentropy/guides/research-methodology/)
