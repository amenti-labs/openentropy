# OpenEntropy — Conditioning Architecture

How raw hardware entropy becomes cryptographically uniform random bytes.

## Overview

```
┌──────────────────────────────────────────────────────────────┐
│                     Entropy Sources (30)                      │
│  clock_jitter, dns_timing, page_fault_timing, ...            │
│  Each returns raw bytes — NO internal conditioning           │
└──────────────────┬───────────────────────────────────────────┘
                   │ raw bytes
                   ▼
┌──────────────────────────────────────────────────────────────┐
│                    EntropyPool (pool.rs)                      │
│  XOR-combines raw bytes from all available sources           │
│  get_raw_bytes()  → returns XOR-combined raw output          │
│  get_bytes()      → passes through conditioning layer        │
└──────────────────┬───────────────────────────────────────────┘
                   │
          ┌────────┴────────┐
          │                 │
          ▼                 ▼
   ┌─────────────┐  ┌──────────────┐
   │  Raw Mode   │  │ Conditioned  │
   │  (bypass)   │  │ (default)    │
   │             │  │              │
   │ XOR-combined│  │ Von Neumann  │
   │ bytes as-is │  │ → SHA-256    │
   └─────────────┘  └──────────────┘
```

## The Two Modes

### Conditioned Output (Default)

The default pipeline applies two stages:

1. **Von Neumann debiasing** — removes statistical bias by examining bit pairs. Outputs 1 for `(1,0)`, 0 for `(0,1)`, discards `(0,0)` and `(1,1)`. Reduces throughput by ~4x but eliminates bias without assumptions about the source distribution.

2. **SHA-256 hashing** — maps the debiased stream to uniformly distributed bytes. Produces exactly the requested number of bytes via iterative hashing.

After conditioning, output is 8.0 bits/byte Shannon entropy regardless of source quality.

### Raw Output (Opt-in)

Raw mode returns XOR-combined source bytes with **no conditioning at all** — no Von Neumann debiasing, no SHA-256 hashing. This is the actual hardware signal.

**Why offer raw mode:**
- **Transparency** — users can verify what the hardware actually produces
- **Research** — entropy researchers need unconditioned samples for statistical analysis
- **NIST compliance** — SP 800-90B requires testing the noise source *before* conditioning
- **Differentiation** — most hardware RNG APIs (ANU, Outshift) only expose post-DRBG output; you can never see the raw raw hardware signal

**Raw mode access:**

| Interface | How to access |
|-----------|---------------|
| Rust API | `pool.get_raw_bytes(n)` |
| CLI | `openentropy stream --unconditioned` |
| CLI | `openentropy device <name> --unconditioned` |
| HTTP API | `GET /entropy?bytes=N&raw=true` (requires `--allow-raw` flag) |
| Python SDK | `pool.get_raw_bytes(n)` |

The HTTP server requires the `--allow-raw` startup flag to enable raw mode — this prevents accidental exposure of unconditioned entropy.

## Conditioning Modes (`conditioning.rs`)

```rust
pub enum ConditioningMode {
    Raw,         // No processing — pass through as-is
    VonNeumann,  // Von Neumann debiasing only
    Sha256,      // Von Neumann + SHA-256 (default)
}

pub fn condition(data: &[u8], output_len: usize, mode: ConditioningMode) -> Vec<u8>
```

All conditioning is centralized in `crates/openentropy-core/src/conditioning.rs`. Individual entropy sources **never** perform their own conditioning — they return raw hardware samples only.

### Why Centralized Conditioning?

Previous versions had SHA-256 calls scattered across individual source files. This was problematic:

1. **Destroyed measurability** — couldn't assess raw source quality
2. **Double-conditioning** — pool applied SHA-256 again on already-hashed output
3. **Masked failures** — a broken source producing zeros would still look random after SHA-256
4. **Violated separation of concerns** — sources should sample hardware, not process data

The refactored design enforces a clean boundary: sources produce raw samples, the conditioning layer (if enabled) makes them uniform.

## Comparison to QRNG/DRBG APIs

| Feature | OpenEntropy | ANU QRNG | Outshift QRNG | Linux `/dev/urandom` |
|---------|-----------------|----------|---------------|---------------------|
| Raw output available | ✅ Yes | ❌ No | ❌ No | ❌ No |
| Source diversity | 39 sources | 1 (vacuum fluctuation) | 1 (superconducting) | ~5 (interrupts, etc.) |
| Conditioning visible | ✅ Optional, documented | ❌ Opaque | ❌ DRBG post-processing | ❌ ChaCha20 CSPRNG |
| Self-hosted | ✅ Local binary | ❌ Cloud API | ❌ Cloud API | ✅ Kernel |
| Statistical tests | ✅ Built-in NIST SP 800-22 | ❌ | ❌ | ❌ |
| Source health monitoring | ✅ Per-source grades | ❌ | ❌ | ❌ |

### The DRBG Problem

Most "hardware random number" APIs don't actually give you hardware randomness. They give you the output of a DRBG (Deterministic Random Bit Generator) that was *seeded* with hardware entropy. The DRBG's output is computationally indistinguishable from the hardware input — but it's not the raw signal itself.

OpenEntropy's raw mode is the equivalent of tapping the wire *before* the DRBG. You get the actual hardware signal with all its imperfections, correlations, and physical characteristics intact.

### When to Use Each Mode

| Use Case | Mode |
|----------|------|
| Cryptographic key generation | Conditioned (default) |
| Application randomness | Conditioned (default) |
| Entropy source research | Raw |
| NIST SP 800-90B compliance testing | Raw |
| Hardware characterization | Raw |
| Consciousness/QRNG experiments | Raw |
| Comparing to DRBG-based APIs | Raw |

## Security Considerations

- **Raw output is NOT suitable for cryptographic use.** Raw bytes have lower Shannon entropy (often 2-6 bits/byte vs 8.0) and may contain statistical patterns.
- **Conditioned output uses SHA-256**, which is a cryptographic hash. The output is computationally uniform. However, openentropy is not a CSPRNG — it does not maintain state or provide forward secrecy.
- **The HTTP server's `--allow-raw` flag** exists specifically to prevent accidental deployment of raw endpoints. Production deployments should not enable it unless raw access is explicitly needed.
