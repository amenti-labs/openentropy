# Ollama Integration Guide

Feed hardware-harvested entropy into LLM inference for non-deterministic token sampling. This guide covers two integration methods and explains why hardware entropy matters for language model generation.

## Why Hardware Entropy for LLM Sampling

Standard LLM inference uses pseudorandom number generators (PRNGs) for temperature-based token sampling. PRNGs are deterministic -- given the same seed, they produce identical token sequences. This means:

- **Reproducible outputs** even at high temperature (same seed = same response)
- **Predictable patterns** for anyone who discovers or guesses the PRNG seed
- **Limited diversity** in multi-turn conversations when using fixed seeds

Hardware entropy from openentropy feeds genuine physical randomness into the sampling process. The randomness comes from 30 independent physical sources (clock jitter, DRAM timing, cache contention, etc.), making token selection truly non-deterministic at the physics level.

---

## Method 1: Device Mode (Named Pipe)

The `device` command creates a FIFO (named pipe) that acts as a character device. Any program that reads from the pipe receives conditioned hardware entropy.

### Setup with ollama-auxrng

[ollama-auxrng](https://github.com/amenti-labs/ollama-auxrng) is a patched version of Ollama that reads from an external RNG device instead of Go's built-in `math/rand`.

```bash
# Terminal 1: Start the entropy device (runs in foreground)
openentropy device /tmp/esoteric-rng

# Terminal 2: Run Ollama with the external RNG
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

Or run the device in the background:

```bash
openentropy device /tmp/esoteric-rng &
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

### How It Works

1. `openentropy device` creates a FIFO at the given path (e.g., `/tmp/esoteric-rng`)
2. The CLI continuously collects entropy from all available sources and feeds conditioned bytes into the pipe
3. When ollama-auxrng needs random numbers for token sampling, it reads from the pipe instead of `math/rand`
4. The LLM's temperature-based softmax sampling uses hardware entropy for token selection

### Device Options

```bash
# Custom buffer size (default: 4096 bytes)
openentropy device /tmp/esoteric-rng --buffer-size 8192

# Filter to specific source categories for faster collection
openentropy device /tmp/esoteric-rng --sources timing,silicon

# Use only cross-platform sources (for Linux)
openentropy device /tmp/esoteric-rng --sources timing,network,disk
```

### Verifying the Device

```bash
# Check that the FIFO exists
ls -la /tmp/esoteric-rng
# prw-r--r-- 1 user group 0 ... /tmp/esoteric-rng

# Read some bytes (will block until the device process is running)
head -c 32 /tmp/esoteric-rng | xxd

# Verify entropy quality of the device output
head -c 10000 /tmp/esoteric-rng > /tmp/test.bin
openentropy report --source pool
```

---

## Method 2: Server Mode (HTTP API)

The `server` command starts an HTTP entropy server compatible with the ANU Quantum Random Number Generator API format. This allows any HTTP-capable client to consume hardware entropy.

### Setup with quantum-llama.cpp

[quantum-llama.cpp](https://github.com/amenti-labs/quantum-llama.cpp) is a llama.cpp fork that supports external QRNG backends for token sampling.

```bash
# Terminal 1: Start the entropy server
openentropy server --port 8042

# Terminal 2: Run quantum-llama.cpp with the QRNG backend
./llama-cli -m model.gguf \
    --qrng-url http://localhost:8042/api/v1/random \
    --qrng-type uint16
```

### Server Options

```bash
# Custom port and bind address
openentropy server --port 8080 --host 0.0.0.0

# Filter sources
openentropy server --port 8042 --sources silicon,timing
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/random?length=N&type=T` | GET | Random data in specified format |
| `/health` | GET | Server health check |
| `/sources` | GET | List active entropy sources with status |
| `/pool/status` | GET | Detailed pool health report |

### Query Parameters for `/api/v1/random`

| Parameter | Values | Default | Description |
|-----------|--------|---------|-------------|
| `length` | 1-65536 | 1024 | Number of values to return |
| `type` | `hex16`, `uint8`, `uint16` | `hex16` | Output format |

### Response Format

**Random data (`/api/v1/random?length=256&type=uint8`):**

```json
{
    "type": "uint8",
    "length": 256,
    "data": [142, 87, 203, 51, 174, 9, 230, 118, "..."],
    "success": true
}
```

**uint16 format (`/api/v1/random?length=128&type=uint16`):**

```json
{
    "type": "uint16",
    "length": 128,
    "data": [36494, 13035, 30411, 59154, "..."],
    "success": true
}
```

**hex16 format (`/api/v1/random?length=128&type=hex16`):**

```json
{
    "type": "hex16",
    "length": 128,
    "data": ["8e57", "cb33", "ae09", "e676", "..."],
    "success": true
}
```

**Health check (`/health`):**

```json
{
    "status": "healthy",
    "sources_healthy": 22,
    "sources_total": 24,
    "raw_bytes": 1048576,
    "output_bytes": 524288
}
```

**Sources list (`/sources`):**

```json
{
    "sources": [
        {
            "name": "clock_jitter",
            "healthy": true,
            "bytes": 50000,
            "entropy": 7.832,
            "time": 0.045,
            "failures": 0
        },
        "..."
    ],
    "total": 24
}
```

**Pool status (`/pool/status`):**

```json
{
    "healthy": 22,
    "total": 24,
    "raw_bytes": 1048576,
    "output_bytes": 524288,
    "buffer_size": 8192,
    "sources": ["..."]
}
```

### curl Examples

```bash
# Get 256 random bytes as uint8 array
curl -s "http://localhost:8042/api/v1/random?length=256&type=uint8" | jq '.data[:10]'

# Get 64 random uint16 values
curl -s "http://localhost:8042/api/v1/random?length=64&type=uint16" | jq '.data'

# Check server health
curl -s "http://localhost:8042/health" | jq .

# List active sources
curl -s "http://localhost:8042/sources" | jq '.sources[] | {name, healthy, entropy}'

# Get raw hex for piping to other tools
curl -s "http://localhost:8042/api/v1/random?length=1024&type=hex16" | jq -r '.data[]'
```

---

## Method 3: Direct Python API

If you are building a custom inference pipeline in Python, you can use the entropy pool directly:

```python
from openentropy import EntropyPool
import struct

pool = EntropyPool.auto()

# Get random bytes for sampling
random_bytes = pool.get_random_bytes(8)

# Convert to float in [0, 1) for temperature sampling
random_u64 = struct.unpack('<Q', random_bytes)[0]
random_float = random_u64 / (2**64)

# Use in your sampling logic
def sample_with_hardware_entropy(logits, temperature):
    """Sample from logits using hardware entropy."""
    import numpy as np

    scaled = logits / temperature
    probs = np.exp(scaled - np.max(scaled))
    probs /= probs.sum()

    # Use hardware entropy for the random selection
    r = struct.unpack('<d', pool.get_random_bytes(8))[0] % 1.0
    cumulative = np.cumsum(probs)
    return np.searchsorted(cumulative, r)
```

---

## Verifying Entropy Quality

Before using hardware entropy for inference, verify the system is working correctly:

```bash
# Discover what sources are available on this machine
openentropy scan

# Benchmark all sources with quality metrics
openentropy bench

# Run the full NIST-inspired test battery
openentropy report

# Check pool health interactively
openentropy pool

# Launch the live monitoring dashboard
openentropy monitor
```

Expected output from a healthy system:

- **Sources:** 20-30 available (depending on platform)
- **Shannon entropy:** > 7.9 bits/byte for conditioned output
- **NIST tests:** 28-31/31 passing
- **Quality grade:** A

---

## Troubleshooting

### Device pipe hangs on read

The device process must be running before any reader opens the pipe. Start the device first, then start the consumer.

### Server returns low entropy

If `sources_healthy` is low in the health check, some sources may be unavailable on your platform. This is normal -- the pool conditioning ensures output quality regardless. Run `openentropy scan` to see which sources are active.

### Permission errors

Some sources require access to system utilities (`sysctl`, `ioreg`, `system_profiler`). Ensure the binary has appropriate permissions. No sources require root/sudo.

### Network sources slow

DNS and TCP timing sources may be slow if the network is unavailable. Use `--sources timing,silicon,system` to exclude network sources for offline operation.

### Linux compatibility

On Linux, macOS-specific sources (WiFi RSSI, Bluetooth, IORegistry, GPU, sensors, dispatch queue, VM page, Spotlight) are not available. The remaining 10-15 sources still provide sufficient entropy for quality output. Use `openentropy scan` to confirm available sources.
