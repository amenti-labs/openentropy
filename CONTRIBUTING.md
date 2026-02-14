# Contributing to openentropy

## Prerequisites

- **Rust 1.85+** (edition 2024)
- **macOS** (primary platform; most entropy sources require Darwin APIs)
- **Python 3.10+** (only needed for PyO3 bindings)
- **maturin** (only needed for Python bindings: `pip install maturin`)

## Workspace Layout

```
Cargo.toml                    # Workspace root
crates/
├── openentropy-core/            # EntropySource trait, 39 sources, pool, conditioning
│   └── src/
│       ├── source.rs         # EntropySource trait definition
│       ├── sources/          # All 39 source implementations
│       │   └── mod.rs        # Source registry (all_sources())
│       ├── pool.rs           # Multi-source entropy pool
│       ├── conditioning.rs   # SHA-256 conditioning
│       ├── platform.rs       # Platform detection
│       └── lib.rs
├── openentropy-cli/             # CLI binary (clap) with 9 commands
├── openentropy-server/          # HTTP server (axum) with ANU QRNG API
├── openentropy-tests/           # NIST SP 800-22 statistical test suite
└── openentropy-python/          # PyO3 bindings via maturin
```

## Building

Build all crates except the Python bindings:

```bash
cargo build --workspace --exclude openentropy-python
```

Release build:

```bash
cargo build --release --workspace --exclude openentropy-python
```

## Testing

```bash
cargo test --workspace --exclude openentropy-python
```

Run a specific test:

```bash
cargo test -p openentropy-core clock_jitter
```

## Linting

```bash
cargo clippy --workspace --exclude openentropy-python -- -D warnings
```

## Formatting

```bash
cargo fmt --all
```

Check formatting without modifying files:

```bash
cargo fmt --all -- --check
```

## Python Bindings

The `openentropy-python` crate uses PyO3 and maturin. It is excluded from normal workspace builds because it requires a Python environment.

```bash
cd crates/openentropy-python
maturin develop
```

To build a release wheel:

```bash
cd crates/openentropy-python
maturin build --release
```

## Adding a New Entropy Source

1. **Create a source module** in `crates/openentropy-core/src/sources/`. If your source fits an existing category file (e.g., `timing.rs`, `silicon.rs`), add it there. Otherwise create a new file.

2. **Implement the `EntropySource` trait**:

```rust
use crate::source::{EntropySource, SourceCategory, SourceInfo};

static INFO: SourceInfo = SourceInfo {
    name: "your_source",
    description: "Brief description of the source",
    physics: "Explanation of the physical phenomenon providing entropy",
    category: SourceCategory::Novel, // pick the right category
    platform_requirements: &["darwin"],
    entropy_rate_estimate: 500.0, // bits/second estimate
};

pub struct YourSource;

impl EntropySource for YourSource {
    fn info(&self) -> &SourceInfo {
        &INFO
    }

    fn is_available(&self) -> bool {
        // Return false if required hardware/OS features are missing
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Collect raw entropy bytes from your source
        let mut samples = Vec::with_capacity(n_samples);
        // ... collection logic ...
        samples
    }
}
```

3. **Register the source** in `crates/openentropy-core/src/sources/mod.rs`:
   - Add `pub mod your_module;` to the module declarations
   - Add `Box::new(your_module::YourSource)` to the `all_sources()` vector

4. **Add tests** in the appropriate test module or in `crates/openentropy-tests/`.

5. **Verify**:
   ```bash
   cargo clippy --workspace --exclude openentropy-python -- -D warnings
   cargo test --workspace --exclude openentropy-python
   ```

## Guidelines

- Every source must handle unavailable hardware gracefully (`is_available()` returns `false`)
- The `EntropySource` trait requires `Send + Sync` -- no interior mutability without proper synchronization
- Use `&'static str` for metadata fields, not `String`
- Document the physics behind the entropy source in the `physics` field of `SourceInfo`
- No hardcoded paths -- use platform detection from `crate::platform`
- Use full paths for system binaries: `/usr/sbin/ioreg`, `/usr/sbin/sysctl`
- Zero clippy warnings: run `cargo clippy -- -D warnings` before submitting

## Commit Style

- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `ci:`, `chore:`
- Keep the subject line under 72 characters
- Reference issues when applicable: `fix: handle missing sysctl key (#42)`

## Pull Request Guidelines

- One logical change per PR
- All CI checks must pass (fmt, clippy, test, build)
- Include a brief description of what changed and why
- If adding a new entropy source, include probe output showing it works on your machine
- If changing the public API, update the Python bindings crate if applicable
- Squash-merge is preferred for clean history
