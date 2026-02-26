//! Sleep jitter entropy source.
//!
//! Requests zero-duration sleeps and measures the actual elapsed time to
//! capture OS scheduler non-determinism.

use std::thread;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

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
    category: SourceCategory::Scheduling,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 0.4,
    composite: false,
    is_fast: true,
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
    #[ignore] // Run with: cargo test -- --ignored
    fn sleep_jitter_collects_bytes() {
        let src = SleepJitterSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn source_info_name() {
        assert_eq!(SleepJitterSource.name(), "sleep_jitter");
    }

    #[test]
    fn source_info_category() {
        assert_eq!(
            SleepJitterSource.info().category,
            SourceCategory::Scheduling
        );
        assert!(!SleepJitterSource.info().composite);
    }
}
