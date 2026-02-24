//! Deterministic PRNG control source (xorshift64).
//!
//! A negative control for consciousness-RNG experiments. If intention appears
//! to influence a deterministic PRNG, it indicates a statistical artifact
//! rather than a genuine anomalous effect on physical hardware.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use std::sync::atomic::{AtomicU64, Ordering};

static INFO: SourceInfo = SourceInfo {
    name: "prng_control",
    description: "Deterministic xorshift64 PRNG (negative control for consciousness experiments)",
    physics: "No physical entropy — deterministic pseudorandom number generator. \
              Used as a negative control: if consciousness experiments show effects \
              on this source, it indicates statistical artifact, not anomalous influence.",
    category: SourceCategory::System,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 0.0,
    composite: false,
};

/// Deterministic xorshift64 PRNG implementing `EntropySource`.
///
/// Seed is fixed at construction. The output is deterministic given the seed,
/// meaning any observed "consciousness effect" on this source is a false positive.
pub struct PrngControlSource {
    state: AtomicU64,
}

impl PrngControlSource {
    pub fn new(seed: u64) -> Self {
        Self {
            state: AtomicU64::new(seed),
        }
    }
}

impl Default for PrngControlSource {
    fn default() -> Self {
        Self::new(0xDEAD_BEEF_CAFE_BABE)
    }
}

impl EntropySource for PrngControlSource {
    fn info(&self) -> &SourceInfo {
        &INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(n_samples);
        let mut state = self.state.load(Ordering::Relaxed);

        for _ in 0..n_samples {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            output.push((state & 0xFF) as u8);
        }

        self.state.store(state, Ordering::Relaxed);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_is_correct() {
        let src = PrngControlSource::default();
        let info = src.info();
        assert_eq!(info.name, "prng_control");
        assert_eq!(info.category, SourceCategory::System);
        assert_eq!(info.entropy_rate_estimate, 0.0);
        assert!(!info.composite);
    }

    #[test]
    fn always_available() {
        assert!(PrngControlSource::default().is_available());
    }

    #[test]
    fn produces_requested_bytes() {
        let src = PrngControlSource::default();
        let data = src.collect(100);
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn deterministic_with_same_seed() {
        let src1 = PrngControlSource::new(12345);
        let src2 = PrngControlSource::new(12345);
        let data1 = src1.collect(32);
        let data2 = src2.collect(32);
        assert_eq!(data1, data2);
    }

    #[test]
    fn different_seeds_produce_different_output() {
        let src1 = PrngControlSource::new(12345);
        let src2 = PrngControlSource::new(54321);
        let data1 = src1.collect(32);
        let data2 = src2.collect(32);
        assert_ne!(data1, data2);
    }

    #[test]
    fn not_all_same_byte() {
        let src = PrngControlSource::default();
        let data = src.collect(64);
        // Should have some variety
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 1, "PRNG output should not be constant");
    }
}
