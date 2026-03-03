---
title: 'Rust Analysis Workflows'
description: 'Task-oriented analysis flows with dispatcher and focused modules'
---

This page maps common Rust analysis tasks to the right API entry points.

## One-call Analysis (recommended)

```rust
use openentropy_core::dispatcher::{analyze, AnalysisProfile};
let report = analyze(&[("source", &data)], &AnalysisProfile::Deep.to_config());
```

Use `AnalysisProfile::Security` for security-focused checks.

## Forensic + Chaos + Trials

```rust
use openentropy_core::{full_analysis, trial_analysis};
use openentropy_core::chaos::chaos_analysis;

let forensic = full_analysis("source", &data);
let chaos = chaos_analysis(&data);
let trials = trial_analysis(&data, &Default::default());
```

## Source-to-source Comparison

```rust
use openentropy_core::compare;
let delta = compare("a", &data_a, "b", &data_b);
```

## Related

- [Analysis System](/openentropy/concepts/analysis/)
- [Rust API Reference](/openentropy/rust-sdk/api/)
