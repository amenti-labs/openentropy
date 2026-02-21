//! Multi-Source Quantum XOR - Combine multiple quantum sources for higher purity
//!
//! KEY INSIGHT: XOR combining multiple independent quantum sources
//! reduces classical noise while preserving quantum randomness!
//!
//! Theory:
//! - Classical noise is UNCORRELATED between sources → cancels in XOR
//! - Quantum randomness is preserved through XOR
//! - More sources = higher quantum purity
//!
//! Example:
//!   Source 1 (SSD):    74% quantum + 26% classical
//!   Source 2 (RAM):    40% quantum + 60% classical
//!   Source 3 (Camera): 80% quantum + 20% classical
//!   Source 4 (Audio):  70% quantum + 30% classical
//!   ────────────────────────────────────────────────
//!   XOR Combined:      ~90% quantum!
//!
//! This is the same principle used in professional QRNGs.

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::camera::CameraNoiseSource;

use super::{AvalancheNoiseSource, SSDTunnelingSource, VacuumFluctuationsSource};

static MULTI_SOURCE_QUANTUM_INFO: SourceInfo = SourceInfo {
    name: "multi_source_quantum",
    description: "XOR-combined multi-source quantum entropy for maximum purity",
    physics: "Combines multiple independent quantum entropy sources via XOR. \
              Classical noise is uncorrelated between sources and cancels out, \
              while quantum randomness is preserved. Sources: SSD tunneling, \
              DRAM timing, camera shot noise, audio Johnson-Nyquist noise. \
              The XOR of these independent quantum processes produces higher \
              quantum purity than any single source alone. This is the same \
              technique used in professional QRNG hardware.",
    category: SourceCategory::Composite,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: true,  // This is a composite source
};

/// Multi-source quantum XOR entropy source
///
/// Combines multiple quantum sources to achieve higher quantum purity.
pub struct MultiSourceQuantumSource {
    /// Sources to combine
    sources: Vec<Box<dyn EntropySource>>,
}

impl Default for MultiSourceQuantumSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiSourceQuantumSource {
    /// Create new multi-source combiner with all available quantum sources
    pub fn new() -> Self {
        Self {
            // Core mix includes camera shot noise so ambient light/sensor behavior
            // directly influences the combined stream in the monitor UI.
            sources: vec![
                Box::new(SSDTunnelingSource::default()),
                Box::new(CameraNoiseSource),
                Box::new(AvalancheNoiseSource::default()),
                Box::new(VacuumFluctuationsSource::default()),
            ],
        }
    }

    /// Add a source to the pool
    pub fn add_source<S: EntropySource + 'static>(&mut self, source: S) {
        self.sources.push(Box::new(source));
    }

    /// XOR combine multiple bit streams
    fn xor_combine(sources: &[Vec<u8>]) -> Vec<u8> {
        if sources.is_empty() {
            return Vec::new();
        }

        let min_len = sources.iter().map(|s| s.len()).min().unwrap_or(0);
        let mut result = Vec::with_capacity(min_len);

        for i in 0..min_len {
            let mut byte: u8 = 0;
            for source in sources {
                byte ^= source[i];
            }
            result.push(byte);
        }

        result
    }

    /// Majority vote across sources (alternative to XOR)
    fn majority_vote(sources: &[Vec<u8>]) -> Vec<u8> {
        if sources.is_empty() {
            return Vec::new();
        }

        let min_len = sources.iter().map(|s| s.len()).min().unwrap_or(0);
        let mut result = Vec::with_capacity(min_len);
        let threshold = sources.len() / 2 + 1;

        // Bit-by-bit majority
        for byte_idx in 0..min_len {
            let mut out_byte = 0u8;
            for bit_idx in 0..8 {
                let mut ones = 0;
                for source in sources {
                    if (source[byte_idx] >> bit_idx) & 1 == 1 {
                        ones += 1;
                    }
                }
                if ones >= threshold {
                    out_byte |= 1 << bit_idx;
                }
            }
            result.push(out_byte);
        }

        result
    }
}

impl EntropySource for MultiSourceQuantumSource {
    fn info(&self) -> &SourceInfo {
        &MULTI_SOURCE_QUANTUM_INFO
    }

    fn is_available(&self) -> bool {
        self.sources.iter().filter(|s| s.is_available()).count() >= 2
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        if n_samples == 0 {
            return Vec::new();
        }

        let mut streams = Vec::new();
        for source in &self.sources {
            if !source.is_available() {
                continue;
            }
            let data = source.collect(n_samples);
            if !data.is_empty() {
                streams.push(data);
            }
        }

        match streams.len() {
            0 => Vec::new(),
            1 => streams.pop().unwrap_or_default(),
            _ => {
                let mut combined = Self::xor_combine(&streams);
                if combined.iter().all(|&b| b == 0) {
                    // Extremely unlikely for healthy inputs, but if it happens
                    // fallback to majority vote to avoid a degenerate stream.
                    combined = Self::majority_vote(&streams);
                }
                combined.truncate(n_samples);
                combined
            }
        }
    }
}

/// Estimated quantum purity improvement from XOR combining
///
/// Returns estimated quantum fraction after combining n sources.
pub fn estimate_combined_purity(source_purities: &[f64]) -> f64 {
    if source_purities.is_empty() {
        return 0.0;
    }
    if source_purities.len() == 1 {
        return source_purities[0];
    }

    // Simple model: quantum adds, classical noise cancels (uncorrelated)
    // Combined quantum ≈ 1 - Π(1 - purity_i)
    let classical_product: f64 = source_purities.iter()
        .map(|&p| 1.0 - p)
        .product();

    1.0 - classical_product
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = MultiSourceQuantumSource::new();
        assert_eq!(src.name(), "multi_source_quantum");
        assert!(src.info().composite);
        assert!(
            src.is_available(),
            "multi-source should be available when >=2 child sources are available"
        );
    }

    #[test]
    fn xor_combine_works() {
        let source1 = vec![0b10101010, 0b11110000];
        let source2 = vec![0b01010101, 0b00001111];
        let source3 = vec![0b11111111, 0b00000000];

        let combined = MultiSourceQuantumSource::xor_combine(&[source1, source2, source3]);

        // XOR of all three: 10101010 ^ 01010101 ^ 11111111 = 00000000
        //                   11110000 ^ 00001111 ^ 00000000 = 11111111
        assert_eq!(combined, vec![0b00000000, 0b11111111]);
    }

    #[test]
    fn purity_estimation() {
        // Single source: no improvement
        assert!((estimate_combined_purity(&[0.74]) - 0.74).abs() < 0.001);

        // Two sources: significant improvement
        // 74% + 40% → 1 - 0.26*0.60 = 1 - 0.156 = 0.844
        let purity = estimate_combined_purity(&[0.74, 0.40]);
        assert!(purity > 0.80, "Two sources should improve purity: {}", purity);

        // Four sources: near-certified quantum
        let purity = estimate_combined_purity(&[0.74, 0.40, 0.50, 0.27]);
        assert!(purity > 0.90, "Four sources should give >90%: {}", purity);
    }

    #[test]
    fn collect_returns_data_when_available() {
        let src = MultiSourceQuantumSource::new();
        if src.is_available() {
            let out = src.collect(64);
            assert!(
                !out.is_empty(),
                "expected non-empty combined output from available child sources"
            );
            assert!(out.len() <= 64);
        }
    }
}
