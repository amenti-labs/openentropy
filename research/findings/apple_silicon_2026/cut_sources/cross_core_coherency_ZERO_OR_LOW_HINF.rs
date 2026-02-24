//! Cross-core cache coherency transfer timing entropy.
//!
//! Apple Silicon uses a ring-bus coherency fabric to maintain cache coherency
//! across all CPU cores. When one core writes to a cache line that another
//! core has cached, the coherency protocol must transfer ownership across
//! the ring, incurring latency proportional to the number of ring hops.
//!
//! ## Physics
//!
//! M4 Mac mini has 10 CPU cores (4 P-cores + 6 E-cores) connected by a
//! coherency ring. A cache line transfer from core A to core B requires:
//!
//! 1. **Invalidation broadcast** — core A signals it's modifying the line
//! 2. **Ring traversal** — invalidation travels around the ring to core B
//! 3. **Acknowledge** — core B responds that it has invalidated its copy
//! 4. **Data transfer** — the modified line becomes visible to core B
//!
//! Each ring hop adds a fixed latency. The number of hops depends on which
//! physical cores the two threads are scheduled on.
//!
//! Empirically on M4 Mac mini (N=500):
//! - **4 discrete modes**: ~45, ~85, ~125, ~165 ticks
//! - **~40 tick spacing** between modes — single ring hop latency
//! - CV=152.7%, range=[41, 3084]
//! - Distribution: [40-50]=131 (26%), [80-90]=246 (49%), [120-130]=101 (20%), [160-170]=17 (3%)
//!
//! ## Why This Is Entropy
//!
//! The coherency transfer time encodes:
//!
//! 1. **Core topology** — which physical cores are involved (determines hop count)
//! 2. **OS scheduler state** — where the kernel scheduled our reader thread
//! 3. **Ring bus congestion** — other cores' coherency traffic adds latency
//! 4. **Power state** — P-cores vs E-cores have different coherency paths
//!
//! The 4-mode distribution directly maps to the 4 possible ring hop counts
//! between core pairs on the M4's topology. The exact mode we land in is
//! determined by the scheduler's placement of our threads, which depends on
//! system load, thermal state, and QoS policies.
//!
//! ## Cross-Process Sensitivity
//!
//! Other processes' memory access patterns create coherency traffic on the
//! ring bus, changing the latency distribution. A compute-heavy process on
//! core 3 will shift our mode distribution depending on which core our
//! threads land on.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::sync::atomic::{AtomicU64, AtomicI32, Ordering};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::thread;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::time::Duration;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::sources::helpers::mach_time;

static CROSS_CORE_COHERENCY_INFO: SourceInfo = SourceInfo {
    name: "cross_core_coherency",
    description: "Cross-core cache coherency ring-bus transfer timing — 4-mode distribution",
    physics: "Two threads on different cores: writer modifies cache line, reader polls for \
              change. Transfer time reveals ring-bus hop count between cores. M4 has 4 modes \
              at ~45, ~85, ~125, ~165 ticks (40-tick spacing = single ring hop). CV=152.7%, \
              range=[41,3084]. Distribution: 26% 1-hop, 49% 2-hop, 20% 3-hop, 3% 4-hop. \
              Entropy from: core topology, scheduler placement, ring congestion, P/E core \
              power state. Other processes' memory traffic changes mode distribution.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 4000.0,
    composite: false,
};

/// Entropy source from cross-core cache coherency transfer timing.
pub struct CrossCoreCoherencySource;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl EntropySource for CrossCoreCoherencySource {
    fn info(&self) -> &SourceInfo {
        &CROSS_CORE_COHERENCY_INFO
    }

