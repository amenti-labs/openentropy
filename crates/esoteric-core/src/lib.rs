//! esoteric-core — Your computer is a quantum noise observatory.
//!
//! Core entropy harvesting library with 30 unconventional sources
//! that exploit the physics of your computer's hardware.

pub mod conditioning;
pub mod platform;
pub mod pool;
pub mod source;
pub mod sources;

pub use conditioning::{quick_quality, quick_shannon, QualityReport};
pub use platform::{detect_available_sources, platform_info};
pub use pool::EntropyPool;
pub use source::{EntropySource, SourceCategory, SourceInfo};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
