---
title: 'SDK Integration'
description: 'Embedding openentropy in Python and Rust applications safely'
---

This guide shows practical integration patterns for application code.

## Pattern 1: Seed a CSPRNG Source

Collect conditioned bytes and pass them to your application's cryptographic
primitive.

```python
from openentropy import EntropyPool
seed = EntropyPool.auto().get_random_bytes(32)
```

```rust
use openentropy_core::EntropyPool;
let seed = EntropyPool::auto().get_random_bytes(32);
```

## Pattern 2: Validate Before Operational Use

Run dispatcher analysis in CI or startup checks:

```python
from openentropy import analyze
report = analyze([("startup", data)], profile="security")
```

```rust
use openentropy_core::dispatcher::{analyze, AnalysisProfile};
let report = analyze(&[("startup", &data)], &AnalysisProfile::Security.to_config());
```

## Pattern 3: Monitor Source Health

Use periodic health reports and alert when healthy source count drops.

## Related

- [Python SDK](/openentropy/python-sdk/)
- [Rust SDK](/openentropy/rust-sdk/)
- [Security Validation](/openentropy/guides/security-validation/)
