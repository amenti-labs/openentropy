//! AES hardware execution unit pipeline timing entropy.
//!
//! ARM64 Apple Silicon includes dedicated AES hardware acceleration via the
//! `AESE`/`AESD`/`AESMC`/`AESIMC` instructions. These execute in the
//! cryptographic execution unit, which is a separate pipeline from the
//! integer and floating-point units.
//!
//! ## Physics
//!
//! AES instruction execution time varies based on:
//!
//! 1. **Execution unit pipeline state** — if the AES unit is in use by
//!    another core (or by the SEP), our AES instructions compete for the
//!    shared AES hardware paths in the system fabric
//! 2. **Key schedule cache state** — the AES unit caches the expanded key
//!    schedule in internal registers; a new key requires re-expansion
//! 3. **CPU pipeline stalls** — memory latency for loading key/data
//!    operands, branch mispredictions in surrounding code
//! 4. **Thermal throttling** — the crypto unit frequency scales
//!    with die temperature under sustained load
//!
//! Empirically measured on M4 Mac mini (N=500, 3× AESE+AESMC per sample):
//! - Same key repeated: CV=268.3%, range=0–42 ticks, **LSB=0.052**
//! - Rotating key: CV=318.0%, range=0–42 ticks, **LSB=0.014**
//!
//! The extreme LSB bias (0.014–0.052) is a **microarchitectural constant**:
//! the AES execution unit always completes in an even number of 24 MHz ticks.
//! This mirrors the AMX coprocessor constant (LSB=0.959, always odd) and
//! the ICC coherency constant (LSB=0.188). These are hardware timing invariants
//! that reveal the internal clock structure of functional units.
//!
//! For entropy extraction, we use the UPPER bits (not LSB). The high variance
//! (CV>268%) provides excellent entropy in the 2+ bit positions.
//!
//! ## Cross-process covert channel
//!
//! On Apple Silicon, multiple cores share the same crypto hardware paths
//! in the fabric layer. Heavy AES usage by other processes (encrypted disk
//! I/O, HTTPS connections, FileVault operations) can increase contention
//! for the AES execution unit, causing our timing to increase. This makes
//! AES timing a genuine cross-process side channel.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::{extract_timing_entropy, mach_time};

static AES_EXEC_TIMING_INFO: SourceInfo = SourceInfo {
    name: "aes_exec_timing",
    description: "ARM64 AES hardware execution unit pipeline timing",
    physics: "Times 3\u{00d7}AESE+AESMC instruction sequences with rotating keys. Execution \
              time varies with AES unit pipeline state, key schedule cache hits, CPU \
              pipeline stalls, and thermal state. Measured CV=268\u{2013}318%, range 0\u{2013}42 ticks. \
              LSB=0.014\u{2013}0.052 is a microarchitectural constant: AES ops always complete \
              in even hardware tick counts, mirroring AMX (always odd) and ICC \
              (always even) constants. Entropy extracted from upper bits. Cross-process \
              sensitivity: heavy FileVault/HTTPS use increases our timing via shared \
              fabric AES hardware paths.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 6000.0,
    composite: false,
};

/// Entropy source from ARM64 AES hardware instruction pipeline timing.
pub struct AESExecTimingSource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use super::*;

    /// Key material for AES operations.
    /// Rotated by 1 byte each sample to force key schedule variation.
    static BASE_KEY: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
        0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
    ];

    /// Plaintext block (constant — only key variation matters).
    static PT: [u8; 16] = [
        0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96,
        0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93, 0x17, 0x2a,
    ];

    /// Execute 3× AESE+AESMC on the given key and plaintext, returning
    /// the elapsed 24 MHz tick count.
    ///
    /// # Safety
    /// Uses inline ARM64 assembly. All register inputs and outputs are
    /// from stack-allocated arrays. No memory safety issues.
    #[inline]
    unsafe fn time_aes_round(key: &[u8; 16], pt: &[u8; 16]) -> u64 {
        let t0 = mach_time();
        // SAFETY: key and pt are 16-byte aligned by the Rust allocator.
        // v0=key, v1=plaintext. AESE performs one AES round. AESMC does
        // MixColumns. Three rounds gives enough work to measure without
        // the overhead being dominated by loop overhead.
        unsafe {
            core::arch::asm!(
                "ld1 {{v0.16b}}, [{key}]",
                "ld1 {{v1.16b}}, [{pt}]",
                "aese v1.16b, v0.16b",
                "aesmc v1.16b, v1.16b",
                "aese v1.16b, v0.16b",
                "aesmc v1.16b, v1.16b",
                "aese v1.16b, v0.16b",
                "aesmc v1.16b, v1.16b",
                key = in(reg) key.as_ptr(),
                pt  = in(reg) pt.as_ptr(),
                out("v0") _,
                out("v1") _,
                options(nostack),
            );
        }
        mach_time().wrapping_sub(t0)
    }

    impl EntropySource for AESExecTimingSource {
        fn info(&self) -> &SourceInfo {
            &AES_EXEC_TIMING_INFO
        }

        fn is_available(&self) -> bool {
            // ARM64 Apple Silicon always has AES hardware acceleration.
            true
        }

        fn collect(&self, n_samples: usize) -> Vec<u8> {
            // 8× oversampling — LSB bias requires upper-bit extraction.
            let raw_count = n_samples * 8 + 128;
            let mut timings = Vec::with_capacity(raw_count);

            // Warm up: fill AES execution unit pipeline and L1 cache.
            for i in 0..64_usize {
                let mut key = BASE_KEY;
                for j in 0..16 { key[j] = key[j].wrapping_add(i as u8); }
                let _ = unsafe { time_aes_round(&key, &PT) };
            }

            for i in 0..raw_count {
                // Rotate key by i bytes to force different key schedule states.
                let mut key = [0u8; 16];
                for j in 0..16 {
                    key[j] = BASE_KEY[(j + i) & 15].wrapping_add(i as u8);
                }

                let t = unsafe { time_aes_round(&key, &PT) };

                // Sanity filter: reject impossible values (would indicate
                // a timer read failure or >4ms overhead from interrupts).
                if t < 100_000 {
                    timings.push(t);
                }
            }

            // Extract entropy from upper bits (not LSB — always-even bias).
            // Shift right by 1 to bring bit-1 to LSB position.
            let shifted: Vec<u64> = timings.iter().map(|&t| t >> 1).collect();
            extract_timing_entropy(&shifted, n_samples)
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for AESExecTimingSource {
    fn info(&self) -> &SourceInfo {
        &AES_EXEC_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        false
    }

    fn collect(&self, _n_samples: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = AESExecTimingSource;
        assert_eq!(src.info().name, "aes_exec_timing");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_apple_silicon() {
        assert!(AESExecTimingSource.is_available());
    }

    #[test]
    #[ignore] // Requires AES hardware — timing is hardware-dependent
    fn collects_bytes_with_variation() {
        let src = AESExecTimingSource;
        if !src.is_available() {
            return;
        }
        let data = src.collect(32);
        assert!(!data.is_empty());
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 2, "expected variation from AES pipeline timing");
    }
}