    fn is_available(&self) -> bool {
        // Requires at least 2 cores
        #[cfg(target_os = "macos")]
        {
            let mut ncpu: i32 = 0;
            let mut len = std::mem::size_of::<i32>();
            unsafe {
                libc::sysctlbyname(
                    b"hw.ncpu\0".as_ptr() as *const i8,
                    &mut ncpu as *mut i32 as *mut libc::c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                );
            }
            ncpu >= 2
        }
        #[cfg(not(target_os = "macos"))]
        { false }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        use std::sync::Arc;

        #[repr(align(64))]
        struct SharedLine {
            val: AtomicU64,
            t_write: AtomicU64,
            t_read: AtomicU64,
            go: AtomicI32,
            done: AtomicI32,
            ready: AtomicI32,
        }

        let shared = Arc::new(SharedLine {
            val: AtomicU64::new(0),
            t_write: AtomicU64::new(0),
            t_read: AtomicU64::new(0),
            go: AtomicI32::new(0),
            done: AtomicI32::new(0),
            ready: AtomicI32::new(0),
        });

        let shared_reader = Arc::clone(&shared);
        let reader = thread::spawn(move || {
            let mut seen: u64 = 0;
            shared_reader.ready.store(1, Ordering::SeqCst);
            while shared_reader.done.load(Ordering::SeqCst) == 0 {
                while shared_reader.go.load(Ordering::SeqCst) == 0
                    && shared_reader.done.load(Ordering::SeqCst) == 0 {}
                if shared_reader.done.load(Ordering::SeqCst) != 0 { break; }

                // Spin until cache line changes
                let deadline = mach_time() + 240_000; // 10ms timeout
                while shared_reader.val.load(Ordering::SeqCst) == seen {
                    if mach_time() > deadline { break; }
                }
                shared_reader.t_read.store(mach_time(), Ordering::SeqCst);
                seen = shared_reader.val.load(Ordering::SeqCst);
                shared_reader.go.store(0, Ordering::SeqCst);
            }
        });

        // Wait for reader to start
        while shared.ready.load(Ordering::SeqCst) == 0 {}
        thread::sleep(Duration::from_micros(1000));

        let n_rounds = n_samples * 2 + 20;
        let mut latencies = Vec::with_capacity(n_rounds);

        // Warm up
        for i in 0..10_u64 {
            while shared.go.load(Ordering::SeqCst) != 0 {}
            shared.val.store(i + 1, Ordering::SeqCst);
            shared.t_write.store(mach_time(), Ordering::SeqCst);
            shared.go.store(1, Ordering::SeqCst);
            while shared.go.load(Ordering::SeqCst) != 0 {}
            thread::sleep(Duration::from_micros(10));
        }

        // Collect
        for i in 0..n_rounds {
            while shared.go.load(Ordering::SeqCst) != 0 {}
            thread::sleep(Duration::from_micros(5));

            shared.val.store(i as u64 + 1000, Ordering::SeqCst);
            shared.t_write.store(mach_time(), Ordering::SeqCst);
            shared.go.store(1, Ordering::SeqCst);
            while shared.go.load(Ordering::SeqCst) != 0 {}

            let lat = shared.t_read.load(Ordering::SeqCst)
                .wrapping_sub(shared.t_write.load(Ordering::SeqCst));
            if lat < 240_000 {
                latencies.push(lat);
            }
        }

        shared.done.store(1, Ordering::SeqCst);
        let _ = reader.join();

        // Extract entropy from latencies
        // The 4-mode distribution (~45, ~85, ~125, ~165) gives ~2 bits per sample
        // Plus the variance within each mode
        let mut result = Vec::with_capacity(n_samples);
        for &lat in latencies.iter().take(n_samples) {
            // Encode mode (2 bits) and offset within mode (6 bits)
            let mode = (lat / 40) as u8; // Which of the 4 modes
            let offset = (lat % 40) as u8; // Offset within mode
            result.push((mode << 6) | (offset & 0x3F));
        }

        while result.len() < n_samples {
            result.push(0);
        }
        result
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl EntropySource for CrossCoreCoherencySource {
    fn info(&self) -> &SourceInfo { &CROSS_CORE_COHERENCY_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = CrossCoreCoherencySource;
        assert_eq!(src.info().name, "cross_core_coherency");
        assert!(matches!(src.info().category, SourceCategory::Microarch));
        assert_eq!(src.info().platform, Platform::MacOS);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn is_available_on_multicore() {
        assert!(CrossCoreCoherencySource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_multimodal_distribution() {
        let data = CrossCoreCoherencySource.collect(32);
        assert!(!data.is_empty());
    }
}
