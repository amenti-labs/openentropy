# Architecture

## Entropy Pool

The core of esoteric-entropy is the `EntropyPool` class that combines multiple independent entropy sources into a single high-quality random byte stream.

### Pipeline

1. **Source Discovery** — `EntropyPool.auto()` instantiates every source class and calls `is_available()` to filter by platform capabilities.
2. **Collection** — `collect_all()` iterates over registered sources, calling `collect()` on each. Failed sources are tracked but don't halt the pool.
3. **Buffering** — Raw bytes are appended to a thread-safe internal buffer.
4. **Conditioning** — `get_random_bytes(n)` extracts conditioned output via SHA-256 mixing:
   - Internal state (32 bytes, persistent across calls)
   - Pool buffer sample (up to 256 bytes)
   - Monotonic counter
   - Current timestamp
   - 8 bytes from `os.urandom()` (defense in depth)
5. **Health Monitoring** — Each source tracks bytes collected, Shannon entropy of last sample, collection time, and failure count.

### Thread Safety

The pool buffer is protected by a `threading.Lock`. Multiple threads can call `get_random_bytes()` concurrently.

### Graceful Degradation

If a source fails (exception, empty output, or low entropy), it's marked unhealthy but the pool continues operating with remaining sources. The SHA-256 conditioning ensures output quality even with degraded inputs.

## Source Interface

Every source implements three methods:
- `is_available()` — platform check (no side effects)
- `collect(n_samples)` — return uint8 ndarray of raw samples
- `entropy_quality()` — self-test returning grade and metrics
