---
title: 'Rust SDK'
description: 'openentropy-core crate for Rust developers'
---

**openentropy-core** is the Rust library for harvesting real entropy from hardware noise. It provides 63 independent entropy sources across thermal, timing, microarchitecture, I/O, GPU, and network domains.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
openentropy-core = "0.10"
```

## Quick Start

### Discover available sources

```rust
use openentropy_core::detect_available_sources;

let sources = detect_available_sources();
println!("Available sources: {}", sources.len());
```

### Sample a single source (recommended)

```rust
use openentropy_core::{ConditioningMode, EntropyPool};

let pool = EntropyPool::auto();
let source = pool.source_names()[0].clone();

let raw = pool.get_source_raw_bytes(&source, 4096).unwrap();
let conditioned = pool
    .get_source_bytes(&source, 256, ConditioningMode::Sha256)
    .unwrap();

println!("Using source: {source}");
println!("Conditioned bytes: {}", conditioned.len());
println!("Raw bytes: {}", raw.len());
```

### Analyze source quality

```rust
use openentropy_core::analysis::full_analysis;

let analysis = full_analysis(&source, &raw);
println!("Shannon entropy: {:.4} bits/byte", analysis.shannon_entropy);
```

### Use pooled output (advanced)

```rust
// Combine available sources for pooled output
let bytes = pool.get_random_bytes(256);
let health = pool.health_report();

println!("Random bytes: {}", bytes.len());
println!("Healthy sources: {}/{}", health.healthy, health.total);
```

## Conditioning Modes

By default, output is SHA-256 conditioned for cryptographic use. You can also request raw or Von Neumann debiased output:

```rust
use openentropy_core::ConditioningMode;

// SHA-256 (default) — cryptographic quality
let bytes = pool.get_bytes(256, ConditioningMode::Sha256);

// Von Neumann debiasing only — preserves more signal structure
let bytes = pool.get_bytes(256, ConditioningMode::VonNeumann);

// Raw unconditioned bytes — for research and analysis
let bytes = pool.get_bytes(256, ConditioningMode::Raw);
```

## [Full API Reference](/openentropy/rust-sdk/api/)

For complete API documentation including all crates, types, and methods, see the [Rust API Reference](/openentropy/rust-sdk/api/).

## Next Steps

- [Rust Quick Reference](/openentropy/rust-sdk/quick-reference/) — most-used calls and common workflows
- [Rust Analysis Workflows](/openentropy/rust-sdk/analysis/) — dispatcher, forensic, chaos, trials, and comparison patterns
- [Analysis System](/openentropy/concepts/analysis/) — interpretation guides and verdict model
