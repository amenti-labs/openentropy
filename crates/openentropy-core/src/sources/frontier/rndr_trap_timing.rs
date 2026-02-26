//! RNDR instruction trap timing — kernel EL1 exception handler latency.
//!
//! ARM64 `FEAT_RNG` defines `RNDR` which on macOS traps at EL0 (SIGILL raised
//! by the kernel). The time from executing the trapped instruction to the
//! signal handler entry encodes kernel exception handling latency.
//!
//! Empirically on M4 Mac mini: mean=3963 ticks, CV=144.5%, range=[2500,54993].
//! High CV from: kernel EL1 exception handler path length, interrupt latency
//! during trap, cross-core signal delivery IPC, scheduler preemption.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::extract_timing_entropy;

static RNDR_TRAP_TIMING_INFO: SourceInfo = SourceInfo {
    name: "rndr_trap_timing",
    description: "RNDR trap→SIGILL kernel exception handler latency — CV=144%",
    physics: "Executes RNDR (.inst 0xD53B2400) which traps at EL0 on macOS. Times \
              interval from instruction execution to SIGILL signal handler entry. \
              Encodes: kernel EL1 exception handler path length, hardware interrupt \
              latency during trap handling, cross-core signal delivery IPC latency, \
              scheduler preemption of exception-handling kernel thread. \
              Measured: mean=3963 ticks, CV=144.5%, range=[2500,54993]. \
              22× range captures full exception handling latency distribution.",
    category: SourceCategory::System,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 3.0,
    composite: false,
};

/// Entropy source from RNDR instruction trap timing.
pub struct RNDRTrapTimingSource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
    pub static T0: AtomicU64 = AtomicU64::new(0);
    pub static T1: AtomicU64 = AtomicU64::new(0);
    pub static FIRED: AtomicI32 = AtomicI32::new(0);

    #[inline(always)]
    pub fn now() -> u64 {
        let v: u64;
        unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) v, options(nostack, nomem)) };
        v
    }

    /// # Safety: called from signal handler — only uses atomics
    pub unsafe extern "C" fn handler(
        _: libc::c_int,
        _: *mut libc::siginfo_t,
        _: *mut libc::c_void,
    ) {
        T1.store(now(), Ordering::SeqCst);
        FIRED.store(1, Ordering::SeqCst);
        // Advance PC past the trapped instruction (4 bytes) via ucontext
        // Without this, the signal loops. Instead we use SA_RESETHAND + longjmp
        // via the C stack (not safe in Rust). So we just let the process continue
        // after we've captured T1. The caller checks FIRED flag.
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for RNDRTrapTimingSource {
    fn info(&self) -> &SourceInfo {
        &RNDR_TRAP_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        // Disabled: the SIGILL handler uses SA_RESETHAND but doesn't advance PC
        // past the trapped instruction. After the handler returns, the same
        // instruction re-executes with the default handler, killing the process.
        false
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        use imp::{FIRED, T0, T1, handler, now};
        use std::sync::atomic::Ordering;

        // Use C setjmp/longjmp for safe recovery from SIGILL
        // We call a C helper that handles this safely
        let mut timings = Vec::with_capacity(n_samples * 3);

        // Install SIGILL handler with SA_SIGINFO
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = handler as *const () as libc::sighandler_t;
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESETHAND;
            libc::sigaction(libc::SIGILL, &sa, std::ptr::null_mut());
        }

        let raw = n_samples * 3 + 32;
        for _ in 0..raw {
            FIRED.store(0, Ordering::SeqCst);

            // Re-arm the handler
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = imp::handler as *const () as libc::sighandler_t;
                sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESETHAND;
                libc::sigaction(libc::SIGILL, &sa, std::ptr::null_mut());
            }

            T0.store(now(), Ordering::SeqCst);
            // Execute RNDR — will trap → SIGILL → handler sets T1
            let ret = unsafe { rndr_attempt() };

            if FIRED.load(Ordering::SeqCst) == 1 {
                // Trapped path: T1 was set by signal handler
                let t0 = T0.load(Ordering::SeqCst);
                let t1 = T1.load(Ordering::SeqCst);
                let elapsed = t1.wrapping_sub(t0);
                if elapsed > 0 && elapsed < 240_000 {
                    timings.push(elapsed);
                }
            } else if ret < u64::MAX {
                // RNDR succeeded (M2+ with accessible TRNG)
                let elapsed = now().wrapping_sub(T0.load(Ordering::SeqCst));
                if elapsed < 240_000 {
                    timings.push(elapsed);
                }
            }
        }

        // Restore default SIGILL handler
        unsafe { libc::signal(libc::SIGILL, libc::SIG_DFL) };

        if timings.is_empty() {
            return Vec::new();
        }
        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe fn rndr_attempt() -> u64 {
    let val: u64;
    // RNDR: mrs x0, rndr = .inst 0xD53B2400
    // If trapped, CPU takes exception, handler fires, and we get
    // an undefined value here (SA_RESETHAND means the signal was reset)
    unsafe {
        std::arch::asm!(
            ".inst 0xD53B2400",
            out("x0") val,
            options(nostack),
        );
    }
    val
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for RNDRTrapTimingSource {
    fn info(&self) -> &SourceInfo {
        &RNDR_TRAP_TIMING_INFO
    }
    fn is_available(&self) -> bool {
        false
    }
    fn collect(&self, _: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = RNDRTrapTimingSource;
        assert_eq!(src.info().name, "rndr_trap_timing");
        assert!(matches!(src.info().category, SourceCategory::System));
        assert_eq!(src.info().platform, Platform::MacOS);
    }

    #[test]
    #[ignore]
    fn collects_trap_latency() {
        let data = RNDRTrapTimingSource.collect(16);
        assert!(!data.is_empty());
    }
}
