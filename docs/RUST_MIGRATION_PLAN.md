# Rust Migration Plan — esoteric-entropy

> Rewriting the esoteric-entropy Python package as a Rust project with Python bindings.
> Author: Amenti Labs | Date: 2026-02-12

## 1. Project Structure

```
esoteric-entropy/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── esoteric-core/      # Core library (entropy sources, pool, conditioning)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pool.rs           # EntropyPool (thread-safe, SHA-256 conditioning)
│   │       ├── source.rs         # EntropySource trait + SourceState
│   │       ├── conditioning.rs   # SHA-256, Von Neumann, XOR-fold
│   │       ├── health.rs         # Health monitoring, quality metrics
│   │       ├── platform.rs       # Platform detection, source discovery
│   │       └── sources/
│   │           ├── mod.rs
│   │           ├── timing.rs       # ClockJitter, MachTiming, SleepJitter
│   │           ├── sysctl.rs       # SysctlSource
│   │           ├── vmstat.rs       # VmstatSource
│   │           ├── network.rs      # DNSTiming, TCPConnect
│   │           ├── wifi.rs         # WiFiRSSI (CoreWLAN)
│   │           ├── disk.rs         # DiskIO
│   │           ├── memory.rs       # MemoryTiming
│   │           ├── gpu.rs          # GPUTiming (sips)
│   │           ├── process.rs      # ProcessSource
│   │           ├── audio.rs        # AudioNoise (CoreAudio)
│   │           ├── camera.rs       # CameraNoise (AVFoundation)
│   │           ├── sensor.rs       # SensorNoise
│   │           ├── bluetooth.rs    # BluetoothNoise (CoreBluetooth)
│   │           ├── silicon.rs      # DRAMRowBuffer, CacheContention, PageFault, SpecExec
│   │           ├── ioregistry.rs   # IORegistry deep mining
│   │           ├── cross_domain.rs # CPUIOBeat, CPUMemoryBeat, MultiDomainBeat
│   │           ├── compression.rs  # CompressionTiming, HashTiming
│   │           └── novel.rs        # DispatchQueue, DyldTiming, VMPageTiming, SpotlightTiming
│   │
│   ├── esoteric-cli/       # CLI binary (clap)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/
│   │       │   ├── mod.rs
│   │       │   ├── scan.rs
│   │       │   ├── probe.rs
│   │       │   ├── bench.rs
│   │       │   ├── stream.rs
│   │       │   ├── device.rs
│   │       │   ├── server.rs
│   │       │   ├── monitor.rs
│   │       │   ├── report.rs
│   │       │   └── pool.rs
│   │       └── tui/            # ratatui TUI
│   │           ├── mod.rs
│   │           ├── app.rs
│   │           ├── ui.rs
│   │           └── widgets.rs
│   │
│   ├── esoteric-server/    # HTTP server (axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── esoteric-tests/     # NIST test suite
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── frequency.rs
│   │       ├── runs.rs
│   │       ├── serial.rs
│   │       ├── spectral.rs
│   │       ├── entropy.rs
│   │       ├── correlation.rs
│   │       ├── distribution.rs
│   │       ├── pattern.rs
│   │       ├── advanced.rs
│   │       └── practical.rs
│   │
│   └── esoteric-python/    # PyO3 bindings (maturin)
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
│
├── esoteric_entropy/       # Existing Python code (KEPT INTACT)
├── pyproject.toml          # Updated for maturin build
└── tests/
```

## 2. Entropy Source Mapping (All 30 Sources)

### Timing Sources (3)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 1 | `ClockJitterSource` | `std::time::Instant` vs `SystemTime` delta LSBs | Pure Rust |
| 2 | `MachTimingSource` | `mach_absolute_time()` via `libc` FFI | `libc::mach_absolute_time()` |
| 3 | `SleepJitterSource` | `thread::sleep(Duration::ZERO)` + `Instant` | Pure Rust |

