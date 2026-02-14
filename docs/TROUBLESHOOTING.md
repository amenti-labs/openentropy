# Troubleshooting

## "No sources available"

**Symptom**: `openentropy scan` shows 0 sources.

**Causes & Fixes**:

- **Unsupported platform**: OpenEntropy primarily targets macOS on Apple Silicon. Linux support covers ~10-15 of 39 sources. Windows is not yet supported.
- **Permissions**: Some sources require elevated permissions or entitlements. Try running with `sudo` to rule out permission issues.
- **Binary mismatch**: Ensure you're running a binary built for your architecture (`uname -m` should match the binary target).

## BLE / WiFi source unavailable

**Symptom**: `bluetooth_noise` or `wifi_rssi` not detected.

**Fixes**:

- **macOS**: Grant Bluetooth access in **System Settings → Privacy & Security → Bluetooth**. The app or terminal needs explicit permission.
- **WiFi**: Ensure WiFi is enabled (the source reads RSSI, it doesn't need to be connected to a network).
- **Mac Mini / Desktop**: Ensure the machine has a Bluetooth/WiFi module. Some headless setups disable these.
- **Linux**: BLE requires `bluez` and may need `CAP_NET_ADMIN` capability.

## Audio source fails

**Symptom**: `audio_noise` source not detected or fails to collect.

**Fixes**:

- **No microphone**: Mac Mini and Mac Pro don't have built-in microphones. Connect an external audio input device.
- **macOS permissions**: Grant microphone access in **System Settings → Privacy & Security → Microphone**.
- **Audio server**: Ensure CoreAudio (macOS) or PulseAudio/PipeWire (Linux) is running.

## Slow sources timing out

**Symptom**: Collection hangs or takes a very long time.

**Explanation**: Some sources are inherently slow:
- `dns_timing` (~22s) — requires DNS lookups
- `tcp_connect_timing` (~39s) — requires TCP connections
- `gpu_timing` (~47s) — requires GPU compute dispatch
- `spotlight_timing` (~13s) — requires Spotlight indexing

**Fix**: By default, OpenEntropy uses only fast sources (<2s). If you explicitly enabled all sources:

```bash
# Use only fast sources (default, recommended)
openentropy bench

# If you need all sources, increase timeout
openentropy bench --sources all
```

In the Rust API:
```rust
pool.collect_all_parallel(60.0);  // 60s timeout
```

## Build failures

### Rust toolchain version

**Symptom**: `error[E0658]: edition 2024 is not yet stable`

**Fix**: OpenEntropy requires Rust 2024 edition (1.85+):

```bash
rustup update stable
rustc --version  # Should be >= 1.85.0
```

### macOS SDK issues

**Symptom**: `ld: framework not found IOKit` or similar linker errors.

**Fix**: Install Xcode command-line tools:

```bash
xcode-select --install
```

### Missing system libraries

**Symptom**: Various linker errors on Linux.

**Fix**: Install development headers:

```bash
# Debian/Ubuntu
sudo apt install build-essential pkg-config libasound2-dev libbluetooth-dev

# Fedora
sudo dnf install gcc pkg-config alsa-lib-devel bluez-libs-devel
```

## Python bindings

### maturin setup

**Symptom**: `ModuleNotFoundError: No module named 'openentropy'`

**Fix**: Build and install the Python bindings:

```bash
pip install maturin
cd /path/to/openentropy
maturin develop --release
```

### PYO3_USE_ABI3_FORWARD_COMPATIBILITY

**Symptom**: `error: the configured Python interpreter (version 3.X) is newer than the maximum supported version`

**Fix**: Set the forward compatibility flag:

```bash
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop --release
```

Or add to your shell profile:

```bash
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
```

### Virtual environments

**Symptom**: bindings installed but not found in your venv.

**Fix**: Ensure maturin builds into the active venv:

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop --release
python -c "import openentropy; print(openentropy.version())"
```

## Still stuck?

- Open an issue: [github.com/amenti-labs/openentropy/issues](https://github.com/amenti-labs/openentropy/issues)
- Check existing issues for your error message
- Include output of `openentropy scan` and `rustc --version` in your report
