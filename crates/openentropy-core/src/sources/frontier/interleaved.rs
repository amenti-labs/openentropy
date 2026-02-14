//! Interleaved frontier source — **COMPOSITE** entropy from cross-source interference.
//!
//! This is NOT a standalone entropy source. It rapidly alternates between all
//! standalone frontier sources and harvests the interference between them as
//! additional independent entropy. See [`InterleavedFrontierSource`] for details.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

use super::standalone_frontier_sources;

/// **[COMPOSITE]** Rapidly alternates between all standalone frontier sources,
/// harvesting cross-source interference as independent entropy.
///
/// # What it measures
/// Nanosecond timing of small-batch collections from each standalone frontier
/// source in round-robin order. The transition timing between sources captures
/// the interference each source's system perturbations create for the next.
///
/// # Why it's entropic
/// When frontier sources are sampled in rapid alternation, each source's
/// system state perturbations affect the next source's measurements:
/// - AMX dispatch affects memory controller state, which affects TLB shootdown timing
/// - Pipe buffer zone allocations affect kernel zone magazine state, which
///   affects Mach port allocation timing
/// - Thread lifecycle scheduling decisions affect kqueue timer delivery timing
/// - Each source's syscalls perturb the CPU pipeline, TLB, and cache state
///   that the next source measures
///
/// The cross-source interference is itself a source of entropy that is
/// independent from each individual source's entropy.
///
/// # What makes it unique
/// This is a **composite** source — it does not measure a single physical
/// entropy domain but instead combines all standalone frontier sources. It is
/// the only source in the system that exploits inter-source interference.
///
/// # Why it's marked composite
/// Unlike standalone sources that each harvest one independent entropy domain,
/// this source has no unique physical mechanism. Its output is a function of
/// all other frontier sources. It is listed separately in CLI output.
pub struct InterleavedFrontierSource;

static INTERLEAVED_FRONTIER_INFO: SourceInfo = SourceInfo {
    name: "interleaved_frontier",
    description: "[COMPOSITE] Cross-source interference from rapidly alternating frontier sources",
    physics: "Rapidly alternates between all frontier sources (AMX, thread lifecycle, Mach IPC, \
              TLB shootdown, pipe buffer, kqueue). Each source's system perturbations affect \
              the next source's measurements: AMX affects memory state, pipe allocation affects \
              kernel zones, thread scheduling affects timer delivery. The cross-source \
              interference pattern is itself independent entropy.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 3000.0,
    composite: true,
};

impl EntropySource for InterleavedFrontierSource {
    fn info(&self) -> &SourceInfo {
        &INTERLEAVED_FRONTIER_INFO
    }

    fn is_available(&self) -> bool {
        // Available if at least 2 frontier sources are available.
        let sources = standalone_frontier_sources();
        sources.iter().filter(|s| s.is_available()).count() >= 2
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let sources = standalone_frontier_sources();
        let available: Vec<Box<dyn EntropySource>> = sources
            .into_iter()
            .filter(|s| s.is_available())
            .collect();

        if available.is_empty() {
            return Vec::new();
        }

        // Collect small batches from each source in round-robin, measuring
        // the transition timing between sources as additional entropy.
        let batch_size = 4;
        let mut timings: Vec<u64> = Vec::with_capacity(n_samples * 4 + 64);
        let raw_count = n_samples * 4 + 64;
        let mut all_bytes: Vec<u8> = Vec::new();

        let mut i = 0;
        while i < raw_count {
            for source in &available {
                let t0 = mach_time();
                let bytes = source.collect(batch_size);
                let t1 = mach_time();

                all_bytes.extend_from_slice(&bytes);
                timings.push(t1.wrapping_sub(t0));

                i += 1;
                if i >= raw_count {
                    break;
                }
            }
        }

        // Mix transition timings with collected bytes.
        let timing_entropy = extract_timing_entropy(&timings, n_samples);

        // XOR timing entropy with collected source bytes for final output.
        let mut result = Vec::with_capacity(n_samples);
        for j in 0..n_samples {
            let t_byte = timing_entropy.get(j).copied().unwrap_or(0);
            let s_byte = all_bytes.get(j).copied().unwrap_or(0);
            result.push(t_byte ^ s_byte);
        }
        result.truncate(n_samples);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = InterleavedFrontierSource;
        assert_eq!(src.name(), "interleaved_frontier");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(src.info().composite);
    }

    #[test]
    #[ignore] // Uses multiple syscalls
    fn collects_bytes() {
        let src = InterleavedFrontierSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
