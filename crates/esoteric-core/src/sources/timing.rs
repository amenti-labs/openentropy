//! Timing-based entropy sources: clock jitter, mach_absolute_time, and sleep jitter.

use std::thread;
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};

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
            // Read both clocks as close together as possible.
            let mono = Instant::now();
            let wall = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();

            // The monotonic clock gives us an opaque instant; read it again
            // to get a nanos-since-first-read delta.
            let mono2 = Instant::now();
            let mono_delta_ns = mono2.duration_since(mono).as_nanos() as u64;

            let wall_ns = wall.as_nanos() as u64;

            // XOR the two clocks together and take the lowest byte.
            // The LSBs capture the independent jitter of each oscillator.
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
/// resolution with variable micro-workloads between samples. Applies
/// Von Neumann debiasing on LSBs and SHA-256 conditioning in 64-byte blocks.
pub struct MachTimingSource;

static MACH_TIMING_INFO: SourceInfo = SourceInfo {
    name: "mach_timing",
    description: "mach_absolute_time() with micro-workload jitter + SHA-256 conditioning",
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
        // We need extra raw samples because Von Neumann debiasing discards ~75%
        // and SHA-256 conditioning maps 64 raw bytes -> 32 output bytes.
        // Collect generously so we can meet the requested n_samples.
        let raw_count = n_samples * 16 + 256;

        // Phase 1: Collect raw LSBs from mach_absolute_time with micro-workloads.
        let mut raw_lsbs = Vec::with_capacity(raw_count);
        for i in 0..raw_count {
            let t0 = unsafe { mach_absolute_time() };

            // Variable micro-workload to perturb pipeline state.
            let iterations = (i % 7) + 1;
            let mut sink: u64 = t0;
            for _ in 0..iterations {
                sink = sink.wrapping_mul(6364136223846793005).wrapping_add(1);
            }

            // Prevent the compiler from optimizing away the workload.
            std::hint::black_box(sink);

            let t1 = unsafe { mach_absolute_time() };
            let delta = t1.wrapping_sub(t0);

            // Take the lowest byte of the delta as a raw sample.
            raw_lsbs.push(delta as u8);
        }

        // Phase 2: Von Neumann debiasing — extract unbiased bits.
        let debiased = von_neumann_debias(&raw_lsbs);

        if debiased.is_empty() {
            // Fallback: return raw LSBs truncated to n_samples if debiasing
            // eliminated everything (extremely unlikely with enough input).
            raw_lsbs.truncate(n_samples);
            return raw_lsbs;
        }

        // Phase 3: SHA-256 conditioning in 64-byte blocks.
        let mut output = Vec::with_capacity(n_samples);
        let mut hasher_state = [0u8; 32];

        for chunk in debiased.chunks(64) {
            let mut h = Sha256::new();
            h.update(&hasher_state);
            h.update(chunk);
            hasher_state = h.finalize().into();
            output.extend_from_slice(&hasher_state);

            if output.len() >= n_samples {
                break;
            }
        }

        output.truncate(n_samples);
        output
    }
}

/// Von Neumann debiasing: takes pairs of bits; (0,1) -> 0, (1,0) -> 1,
/// same -> discard. Packs surviving bits back into bytes.
fn von_neumann_debias(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::new();

    for byte in data {
        for i in (0..8).step_by(2) {
            let b1 = (byte >> (7 - i)) & 1;
            let b2 = (byte >> (6 - i)) & 1;
            if b1 != b2 {
                bits.push(b1);
            }
        }
    }

    // Pack bits back into bytes.
    let mut result = Vec::with_capacity(bits.len() / 8);
    for chunk in bits.chunks_exact(8) {
        let mut byte = 0u8;
        for (j, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - j);
        }
        result.push(byte);
    }

    result
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
        let mut output = Vec::with_capacity(n_samples);

        for _ in 0..n_samples {
            let before = Instant::now();
            thread::sleep(Duration::ZERO);
            let elapsed_ns = before.elapsed().as_nanos() as u64;

            // The LSB of the elapsed nanoseconds carries the jitter.
            output.push(elapsed_ns as u8);
        }

        output
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
        assert_eq!(data.len(), 128);
        // Sanity: not all bytes should be identical.
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
        assert_eq!(data.len(), 64);
    }

    #[test]
    fn von_neumann_reduces_size() {
        // Von Neumann debiasing should always produce fewer bytes than input.
        let input = vec![0xAA; 100]; // 10101010 pattern -> every pair differs
        let output = von_neumann_debias(&input);
        // 100 bytes * 4 pairs each = 400 pairs, all differing = 400 bits = 50 bytes
        assert_eq!(output.len(), 50);
    }

    #[test]
    fn source_info_names() {
        assert_eq!(ClockJitterSource.name(), "clock_jitter");
        assert_eq!(MachTimingSource.name(), "mach_timing");
        assert_eq!(SleepJitterSource.name(), "sleep_jitter");
    }
}
