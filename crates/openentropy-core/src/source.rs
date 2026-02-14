//! Abstract entropy source trait and runtime state.
//!
//! Every entropy source implements the [`EntropySource`] trait, which provides
//! metadata via [`SourceInfo`], availability checking, and raw sample collection.

use std::time::Duration;

/// Category of entropy source, used for classification and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceCategory {
    Timing,
    System,
    Network,
    Hardware,
    Silicon,
    CrossDomain,
    Novel,
    Frontier,
}

impl std::fmt::Display for SourceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timing => write!(f, "timing"),
            Self::System => write!(f, "system"),
            Self::Network => write!(f, "network"),
            Self::Hardware => write!(f, "hardware"),
            Self::Silicon => write!(f, "silicon"),
            Self::CrossDomain => write!(f, "cross_domain"),
            Self::Novel => write!(f, "novel"),
            Self::Frontier => write!(f, "frontier"),
        }
    }
}

/// Metadata about an entropy source.
///
/// Each source declares its name, a human-readable description, a physics
/// explanation of how it harvests entropy, its category, platform requirements,
/// and an estimated entropy rate in bits per sample.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// Unique identifier (e.g. `"clock_jitter"`).
    pub name: &'static str,
    /// One-line human-readable description.
    pub description: &'static str,
    /// Physics explanation of the entropy mechanism.
    pub physics: &'static str,
    /// Source category for classification.
    pub category: SourceCategory,
    /// Platform requirements (e.g. `["macos"]`).
    pub platform_requirements: &'static [&'static str],
    /// Estimated entropy rate in bits per sample.
    pub entropy_rate_estimate: f64,
    /// Whether this is a composite source (combines multiple standalone sources).
    ///
    /// Composite sources don't measure a single independent entropy domain.
    /// They combine or interleave other sources. The CLI displays them
    /// separately from standalone sources.
    pub composite: bool,
}

/// Trait that every entropy source must implement.
pub trait EntropySource: Send + Sync {
    /// Source metadata.
    fn info(&self) -> &SourceInfo;

    /// Check if this source can operate on the current machine.
    fn is_available(&self) -> bool;

    /// Collect raw entropy samples. Returns a `Vec<u8>` of up to `n_samples` bytes.
    fn collect(&self, n_samples: usize) -> Vec<u8>;

    /// Convenience: name from info.
    fn name(&self) -> &'static str {
        self.info().name
    }
}

/// Runtime state for a registered source in the pool.
pub struct SourceState {
    pub source: Box<dyn EntropySource>,
    pub weight: f64,
    pub total_bytes: u64,
    pub failures: u64,
    pub last_entropy: f64,
    pub last_min_entropy: f64,
    pub last_collect_time: Duration,
    pub healthy: bool,
}

impl SourceState {
    pub fn new(source: Box<dyn EntropySource>, weight: f64) -> Self {
        Self {
            source,
            weight,
            total_bytes: 0,
            failures: 0,
            last_entropy: 0.0,
            last_min_entropy: 0.0,
            last_collect_time: Duration::ZERO,
            healthy: true,
        }
    }
}
