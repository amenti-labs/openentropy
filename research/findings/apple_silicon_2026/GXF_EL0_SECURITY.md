# Security Finding: GXF Namespace Register EL0-Readable on Apple M4

**Date:** 2026-02-24  
**Severity:** Informational / Low  
**Platform:** Apple M4 (T8132), macOS 15.3  
**CVE:** Not assigned

## Finding

Register `S3_6_c15_c1_5` (op0=3, op1=6, CRn=c15, CRm=c1, op2=5) is **readable from EL0** via a JIT-generated MRS instruction on Apple M4. This register resides in the GXF (Guarded eXecution Framework) namespace, which is Apple's proprietary hypervisor/privilege layer.

```
Value:  0x2010002030100000  (constant across all reads, all boots tested)
Timing: 0/41/42/83/84 ticks — trap-and-emulate path confirmed (41-tick latency)
```

## Technical Context

The GXF namespace uses `op1=6` in the ARM64 implementation-defined register space (CRn=c15). These registers are normally accessible only at EL1 or above. Apple has set `SCTLR_EL1.UCI=1` for certain CRn=c15 registers (confirmed by DC CIVAC EL0 accessibility), and appears to have accidentally exposed this GXF capability register.

**Value interpretation:** `0x2010002030100000` is a bitmask. Decoded:
- `bit 60`: 1 — some capability flag
- `bit 53`: 1 — likely "GXF capable"  
- `bits 37:32`: `0x30` — permission field
- `bits 23:16`: `0x10` — state field
- `bits 7:0`: `0x00` — reserved

The constant value across reboots confirms it is a **static capability register**, not a mutable state register. No exploitable mutable state is exposed.

## Impact

- **Confidentiality:** Low. The value is constant and does not leak secrets
- **Integrity:** None. The register is read-only from EL0
- **Availability:** None
- **Entropy:** Low (CV=35.2% — from trap-path timing variation, not register value)

The trap-and-emulate timing (41-tick quantum) is more valuable as an entropy signal than the register value itself.

## Compared to DC CIVAC

Apple also exposes `DC CIVAC` (cache clean+invalidate to PoC) from EL0 by setting `SCTLR_EL1.UCI=1`. That is intentional and documented. The GXF register exposure is likely **unintentional** — GXF is a proprietary namespace that should not be user-accessible.

## Disclosure Status

Informational finding documented here. No exploit path identified. If Apple's PSIRT determines this constitutes a security boundary violation, the constant value means no actionable attack exists.
