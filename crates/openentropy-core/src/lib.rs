//! # openentropy-core
//!
//! **Your computer is a quantum noise observatory.**
//!
//! `openentropy-core` is the core entropy harvesting library that extracts randomness
//! from 30 unconventional hardware sources — clock jitter, DRAM row buffer timing,
//! CPU speculative execution, Bluetooth RSSI, NVMe latency, and more.
//!
//! ## Quick Start
//!
//! ```no_run
//! use openentropy_core::EntropyPool;
//!
//! // Auto-detect all available sources and create a pool
//! let pool = EntropyPool::auto();
//!
//! // Get conditioned random bytes
//! let random_bytes = pool.get_random_bytes(256);
//! assert_eq!(random_bytes.len(), 256);
//!
//! // Check pool health
//! let health = pool.health_report();
//! println!("{}/{} sources healthy", health.healthy, health.total);
//! ```
//!
//! ## Architecture
//!
//! Sources → Pool (XOR combine) → Output
//!
//! Two output modes:
//! - **Conditioned** (default): SHA-256 final conditioning with OS entropy mixed in
//! - **Raw** (`get_raw_bytes`): XOR-combined only — no hashing, no whitening
//!
//! Raw mode preserves the actual hardware noise signal for researchers studying
//! device entropy characteristics. Most QRNG APIs (ANU, Outshift) run DRBG
//! post-processing that destroys the raw quantum/hardware signal. We don't.
//!
//! Every source implements the [`EntropySource`] trait. The [`EntropyPool`]
//! collects from all registered sources and XOR-combines independent streams.

pub mod conditioning;
pub mod platform;
pub mod pool;
pub mod source;
pub mod sources;

pub use conditioning::{ConditioningMode, QualityReport, condition, quick_quality, quick_shannon};
pub use platform::{detect_available_sources, platform_info};
pub use pool::{EntropyPool, HealthReport, SourceHealth, SourceInfoSnapshot};
pub use source::{EntropySource, SourceCategory, SourceInfo};

/// Library version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
