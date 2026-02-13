//! Platform detection and source discovery.

use crate::source::EntropySource;
use crate::sources::all_sources;

/// Discover all entropy sources available on this machine.
pub fn detect_available_sources() -> Vec<Box<dyn EntropySource>> {
    all_sources()
        .into_iter()
        .filter(|s| s.is_available())
        .collect()
}

/// Platform information.
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        system: std::env::consts::OS.to_string(),
        machine: std::env::consts::ARCH.to_string(),
        family: std::env::consts::FAMILY.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub system: String,
    pub machine: String,
    pub family: String,
}