### System Sources (3)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 4 | `SysctlSource` | `Command::new("/usr/sbin/sysctl")` batch parsing | subprocess |
| 5 | `VmstatSource` | `Command::new("vm_stat")` counter parsing | subprocess |
| 6 | `ProcessSource` | `Command::new("ps")` + SHA-256 hash | subprocess |

### Network Sources (3)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 7 | `DNSTimingSource` | Raw UDP DNS packet via `std::net::UdpSocket` | Pure Rust |
| 8 | `TCPConnectSource` | `TcpStream::connect()` timing | Pure Rust |
| 9 | `WiFiRSSISource` | `Command::new("/usr/sbin/networksetup")` + airport fallback | subprocess / CoreWLAN FFI |

### Hardware Sources (5)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 10 | `DiskIOSource` | `tempfile` + random read timing via `std::fs` | Pure Rust |
| 11 | `MemoryTimingSource` | `mmap` allocation + access timing | `libc::mmap/munmap` |
| 12 | `GPUTimingSource` | `Command::new("/usr/bin/sips")` timing | subprocess |
| 13 | `AudioNoiseSource` | `Command::new("ffmpeg")` or CoreAudio FFI | subprocess/FFI |
| 14 | `CameraNoiseSource` | `Command::new("ffmpeg")` capture | subprocess |

### More Hardware (3)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 15 | `SensorNoiseSource` | `Command::new("/usr/sbin/ioreg")` sensor check | subprocess |
| 16 | `BluetoothNoiseSource` | `Command::new("/usr/sbin/system_profiler")` | subprocess |
| 17 | `IORegistryEntropySource` | `Command::new("/usr/sbin/ioreg")` deep mining | subprocess |

### Silicon Microarchitecture (4)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 18 | `DRAMRowBufferSource` | 32MB alloc + random access timing via `mach_absolute_time` | `libc` FFI + unsafe |
| 19 | `CacheContentionSource` | 8MB alloc + sequential/random alternating | `libc` FFI + unsafe |
| 20 | `PageFaultTimingSource` | `mmap/munmap` page fault timing | `libc` FFI |
| 21 | `SpeculativeExecutionSource` | Data-dependent branches + timing | inline, `Instant` |

### Cross-Domain Beat (3)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 22 | `CPUIOBeatSource` | CPU work ↔ file I/O timing via `mach_absolute_time` | `libc` FFI |
| 23 | `CPUMemoryBeatSource` | CPU work ↔ random memory timing | `libc` FFI |
| 24 | `MultiDomainBeatSource` | CPU/mem/IO/syscall interleave timing | `libc` FFI |

### Compression/Hash Timing (2)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 25 | `CompressionTimingSource` | `flate2::compress` timing oracle | `flate2` crate |
| 26 | `HashTimingSource` | `sha2::Sha256` timing oracle | `sha2` crate |

### Novel Sources (4)

| # | Python Class | Rust Implementation | Key APIs |
|---|---|---|---|
| 27 | `DispatchQueueSource` | Thread pool + crossbeam channel scheduling timing | `std::thread` |
| 28 | `DyldTimingSource` | `libloading::Library::new()` timing | `libloading` crate |
| 29 | `VMPageTimingSource` | `mmap/munmap` cycle timing | `libc` FFI |
| 30 | `SpotlightTimingSource` | `Command::new("/usr/bin/mdls")` timing | subprocess |

## 3. Key Crate Dependencies

```toml
[workspace.dependencies]
# Core
sha2 = "0.10"          # SHA-256 conditioning
flate2 = "1"           # zlib compression timing + ratio test
libc = "0.2"           # mach_absolute_time, mmap, sysctl FFI
rand = "0.8"           # Random indices for memory access patterns
tempfile = "3"         # Temp files for disk I/O
libloading = "0.8"     # dlopen/dlsym timing (dyld source)

# CLI
clap = { version = "4", features = ["derive"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# HTTP Server
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Python bindings
pyo3 = { version = "0.23", features = ["extension-module"] }

# Testing
statrs = "0.18"        # Statistical distributions for NIST tests
rustfft = "6"          # FFT for spectral tests
```

