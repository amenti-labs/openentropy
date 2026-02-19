# OpenEntropy Examples

## Rust Examples

| Example | Description |
|---------|-------------|
| [`rust/basic.rs`](rust/basic.rs) | Simple entropy collection — auto-detect sources, get random bytes, print as hex |
| [`rust/raw_vs_conditioned.rs`](rust/raw_vs_conditioned.rs) | Compare Raw, VonNeumann, and Sha256 conditioning modes side by side |
| [`rust/stream_to_file.rs`](rust/stream_to_file.rs) | Collect entropy and write raw bytes to a file |

### Running Rust Examples

```bash
cargo run -p openentropy-core --example basic
cargo run -p openentropy-core --example raw_vs_conditioned
cargo run -p openentropy-core --example stream_to_file
```

## Python Examples

| Example | Description |
|---------|-------------|
| [`python/basic.py`](python/basic.py) | Create an entropy pool, collect bytes, print health stats |
| [`python/raw_entropy.py`](python/raw_entropy.py) | Get raw unconditioned bytes and compare with conditioned output |
| [`python/named_pipe.py`](python/named_pipe.py) | Feed hardware entropy to any program via a named pipe (FIFO) |

### Running Python Examples

First, build the Python bindings:

```bash
pip install maturin
maturin develop --release
```

Then run any example:

```bash
python examples/python/basic.py
python examples/python/raw_entropy.py
python examples/python/named_pipe.py
```
