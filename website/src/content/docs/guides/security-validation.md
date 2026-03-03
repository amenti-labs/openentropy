---
title: 'Security Validation'
description: 'Practical workflow for validating entropy quality for cryptographic use'
---

Use this workflow when validating entropy quality for key generation or CSPRNG
seeding.

## 1) Run Security Profile

```bash
openentropy analyze --profile security --output audit.md
```

`security` enables forensic + entropy breakdown + NIST-style report behavior.

## 2) Confirm Entropy Quality Signals

- Min-entropy is in an acceptable range for your threat model
- Forensic metrics show no persistent structural failures
- NIST-style report pass/fail trends are stable across runs

## 3) Compare Multiple Runs

Run at least a few independent captures to avoid one-off conclusions:

```bash
openentropy record all --duration 1m --analyze
openentropy sessions sessions/<id> --profile security
```

## 4) Enforce Conditioned Output In Production

Use SHA-256 conditioned output for operational use:

```bash
openentropy stream --conditioning sha256 --format raw --bytes 1024
```

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Entropy Breakdown](/openentropy/concepts/analysis-entropy/)
- [Conditioning](/openentropy/concepts/conditioning/)
- [Troubleshooting](/openentropy/guides/troubleshooting/)
