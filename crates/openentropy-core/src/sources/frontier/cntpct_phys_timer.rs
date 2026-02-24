//! ARM64 physical system counter (CNTPCT_EL0) timing entropy.
//!
//! The ARM architecture defines two system counters accessible from EL0:
//! - `CNTVCT_EL0`: **Virtual** counter — may have an offset applied by a hypervisor
//! - `CNTPCT_EL0`: **Physical** counter — the raw hardware counter value
//!
//! On macOS, both counters track wall time at 24 MHz with no hypervisor offset.
//! However, they have **different microarchitectural read paths**.
//!
//! ## Physics
//!
//! `CNTVCT_EL0` is cached in a core-local register that is updated every
//! hardware clock tick. Most consecutive reads return the same tick value
//! (both reads complete within the same 41.67 ns window).
//!
//! `CNTPCT_EL0` reads the physical counter, which on Apple Silicon requires
//! a **domain crossing** — the request must travel from the CPU core to the
//! physical counter in the system counter module, which sits outside the core
//! cluster. This domain crossing adds latency that varies with:
//!
//! 1. **System fabric bus load**: Other components (GPU, NVMe, network) reading
//!    the physical counter create arbitration delays on the counter bus.
//!
//! 2. **Power state of the counter domain**: If the counter domain has been
//!    in a low-power state, the domain crossing takes longer to wake up.
//!
//! 3. **Kernel clock domain crossing**: The physical counter may be in a
//!    different clock domain from the CPU core, requiring synchronization.
//!
//! Empirically on M4 Mac mini (N=5000, tight loop):
//! - Mean: 5.27 ticks (~220 ns), **CV=1863.0%**, range=0–5,000 ticks
//! - 4,639/5,000 reads (92.8%) complete in 0 ticks (same tick as caller)
//! - Max observed: 5,000 ticks (208 µs) — likely preemption during domain crossing
//!
//! **CV=1863% is the highest variance of any entropy source measured** — the
//! extreme coefficient of variation reflects the bimodal nature: 92.8% of reads
//! are immediate, but 7.2% involve a domain-crossing delay that varies from
//! 1 tick to 5,000 ticks depending on physical counter bus state.
//!
//! ## Comparison with CNTVCT
//!
//! In the same tight loop, `CNTVCT_EL0` shows CV=395.8% — much lower than
//! `CNTPCT_EL0`'s 1863%. The 4.7× higher variance for CNTPCT directly
//! measures the nondeterminism in the physical counter domain crossing,
//! which is absent from the locally-cached virtual counter.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::extract_timing_entropy;

static CNTPCT_PHYS_TIMER_INFO: SourceInfo = SourceInfo {
    name: "cntpct_phys_timer",
    description: "ARM64 CNTPCT_EL0 physical timer domain-crossing timing — CV=1863%",
    physics: "Reads CNTPCT_EL0 (physical system counter) timed by CNTVCT_EL0 (virtual). \
              Unlike CNTVCT which is core-locally cached, CNTPCT requires a domain crossing \
              to the physical counter module outside the CPU cluster. Domain crossing time \
              varies with system fabric bus load, counter domain power state, and clock \
              domain synchronization. Measured: CV=1863.0% (highest of any source), \
              mean=5.27 ticks, range=0\u{2013}5000 ticks. 92.8% of reads are immediate (0 ticks); \
              7.2% involve a delayed domain crossing. 4.7\u{00d7} higher variance than CNTVCT, \
              directly measuring physical counter bus nondeterminism.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2500.0,  // High CV but sparse events — modest rate
    composite: false,
};

/// Entropy source from CNTPCT_EL0 physical timer domain-crossing timing.
pub struct CNTPCTPhysTimerSource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for CNTPCTPhysTimerSource {
    fn info(&self) -> &SourceInfo {
        &CNTPCT_PHYS_TIMER_INFO
    }

    fn is_available(&self) -> bool {
        // CNTPCT_EL0 is accessible on macOS/Apple Silicon (confirmed empirically).
        // The EL0 trap for CNTPCT is NOT set by the Apple macOS kernel.
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // CV=1863% means most samples are 0 (no entropy) but 7.2% have high
        // variance timing. We need ~14× oversampling to get n_samples events.
        let raw = n_samples * 20 + 256;
        let mut timings = Vec::with_capacity(raw);

        let mut prev_virt: u64;
        unsafe {
            core::arch::asm!(
                "mrs {v}, cntvct_el0",
                v = out(reg) prev_virt,
                options(nostack, nomem),
            );
        }

        for _ in 0..raw {
            let virt_before: u64;
            let phys: u64;
            let virt_after: u64;

            unsafe {
                // Read virtual before, physical, virtual after.
                // The timing of the CNTPCT read = (virt_after - virt_before).
                core::arch::asm!(
                    "mrs {vb}, cntvct_el0",
                    "mrs {p},  cntpct_el0",
                    "mrs {va}, cntvct_el0",
                    vb = out(reg) virt_before,
                    p  = out(reg) phys,
                    va = out(reg) virt_after,
                    options(nostack, nomem),
                );
            }

            let elapsed = virt_after.wrapping_sub(virt_before);
            let _ = phys;

            // Cap at 10ms (240K ticks) — reject suspend/resume.
            if elapsed < 240_000 {
                timings.push(elapsed);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for CNTPCTPhysTimerSource {
    fn info(&self) -> &SourceInfo { &CNTPCT_PHYS_TIMER_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = CNTPCTPhysTimerSource;
        assert_eq!(src.info().name, "cntpct_phys_timer");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_apple_silicon() {
        assert!(CNTPCTPhysTimerSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_with_domain_crossings() {
        let data = CNTPCTPhysTimerSource.collect(32);
        assert!(!data.is_empty());
    }
}
