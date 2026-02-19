//! Standardized entropy measurement primitives shared across the project.
//!
//! This module provides a single canonical representation for common per-stream
//! quality metrics used throughout openentropy (Shannon entropy, min-entropy,
//! compression ratio, and throughput).

use flate2::Compression;
use flate2::write::ZlibEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::conditioning::{quick_min_entropy, quick_shannon};

/// Canonical per-stream entropy measurements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntropyMeasurements {
    /// Number of bytes analyzed.
    pub bytes: usize,
    /// Shannon entropy in bits/byte.
    pub shannon_entropy: f64,
    /// Min-entropy (MCV style) in bits/byte.
    pub min_entropy: f64,
    /// zlib level-9 compression ratio. Lower means more structure.
    pub compression_ratio: f64,
    /// Throughput in bytes/second for this sample.
    pub throughput_bps: f64,
}

/// Canonical per-source measurement row used across CLI, server, and SDKs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceMeasurementRecord {
    /// Source name.
    pub name: String,
    /// Source category label.
    pub category: String,
    /// Standard measurements for the sampled stream.
    pub measurements: EntropyMeasurements,
}

impl Default for EntropyMeasurements {
    fn default() -> Self {
        Self {
            bytes: 0,
            shannon_entropy: 0.0,
            min_entropy: 0.0,
            compression_ratio: 0.0,
            throughput_bps: 0.0,
        }
    }
}

impl EntropyMeasurements {
    /// Measure a byte stream with optional elapsed seconds for throughput.
    pub fn from_bytes(data: &[u8], elapsed_seconds: Option<f64>) -> Self {
        if data.is_empty() {
            return Self::default();
        }
        let elapsed = elapsed_seconds.unwrap_or(0.0);
        let throughput_bps = if elapsed > 0.0 {
            data.len() as f64 / elapsed
        } else {
            0.0
        };
        Self {
            bytes: data.len(),
            shannon_entropy: quick_shannon(data),
            min_entropy: quick_min_entropy(data).max(0.0),
            compression_ratio: compression_ratio(data),
            throughput_bps,
        }
    }
}

impl SourceMeasurementRecord {
    /// Build a canonical source measurement record from raw bytes.
    pub fn from_bytes(
        name: impl Into<String>,
        category: impl Into<String>,
        data: &[u8],
        elapsed_seconds: Option<f64>,
    ) -> Self {
        Self {
            name: name.into(),
            category: category.into(),
            measurements: EntropyMeasurements::from_bytes(data, elapsed_seconds),
        }
    }
}

/// Compression ratio helper used as a structure proxy.
pub fn compression_ratio(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    if enc.write_all(data).is_err() {
        return 0.0;
    }
    match enc.finish() {
        Ok(c) => c.len() as f64 / data.len() as f64,
        Err(_) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_empty() {
        let m = EntropyMeasurements::from_bytes(&[], Some(1.0));
        assert_eq!(m.bytes, 0);
        assert_eq!(m.shannon_entropy, 0.0);
        assert_eq!(m.min_entropy, 0.0);
    }

    #[test]
    fn metrics_uniform_like() {
        let data: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let m = EntropyMeasurements::from_bytes(&data, Some(0.5));
        assert!(m.shannon_entropy > 7.9);
        assert!(m.min_entropy > 7.0);
        assert!(m.throughput_bps > 0.0);
    }

    #[test]
    fn source_measurement_record() {
        let data = vec![0u8; 1024];
        let row = SourceMeasurementRecord::from_bytes("clock_jitter", "timing", &data, Some(1.0));
        assert_eq!(row.name, "clock_jitter");
        assert_eq!(row.category, "timing");
        assert_eq!(row.measurements.bytes, 1024);
    }
}
