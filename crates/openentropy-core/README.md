# openentropy-core

Core Rust library for OpenEntropy.

`openentropy-core` provides entropy collection and conditioning primitives:

- Source discovery (`detect_available_sources`)
- Multi-source pool (`EntropyPool`)
- Output conditioning (`raw`, `von_neumann`, `sha256`)
- Health reporting and source metadata

## Install

```toml
[dependencies]
openentropy-core = "0.5.1"
```

## Example

```rust
use openentropy_core::EntropyPool;

let pool = EntropyPool::auto();
let bytes = pool.get_random_bytes(64);
assert_eq!(bytes.len(), 64);
```

## Repository

https://github.com/amenti-labs/openentropy