## 4. Python SDK (PyO3/maturin)

The Python package will be pip-installable and maintain the same API:

```python
from esoteric_entropy import EntropyPool, EntropySource

# Same as current API
pool = EntropyPool.auto()
data = pool.get_random_bytes(32)

# Health
report = pool.health_report()
pool.print_health()

# NumPy compatibility
from esoteric_entropy import EsotericBitGenerator, EsotericRandom
rng = EsotericRandom()
values = rng.random(size=100)
```

PyO3 bindings expose:
- `EntropyPool` (with `.auto()`, `.get_random_bytes()`, `.collect_all()`, `.health_report()`)
- `EntropySource` (read-only source info)
- `run_all_tests()` (NIST battery)
- `calculate_quality_score()`

## 5. CLI Commands (clap)

```
esoteric-entropy scan          # List available sources
esoteric-entropy probe <name>  # Test single source quality
esoteric-entropy bench         # Benchmark all sources
esoteric-entropy stream        # Stream entropy to stdout
  --format raw|hex|base64
  --rate <bytes/sec>
  --bytes <total>
  --sources <filter>
esoteric-entropy device <path> # Named pipe FIFO
  --buffer-size <bytes>
  --sources <filter>
esoteric-entropy server        # HTTP API server
  --port <port>
  --host <addr>
  --sources <filter>
esoteric-entropy monitor       # Interactive TUI
  --refresh <secs>
  --sources <filter>
esoteric-entropy report        # NIST test battery
  --samples <n>
  --source <name>
  --output <path>
esoteric-entropy pool          # Pool health metrics
```

## 6. TUI Monitor (ratatui)

Interactive dashboard with:
- Source health table (name, status, bytes, entropy, time, failures)
- Real-time entropy rate chart (sparkline/line graph)
- Pool status bar (buffer size, output rate, health grade)
- Source info panel (physics description, category, platform)
- Live RNG output display (hex, int, float)
- Keyboard controls: q=quit, space=toggle source, i=info, ↑↓=navigate

## 7. HTTP Server (axum)

ANU QRNG API compatible endpoints:
- `GET /api/v1/random?length=N&type=hex16|uint8|uint16`
- `GET /health`
- `GET /sources`
- `GET /pool/status`

CORS headers, JSON responses, same format as Python version.

## 8. NIST Test Suite

31 tests ported to Rust:
- **Frequency**: monobit, block, byte
- **Runs**: runs, longest run of ones
- **Serial**: serial, approximate entropy
- **Spectral**: DFT, flatness
- **Entropy**: Shannon, min-entropy, permutation, compression ratio, Kolmogorov
- **Correlation**: auto, serial, lag-N, cross
- **Distribution**: KS, Anderson-Darling
- **Pattern**: overlapping template, non-overlapping, Maurer's universal
- **Advanced**: binary matrix rank (GF(2)), linear complexity (Berlekamp-Massey), CUSUM, random excursions, birthday spacing
- **Practical**: bit avalanche, Monte Carlo Pi, mean & variance

Using `statrs` for chi2/normal/Poisson CDFs and `rustfft` for FFT.

## 9. Implementation Order

1. Workspace + core crate scaffolding
2. `source.rs` trait + `conditioning.rs` + `pool.rs`
3. All 30 sources (grouped by category)
4. Platform detection + source discovery
5. CLI with scan/probe/bench/stream/pool commands
6. HTTP server
7. Device (FIFO) command
8. TUI monitor
9. NIST test suite
10. PyO3 Python bindings
11. Integration testing

## 10. Testing Strategy

- **Unit tests**: Each source `.is_available()` and `.collect()` on this Mac Mini M4
- **Integration tests**: Pool + conditioning pipeline
- **Statistical tests**: NIST battery on conditioned output
- **CLI tests**: All commands produce expected output
- **Python binding tests**: API compatibility with existing Python code
- **Benchmark tests**: Compare throughput with Python version
