//! Timing-based entropy sources: clock jitter, mach_absolute_time, and sleep jitter.
//!
//! **Raw output characteristics:** LSBs of timing deltas and clock differences.
//! Shannon entropy ~2-5 bits/byte depending on source. Clock jitter has lowest
//! entropy rate; mach_timing and sleep_jitter are higher due to pipeline effects.

use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

// ---------------------------------------------------------------------------
// ClockJitterSource
// ---------------------------------------------------------------------------

/// Measures phase noise between two independent clock oscillators
/// (`Instant` vs `SystemTime`). Each clock is driven by a separate PLL on the
/// SoC; thermal noise in the VCO causes random frequency drift. The LSBs of
/// their difference are genuine analog entropy.
pub struct ClockJitterSource;

static CLOCK_JITTER_INFO: SourceInfo = SourceInfo {
    name: "clock_jitter",
    description: "Phase noise between Instant and SystemTime clocks",
    physics: "Measures phase noise between two independent clock oscillators \
              (perf_counter vs monotonic). Each clock is driven by a separate \
              PLL (Phase-Locked Loop) on the SoC. Thermal noise in the PLL's \
              voltage-controlled oscillator causes random frequency drift — \
              the LSBs of their difference are genuine analog entropy from \
              crystal oscillator physics.",
    category: SourceCategory::Timing,
    platform_requirements: &[],
    entropy_rate_estimate: 0.5,
};

impl EntropySource for ClockJitterSource {
    fn info(&self) -> &SourceInfo {
        &CLOCK_JITTER_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(n_samples);

        for _ in 0..n_samples {
            let mono = Instant::now();
            let wall = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();

            let mono2 = Instant::now();
            let mono_delta_ns = mono2.duration_since(mono).as_nanos() as u64;
            let wall_ns = wall.as_nanos() as u64;

            let delta = mono_delta_ns ^ wall_ns;
            output.push(delta as u8);
        }

        output
    }
}

// ---------------------------------------------------------------------------
// MachTimingSource  (macOS only)
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn mach_absolute_time() -> u64;
}

/// Reads the ARM system counter (`mach_absolute_time`) at sub-nanosecond
/// resolution with variable micro-workloads between samples. Returns raw
/// LSBs of timing deltas — no conditioning applied.
pub struct MachTimingSource;

static MACH_TIMING_INFO: SourceInfo = SourceInfo {
    name: "mach_timing",
    description: "mach_absolute_time() with micro-workload jitter (raw LSBs)",
    physics: "Reads the ARM system counter (mach_absolute_time) at sub-nanosecond \
              resolution with variable micro-workloads between samples. The timing \
              jitter comes from CPU pipeline state: instruction reordering, branch \
              prediction, cache state, interrupt coalescing, and power-state \
              transitions.",
    category: SourceCategory::Timing,
    platform_requirements: &["macOS"],
    entropy_rate_estimate: 0.3,
};

impl EntropySource for MachTimingSource {
    fn info(&self) -> &SourceInfo {
        &MACH_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Collect raw LSBs from mach_absolute_time with micro-workloads.
        let raw_count = n_samples * 2 + 64;
        let mut raw = Vec::with_capacity(n_samples);

        for i in 0..raw_count {
            let t0 = unsafe { mach_absolute_time() };

            // Variable micro-workload to perturb pipeline state.
            let iterations = (i % 7) + 1;
            let mut sink: u64 = t0;
            for _ in 0..iterations {
                sink = sink.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(sink);

            let t1 = unsafe { mach_absolute_time() };
            let delta = t1.wrapping_sub(t0);

            // Raw LSB of the delta — unconditioned
            raw.push(delta as u8);

            if raw.len() >= n_samples {
                break;
            }
        }

        raw.truncate(n_samples);
        raw
    }
}

// ---------------------------------------------------------------------------
// SleepJitterSource
// ---------------------------------------------------------------------------

/// Requests zero-duration sleeps and measures the actual elapsed time.
/// The jitter captures OS scheduler non-determinism: timer interrupt
/// granularity, thread priority decisions, runqueue length, and DVFS.
pub struct SleepJitterSource;

static SLEEP_JITTER_INFO: SourceInfo = SourceInfo {
    name: "sleep_jitter",
    description: "OS scheduler jitter from zero-duration sleeps",
    physics: "Requests zero-duration sleeps and measures actual wake time. The jitter \
              captures OS scheduler non-determinism: timer interrupt granularity (1-4ms), \
              thread priority decisions, runqueue length, and thermal-dependent clock \
              frequency scaling (DVFS).",
    category: SourceCategory::Timing,
    platform_requirements: &[],
    entropy_rate_estimate: 0.4,
};

impl EntropySource for SleepJitterSource {
    fn info(&self) -> &SourceInfo {
        &SLEEP_JITTER_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let oversample = n_samples * 2 + 64;
        let mut raw_timings = Vec::with_capacity(oversample);

        for _ in 0..oversample {
            let before = Instant::now();
            thread::sleep(Duration::ZERO);
            let elapsed_ns = before.elapsed().as_nanos() as u64;
            raw_timings.push(elapsed_ns);
        }

        // Compute deltas and XOR adjacent pairs
        let deltas: Vec<u64> = raw_timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        let mut raw = Vec::with_capacity(n_samples);
        for pair in deltas.windows(2) {
            let xored = pair[0] ^ pair[1];
            raw.push(xored as u8);
            if raw.len() >= n_samples {
                break;
            }
        }

        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_jitter_collects_bytes() {
        let src = ClockJitterSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty()); assert!(data.len() <= 128);
        let first = data[0];
        assert!(data.iter().any(|&b| b != first), "all bytes were identical");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn mach_timing_collects_bytes() {
        let src = MachTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn sleep_jitter_collects_bytes() {
        let src = SleepJitterSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn source_info_names() {
        assert_eq!(ClockJitterSource.name(), "clock_jitter");
        assert_eq!(MachTimingSource.name(), "mach_timing");
        assert_eq!(SleepJitterSource.name(), "sleep_jitter");
    }
}
