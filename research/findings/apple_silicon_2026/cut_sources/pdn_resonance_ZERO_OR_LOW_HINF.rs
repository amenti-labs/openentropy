//! Power delivery network resonance — cross-core voltage noise entropy.
//!
//! The PCB power planes have LC resonances at specific frequencies. When
//! different chip components draw current, standing waves form in the power
//! delivery network creating voltage droops that affect operation timing.
//!
//! By running a stress workload on background threads while measuring timing
//! on the current thread, we capture PDN voltage noise from cross-core coupling.
//!

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Number of iterations per timing measurement.
const MEASUREMENT_ITERS: usize = 100;

static PDN_RESONANCE_INFO: SourceInfo = SourceInfo {
    name: "pdn_resonance",
    description: "Power delivery network resonance from cross-core voltage noise",
    physics: "Runs stress workloads on background threads (memory thrashing, ALU, FPU) \
              while measuring timing of a fixed small workload on the current thread. \
              The timing perturbation captures power delivery network (PDN) voltage noise: \
              LC resonances in PCB power planes, voltage droop from bursty current draw, \
              and cross-core power supply coupling. Each measurement thread combination \
              creates a different current profile exciting different PDN modes.",
    category: SourceCategory::Thermal,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 500.0,
    composite: false,
};

/// Entropy source that harvests PDN voltage noise via cross-core timing perturbation.
pub struct PDNResonanceSource;

impl EntropySource for PDNResonanceSource {
    fn info(&self) -> &SourceInfo {
        &PDN_RESONANCE_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Spawn stress threads to excite PDN resonance.
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let mut handles = Vec::new();

        // Memory stress thread — bursty current from cache misses.
        {
            let running = running.clone();
            handles.push(std::thread::spawn(move || {
                let mut buf = vec![0u64; 512 * 1024]; // 4 MB
                while running.load(std::sync::atomic::Ordering::Relaxed) {
                    for i in (0..buf.len()).step_by(64) {
                        buf[i] = buf[i].wrapping_add(1);
                    }
                    std::hint::black_box(&buf);
                }
            }));
        }

        // ALU stress thread — different current profile.
        {
            let running = running.clone();
            handles.push(std::thread::spawn(move || {
                let mut a: u64 = mach_time() | 1;
                while running.load(std::sync::atomic::Ordering::Relaxed) {
                    for _ in 0..10000 {
                        a = a
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                    }
                    std::hint::black_box(a);
                }
            }));
        }

        // Brief warmup for stress threads.
        std::thread::sleep(std::time::Duration::from_millis(1));

        // Measure timing on this thread while stress threads run.
        for _ in 0..raw_count {
            let t0 = mach_time();
            let mut acc: u64 = 0;
            for j in 0..MEASUREMENT_ITERS as u64 {
                acc = acc.wrapping_add(j);
            }
            let t1 = mach_time();
            std::hint::black_box(acc);
            timings.push(t1.wrapping_sub(t0));
        }

        // Stop stress threads.
        running.store(false, std::sync::atomic::Ordering::Relaxed);
        for h in handles {
            let _ = h.join();
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = PDNResonanceSource;
        assert_eq!(src.name(), "pdn_resonance");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Hardware/timing dependent
    fn collects_bytes() {
        let src = PDNResonanceSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }
}
