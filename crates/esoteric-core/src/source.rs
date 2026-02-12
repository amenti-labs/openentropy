//! Abstract entropy source trait and runtime state.

use std::time::Duration;

/// Category of entropy source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCategory {
    Timing,
    System,
    Network,
    Hardware,
    Silicon,
    CrossDomain,
    Novel,
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
        }
    }
}

/// Metadata about an entropy source.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub physics: &'static str,
    pub category: SourceCategory,
    pub platform_requirements: &'static [&'static str],
    pub entropy_rate_estimate: f64,
}

/// Trait that every entropy source must implement.
pub trait EntropySource: Send + Sync {
    /// Source metadata.
    fn info(&self) -> &SourceInfo;

    /// Check if this source can operate on the current machine.
    fn is_available(&self) -> bool;

    /// Collect raw entropy samples. Returns a Vec<u8> of up to `n_samples` bytes.
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
            last_collect_time: Duration::ZERO,
            healthy: true,
        }
    }
}
