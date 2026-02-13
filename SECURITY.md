# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.3.x   | ✅ Current release |
| < 0.3   | ❌ No longer supported |

## Reporting a Vulnerability

We take security seriously. If you discover a vulnerability in OpenEntropy, please report it responsibly.

### How to Report

1. **Email**: [security@amenti-labs.com](mailto:security@amenti-labs.com)
2. **GitHub Security Advisories**: [Report a vulnerability](https://github.com/amenti-labs/openentropy/security/advisories/new)

**Do not** open a public GitHub issue for security vulnerabilities.

We will acknowledge your report within 48 hours and aim to provide a fix or mitigation within 7 days for critical issues.

## What OpenEntropy IS

- A **hardware entropy harvester** that extracts randomness from physical phenomena in consumer hardware (clock jitter, DRAM timing, cache contention, sensor noise, etc.)
- A tool for **researchers** studying device entropy characteristics
- A **supplement** to existing entropy sources (OS CSPRNG, hardware RNG modules)
- An **entropy provider** for applications that benefit from hardware randomness (e.g., LLM sampling via ollama-auxrng)

## What OpenEntropy is NOT

- ❌ **Not a CSPRNG** — OpenEntropy is not a cryptographically secure pseudorandom number generator
- ❌ **Not a replacement for `/dev/urandom`** — your OS CSPRNG is well-audited and battle-tested; use it for cryptographic keys
- ❌ **Not formally certified** — OpenEntropy has not undergone FIPS 140-3, Common Criteria, or any formal certification process
- ❌ **Not guaranteed constant-rate** — entropy collection speed depends on hardware and source availability

## Threat Model

### What OpenEntropy protects against

- **PRNG state compromise**: If an attacker recovers the PRNG state of a software RNG, hardware entropy from OpenEntropy remains independent
- **Deterministic sampling in LLMs**: When used with ollama-auxrng or quantum-llama.cpp, provides non-deterministic randomness that cannot be predicted from model weights alone
- **Single-source failure**: The pool XOR-combines multiple independent sources — no single source failure compromises output

### What OpenEntropy does NOT protect against

- **Physical access attacks**: An attacker with physical access to the machine can observe the same hardware phenomena
- **Side-channel leakage**: The entropy sources themselves may be observable via electromagnetic emissions, power analysis, or shared hardware
- **Compromised OS/kernel**: If the OS is compromised, the attacker can intercept entropy at any point
- **Insufficient entropy at boot**: Early in the boot process, few sources may be available
- **Malicious source injection**: If `add_source()` is called with a malicious implementation, the pool integrity depends on other sources

### Conditioning modes

| Mode | Security | Use Case |
|------|----------|----------|
| **SHA-256** (default) | Cryptographic conditioning with OS entropy mixed in. Safe for general use. | Default for all applications |
| **VonNeumann** | Debiases first-order bias only. Not cryptographically strong. | Research, entropy analysis |
| **Raw** | ⚠️ No conditioning at all. XOR-combined source bytes only. | Research only |

### ⚠️ Raw Mode Warning

**Raw mode (`--unconditioned`, `?raw=true`, `ConditioningMode::Raw`) bypasses all conditioning.** The output reflects actual hardware noise characteristics, including any biases, correlations, or patterns present in the physical sources.

- Raw output **will not** pass standard randomness tests (NIST SP 800-22)
- Raw output **should not** be used for cryptographic purposes
- Raw output **is intended** for researchers studying hardware entropy characteristics
- **Use at your own risk**

## Architectural Security Properties

1. **Defense in depth**: SHA-256 conditioning mixes hardware entropy with OS entropy (`getrandom`) and a monotonic counter — even if all hardware sources produce zeros, the output remains unpredictable
2. **Source independence**: Sources exploit different physical phenomena (timing, memory, network, silicon microarchitecture) — compromise of one category does not affect others
3. **No network dependency**: All entropy is harvested locally — no API calls, no external servers, no trust in third parties
4. **Thread safety**: The entropy pool uses `Mutex`-guarded state for safe concurrent access
