---
title: 'Research Methodology'
description: 'Workflow for studying raw hardware noise and source behavior'
---

Use this workflow for experimental characterization of raw source behavior.

## 1) Capture Raw Signal

```bash
openentropy analyze --profile deep --output deep-analysis.json
openentropy stream --conditioning raw --format raw --bytes 4096 > sample.bin
```

`deep` enables forensic + entropy + chaos + trials + cross-correlation.

## 2) Record Sessions

```bash
openentropy record all --duration 5m --analyze --telemetry
openentropy sessions sessions/<id> --profile deep --output session-analysis.json
```

## 3) Compare Sessions

```bash
openentropy compare sessions/<id-a> sessions/<id-b> --profile deep --output comparison.json
```

## 4) Use Calibration When Needed

For trial-heavy workflows, gate recordings with calibration:

```bash
openentropy record qcicada --calibrate --duration 5m
```

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Trial Analysis](/openentropy/concepts/trials/)
- [Entropy Sources](/openentropy/concepts/sources/)
