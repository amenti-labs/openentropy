# Integration Guide

Step-by-step guides for using OpenEntropy with other tools.

## ollama-auxrng: Hardware Entropy for Ollama

Ollama supports an auxiliary RNG device via the `OLLAMA_AUXRNG_DEV` environment variable. OpenEntropy can feed hardware entropy through a named pipe.

### Steps

1. **Start the entropy device** (creates a named pipe):

   ```bash
   openentropy device /tmp/openentropy-rng
   ```

   This blocks and continuously supplies entropy to any reader.

2. **In another terminal, start Ollama with hardware entropy**:

   ```bash
   OLLAMA_AUXRNG_DEV=/tmp/openentropy-rng ollama run llama3
   ```

   Ollama will read hardware entropy from the pipe for sampling randomness.

3. **Verify it's working**:

   The `openentropy device` command prints statistics when a reader connects. You should see bytes being consumed as Ollama generates tokens.

### Options

```bash
# Use only fast sources (default)
openentropy device /tmp/openentropy-rng

# Custom buffer size
openentropy device /tmp/openentropy-rng --buffer-size 8192

# Specific sources only
openentropy device /tmp/openentropy-rng --sources timing,silicon
```

---

## quantum-llama.cpp: ANU QRNG-Compatible Server

[quantum-llama.cpp](https://github.com/nicholasgasior/quantum-llama.cpp) expects a QRNG endpoint compatible with the ANU Quantum Random Numbers API. OpenEntropy's HTTP server speaks this protocol.

### Steps

1. **Start the entropy server**:

   ```bash
   openentropy server --port 8080
   ```

2. **Point quantum-llama.cpp at it**:

   ```bash
   ./llama-cli -m model.gguf --qrng-url http://localhost:8080/api/v1/random
   ```

3. **Test the endpoint manually**:

   ```bash
   # Get 256 random uint8 values
   curl "http://localhost:8080/api/v1/random?length=256&type=uint8"

   # Get hex-encoded output
   curl "http://localhost:8080/api/v1/random?length=64&type=hex16"

   # Raw (unconditioned) output — requires --allow-raw flag
   openentropy server --port 8080 --allow-raw
   curl "http://localhost:8080/api/v1/random?length=256&type=uint8&raw=true"
   ```

### Server Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/random?length=N&type=T` | Random data. Types: `hex16`, `uint8`, `uint16` |
| `GET /health` | Pool health status |
| `GET /sources[?experimental=true&telemetry=true&sample_bytes=1024]` | List sources with per-source stats and optional diagnostics (`quantum_proxy_v3`, `telemetry_v1`) |
| `GET /pool/status[?experimental=true&telemetry=true&sample_bytes=1024]` | Detailed pool metrics and optional diagnostics (`quantum_proxy_v3`, `telemetry_v1`) |

`experimental` and `telemetry` are independent opt-ins.

---

## Generic: Pipe Entropy to Any Program

Use `openentropy stream` to pipe raw bytes into any program expecting random data on stdin.

### Raw bytes to stdin

```bash
# Pipe 1KB of conditioned entropy
openentropy stream --format raw --bytes 1024 | your-program

# Continuous stream (Ctrl+C to stop)
openentropy stream --format raw | your-program

# Rate-limited stream (1KB/sec)
openentropy stream --format raw --rate 1024 | your-program
```

### Replace /dev/urandom reads

```bash
# Write entropy to a file, then use it
openentropy stream --format raw --bytes 4096 > /tmp/entropy.bin

# Feed to dd
openentropy stream --format raw --bytes 512 | dd of=/dev/disk...
```

### Hex or Base64 output

```bash
# Hex for scripts
openentropy stream --format hex --bytes 64

# Base64 for APIs
openentropy stream --format base64 --bytes 64
```

### Unconditioned (raw hardware noise)

```bash
# For research — bypasses SHA-256 conditioning
openentropy stream --conditioning raw --format raw --bytes 4096 > noise.bin

# Analyze with your own tools
openentropy stream --conditioning raw --format hex --bytes 256
```

⚠️ Raw output will show biases and patterns from the physical sources. This is expected — the hardware noise is real, not whitened.
