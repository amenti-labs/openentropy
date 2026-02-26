//! Clock jitter entropy source.
//!
//! Measures phase noise between two independent clock oscillators
//! (`Instant` vs `SystemTime`).

use std::time::{Instant, SystemTime};

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

/// Measures timing jitter between two clock readout paths (`Instant` vs
/// `SystemTime`). On platforms where these use the same underlying oscillator,
/// the entropy comes from variable OS abstraction overhead, cache state, and
/// interrupt timing — not from independent PLLs.
pub struct ClockJitterSource;

static CLOCK_JITTER_INFO: SourceInfo = SourceInfo {
    name: "clock_jitter",
    description: "Timing jitter between Instant and SystemTime readout paths",
    physics: "Reads both Instant (monotonic) and SystemTime (wall clock) in \
              rapid succession and measures the time for the pair of reads. \
              Jitter comes from variable OS clock-readout overhead, cache state, \
              and interrupt timing between the two clock API calls.",
    category: SourceCategory::Timing,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 0.5,
    composite: false,
    is_fast: true,
};

impl EntropySource for ClockJitterSource {
    fn info(&self) -> &SourceInfo {
        &CLOCK_JITTER_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings = Vec::with_capacity(raw_count);

        for _ in 0..raw_count {
            // Measure the time to read both clock sources back-to-back.
            // The jitter comes from variable readout overhead and
            // interrupt/cache interference between the two calls.
            let t0 = Instant::now();

            let _wall = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            std::hint::black_box(&_wall);

            let elapsed = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed);
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn clock_jitter_collects_bytes() {
        let src = ClockJitterSource;
        assert!(src.is_available());
        let data = src.collect(128);
        assert!(!data.is_empty());
        assert!(data.len() <= 128);
        let first = data[0];
        assert!(data.iter().any(|&b| b != first), "all bytes were identical");
    }

    #[test]
    fn source_info_name() {
        assert_eq!(ClockJitterSource.name(), "clock_jitter");
    }

    #[test]
    fn source_info_category() {
        assert_eq!(ClockJitterSource.info().category, SourceCategory::Timing);
        assert!(!ClockJitterSource.info().composite);
    }
}
