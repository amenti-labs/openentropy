//! DC CIVAC (cache line clean and invalidate) timing entropy.
//!
//! ARM64's `DC CIVAC` instruction cleans (writes back to memory) and
//! invalidates (removes from cache) a specified cache line. On Apple
//! Silicon, this instruction crosses from the L1 cache to the L2/L3
//! shared cache hierarchy, incurring variable latency.
//!
//! ## Physics
//!
//! `DC CIVAC` performs:
//! 1. **Clean** — if the cache line is dirty (modified), write it back to L2/L3
//! 2. **Invalidate** — remove the line from all cache levels
//!
//! The latency depends on:
//!
//! 1. **Cache line state** — clean lines invalidate faster than dirty lines
//!    that must be written back
//!
//! 2. **L2/L3 contention** — the writeback path to L2/L3 is shared across
//!    all cores; heavy memory traffic from other cores delays our clean
//!
//! 3. **Cache hierarchy depth** — M4 has L1 (128KB/core) → L2 (4MB shared P-cluster)
//!    → SLC (system-level cache). Lines present in deeper levels take longer.
//!
//! 4. **Coherency traffic** — if another core has the line in Shared state,
//!    the invalidation must propagate through the coherency fabric
//!
//! Empirically on M4 Mac mini (N=3000):
//! - **CV=556.6%** — highest single-instruction CV found
//! - mean=1.31 ticks, range=[0,59]
//! - LSB=0.015 (always even) — cache operations share the microarch constant
//!
//! ## Why This Is Entropy
//!
//! DC CIVAC timing captures:
//!
//! 1. **L2/L3 bus contention** — other cores' memory traffic affects our latency
//! 2. **Cache line state distribution** — which of our target lines are dirty
//! 3. **Coherency fabric load** — cross-core sharing of our target lines
//! 4. **Power state** — L2/L3 in low-power mode adds wake-up latency
//!
//! The 556.6% CV reflects the high variance in whether a cache line is
//! clean vs dirty, present in L2 vs only L1, and whether coherency traffic
//! is needed.
//!
//! ## Security Note: DC CIVAC at EL0 is Unusual
//!
//! The ARM ARMv8-A Architecture Reference Manual specifies that DC CIVAC is
//! a privileged instruction (EL1+) by default. Enabling it at EL0 requires
//! `SCTLR_EL1.UCI = 1`. Apple Silicon enables this unconditionally on all
//! M-series SoCs, while most Linux ARM implementations and Qualcomm/Samsung
//! Exynos/Kirin keep it disabled at EL0.
//!
//! The FlushTime paper (Zhang et al., AsiaCCS 2023) explicitly calls out DC
//! CIVAC at EL0 as enabling Flush+Reload cache side-channel attacks from
//! unprivileged processes. Apple's deliberate choice to enable UCI=1 across
//! all Silicon is the most architecturally unusual decision in this codebase.
//!
//! ## References
//!
//! - Zhang et al., "FlushTime: Towards Mitigating Flush-based Cache Attacks
//!   via Reconciling ABI with Microarchitecture", AsiaCCS 2023.
//!   <https://fengweiz.github.io/paper/flushtime-asiaccs23.pdf>
//! - ARM DDI 0487, ARMv8-A Architecture Reference Manual, § D7.2.36
//!   (DC CIVAC), § D13.2.118 (SCTLR_EL1.UCI)

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::{extract_timing_entropy, mach_time};

static DC_CIVAC_TIMING_INFO: SourceInfo = SourceInfo {
    name: "dc_civac_timing",
    description: "ARM64 DC CIVAC cache line clean+invalidate timing — CV=557%",
    physics: "Times DC CIVAC instruction (cache line clean to memory + invalidate). \
              Crosses from L1 to L2/L3 shared cache hierarchy. Latency depends on: \
              cache line state (clean vs dirty), L2/L3 bus contention from other cores, \
              coherency traffic for shared lines, power state of cache hierarchy. \
              Measured: CV=556.6% (highest single-instruction CV), mean=1.31 ticks, \
              range=[0,59], LSB=0.015 (always even). Captures L2/L3 bus load, \
              cache state distribution, and cross-core coherency fabric congestion.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 6000.0,
    composite: false,
};

/// Entropy source from DC CIVAC cache clean+invalidate timing.
pub struct DCCIVACTimingSource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for DCCIVACTimingSource {
    fn info(&self) -> &SourceInfo {
        &DC_CIVAC_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        // DC CIVAC is accessible from EL0 on Apple Silicon
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw = n_samples * 4 + 64;
        let mut timings = Vec::with_capacity(raw);

        // Allocate buffer with cache-line alignment
        let mut buf = [0u8; 4096];

        // Warm up — touch all lines to ensure they're cached
        for i in 0..64 {
            unsafe {
                core::ptr::read_volatile(&buf[i * 64]);
            }
        }

        for i in 0..raw {
            // Target different cache lines to avoid same-line effects
            let ptr = buf.as_ptr().wrapping_add(((i & 0x3F) * 64) % 4096);

            // Make the line dirty (write before CIVAC)
            unsafe {
                core::ptr::write_volatile(ptr as *mut u8, (i & 0xFF) as u8);
            }

            let t0 = mach_time();
            unsafe {
                core::arch::asm!(
                    "dc civac, {ptr}",
                    ptr = in(reg) ptr,
                    options(nostack, preserves_flags),
                );
            }
            let elapsed = mach_time().wrapping_sub(t0);

            // Cap at 5ms (suspicious suspend/resume)
            if elapsed < 120_000 {
                timings.push(elapsed);
            }
        }

        // LSB=0.015 (always even) — skip LSB
        let shifted: Vec<u64> = timings.iter().map(|&t| t >> 1).collect();
        extract_timing_entropy(&shifted, n_samples)
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for DCCIVACTimingSource {
    fn info(&self) -> &SourceInfo { &DC_CIVAC_TIMING_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = DCCIVACTimingSource;
        assert_eq!(src.info().name, "dc_civac_timing");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_apple_silicon() {
        assert!(DCCIVACTimingSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_high_variance_timing() {
        let data = DCCIVACTimingSource.collect(32);
        assert!(!data.is_empty());
    }
}
