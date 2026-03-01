# openentropy-core

Core Rust library for OpenEntropy.

`openentropy-core` provides entropy collection, conditioning, and analysis:

- Source discovery (`detect_available_sources`)
- Multi-source pool (`EntropyPool`)
- Output conditioning (`raw`, `von_neumann`, `sha256`)
- Health reporting and source metadata
- Statistical analysis (`full_analysis`, `spectral_analysis`, `bit_bias`, ...)
- Differential comparison (`compare`, `two_sample_tests`, `cliffs_delta`, ...)
- PEAR-style trial analysis (`trial_analysis`, `stouffer_combine`, `calibration_check`)

## Install

```toml
[dependencies]
openentropy-core = "0.9"
```

## Example

```rust
use openentropy_core::{EntropyPool, full_analysis, compare};

// Collect entropy
let pool = EntropyPool::auto();
let bytes = pool.get_random_bytes(1000);

// Analyze
let analysis = full_analysis("my_source", &bytes);
println!("Shannon H: {:.4}", analysis.shannon_entropy);

// Compare two streams
let other = pool.get_random_bytes(1000);
let diff = compare("stream_a", &bytes, "stream_b", &other);
println!("KS p-value: {:.4}", diff.two_sample.ks_p_value);
```

## Repository

https://github.com/amenti-labs/openentropy
