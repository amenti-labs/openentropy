# Ollama Integration Guide

Feed hardware-harvested entropy into LLM inference for non-deterministic sampling.

## Method 1: ollama-auxrng (Named Pipe)

[ollama-auxrng](https://github.com/amenti-labs/ollama-auxrng) patches Ollama to read from an external RNG device.

### Setup

```bash
# Install esoteric-entropy
pip install esoteric-entropy

# Start the entropy device (background)
esoteric-entropy device /tmp/esoteric-rng &

# Run Ollama with the external RNG
OLLAMA_AUXRNG_DEV=/tmp/esoteric-rng ollama run llama3
```

### How it works

1. `esoteric-entropy device` creates a FIFO (named pipe) at the given path
2. It continuously feeds conditioned entropy bytes into the pipe
3. ollama-auxrng reads from the pipe instead of Go's `math/rand`
4. LLM token sampling uses hardware entropy for temperature-based selection

### Options

```bash
# Custom buffer size (default 4096)
esoteric-entropy device /tmp/esoteric-rng --buffer-size 8192

# Filter to specific sources
esoteric-entropy device /tmp/esoteric-rng --sources timing,silicon
```

## Method 2: quantum-llama.cpp (HTTP API)

[quantum-llama.cpp](https://github.com/amenti-labs/quantum-llama.cpp) is a llama.cpp fork with QRNG sampling support.

### Setup

```bash
# Start the entropy server
esoteric-entropy server --port 8042

# Run quantum-llama.cpp
./llama-cli -m model.gguf \
    --qrng-url http://localhost:8042/api/v1/random \
    --qrng-type uint16
```

### API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/random?length=N&type=hex16` | Random hex strings |
| `GET /api/v1/random?length=N&type=uint8` | Random bytes (0-255) |
| `GET /api/v1/random?length=N&type=uint16` | Random uint16 (0-65535) |
| `GET /health` | Server health check |
| `GET /sources` | List active entropy sources |
| `GET /pool/status` | Pool health report |

### Response Format

```json
{
    "type": "uint8",
    "length": 256,
    "data": [142, 87, 203, ...],
    "success": true
}
```

## Verifying Entropy Quality

```bash
# Run the test battery
esoteric-entropy report

# Check pool health
esoteric-entropy pool

# Benchmark sources
esoteric-entropy bench
```
