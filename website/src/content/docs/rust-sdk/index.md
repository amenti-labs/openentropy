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

### Initialize the entropy pool

```rust
use openentropy_core::EntropyPool;

// Auto-detect available sources on your platform
let pool = EntropyPool::auto();
```

### Get random bytes

```rust
// Get 256 bytes of cryptographically-conditioned random data (SHA-256 by default)
let bytes = pool.get_random_bytes(256);
print!("Random hex: ");
for b in &bytes {
    print!("{b:02x}");
}
println!();
```

### Analyze entropy quality

```rust
// Get health report for all sources
let health = pool.health_report();
println!("Healthy sources: {}/{}", health.healthy, health.total);
println!("Total entropy collected: {} bytes", health.raw_bytes);

// Per-source breakdown
for source in &health.sources {
    println!("{}: {} bytes, entropy={:.4} bits/byte", 
        source.name, source.bytes, source.entropy);
}
```

### Full analysis workflow

```rust
use openentropy_core::{compare, trial_analysis};
use openentropy_core::analysis::full_analysis;

let data = pool.get_raw_bytes(5000);

// Per-source statistical analysis
let analysis = full_analysis("my_source", &data);
println!("Shannon entropy: {:.4} bits/byte", analysis.shannon_entropy);

// Differential comparison of two streams
let other = pool.get_raw_bytes(5000);
let diff = compare("stream_a", &data, "stream_b", &other);

// PEAR-style trial analysis
let trials = trial_analysis(&data, &Default::default());
println!("Terminal Z: {:.4}, p = {:.4}", trials.terminal_z, trials.terminal_p_value);
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

## Source Discovery

Detect which entropy sources are available on the current platform:

```rust
use openentropy_core::detect_available_sources;

let sources = detect_available_sources();
println!("Available sources: {}", sources.len());

for source in sources {
    let info = source.info();
    println!("  {} — {} ({})", info.name, info.description, info.category);
}
```

## [Full API Reference](/openentropy/rust-sdk/api/)

For complete API documentation including all crates, types, and methods, see the [Rust API Reference](/openentropy/rust-sdk/api/).

## Next Steps

- [Rust Quick Reference](/openentropy/rust-sdk/quick-reference/) — most-used calls and common workflows
- [Rust Analysis Workflows](/openentropy/rust-sdk/analysis/) — dispatcher, forensic, chaos, trials, and comparison patterns
- [Analysis System](/openentropy/concepts/analysis/) — interpretation guides and verdict model
