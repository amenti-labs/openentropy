//! Shared source stream sampling helpers used across CLI, server, SDK, and examples.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::metrics::standard::SourceMeasurementRecord;
use crate::pool::EntropyPool;

/// Default per-source timeout for shared diagnostics sampling.
pub const DEFAULT_SAMPLE_TIMEOUT_SECS: f64 = 3.0;

/// Raw stream sample from one named source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRawStreamSample {
    pub name: String,
    pub category: String,
    pub data: Vec<u8>,
}

/// Collect raw streams from all registered sources.
pub fn collect_source_stream_samples(
    pool: &EntropyPool,
    sample_bytes: usize,
    min_samples: usize,
) -> Vec<SourceRawStreamSample> {
    collect_source_stream_samples_with_timeout(
        pool,
        sample_bytes,
        min_samples,
        DEFAULT_SAMPLE_TIMEOUT_SECS,
    )
}

/// Collect raw streams from all registered sources using timeout-safe parallel sampling.
pub fn collect_source_stream_samples_with_timeout(
    pool: &EntropyPool,
    sample_bytes: usize,
    min_samples: usize,
    timeout_secs: f64,
) -> Vec<SourceRawStreamSample> {
    pool.collect_source_raw_samples_parallel(sample_bytes.max(1), timeout_secs, min_samples.max(1))
        .into_iter()
        .map(|s| SourceRawStreamSample {
            name: s.name,
            category: s.category,
            data: s.data,
        })
        .collect()
}

/// Collect raw streams from a selected set of source names.
pub fn collect_named_source_stream_samples(
    pool: &EntropyPool,
    source_names: &[String],
    sample_bytes: usize,
    min_samples: usize,
) -> Vec<SourceRawStreamSample> {
    let selected: HashSet<&str> = source_names.iter().map(|s| s.as_str()).collect();
    collect_source_stream_samples(pool, sample_bytes, min_samples)
        .into_iter()
        .filter(|s| selected.contains(s.name.as_str()))
        .collect()
}

/// Convert sampled rows into `(name, bytes)` tuples expected by analysis helpers.
pub fn to_named_streams(samples: &[SourceRawStreamSample]) -> Vec<(String, Vec<u8>)> {
    samples
        .iter()
        .map(|s| (s.name.clone(), s.data.clone()))
        .collect()
}

/// Convert sampled rows into canonical per-source standard measurement records.
pub fn to_measurement_records(samples: &[SourceRawStreamSample]) -> Vec<SourceMeasurementRecord> {
    samples
        .iter()
        .map(|s| SourceMeasurementRecord::from_bytes(&s.name, &s.category, &s.data, None))
        .collect()
}
