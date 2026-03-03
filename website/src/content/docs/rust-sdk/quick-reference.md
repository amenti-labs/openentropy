---
title: 'Rust Quick Reference'
description: 'Most-used Rust SDK calls for pool, analysis, and sessions'
---

Use this page for common workflows. For all types and signatures, see
[Rust API Reference](/openentropy/rust-sdk/api/).

## Pool Basics

```rust
use openentropy_core::EntropyPool;

let pool = EntropyPool::auto();
pool.collect_all_parallel(5.0);
let data = pool.get_random_bytes(256);
let raw = pool.get_raw_bytes(256);
let health = pool.health_report();
```

## Source Discovery

```rust
use openentropy_core::{detect_available_sources, platform_info};
let sources = detect_available_sources();
let platform = platform_info();
```

## Dispatcher Analysis

```rust
use openentropy_core::dispatcher::{analyze, AnalysisProfile};
let report = analyze(&[("src", &data)], &AnalysisProfile::Deep.to_config());
```

## Focused Analysis Calls

```rust
use openentropy_core::{full_analysis, chaos::chaos_analysis, trial_analysis};

let forensic = full_analysis("src", &data);
let chaos = chaos_analysis(&data);
let trials = trial_analysis(&data, &Default::default());
```

## Session Workflows

```rust
use openentropy_core::{list_sessions, load_session_raw_data};
use std::path::Path;

let sessions = list_sessions(Path::new("sessions"))?;
let raw_map = load_session_raw_data(&sessions[0].0)?;
```

## Related

- [Rust API Reference](/openentropy/rust-sdk/api/)
- [Analysis System](/openentropy/concepts/analysis/)
