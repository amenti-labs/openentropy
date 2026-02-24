//! Quantum Entropy Sources — hardware entropy with genuine quantum components
//!
//! This module provides entropy sources that tap into physical processes with
//! documented quantum origins. Each source includes an honest `quantum_fraction`
//! estimate reflecting how much of the measured signal is genuinely quantum
//! versus classical noise.
//!
//! ## Source Categories
//!
//! | Source | Physics | Quantum Fraction | Rate |
//! |--------|---------|------------------|------|
//! | `cosmic_muon` | Cosmic ray muon detection via camera | 0.95 | ~1-10/s |
//! | `radioactive_decay` | Nuclear decay via camera sensor | 0.99 | ~5-20/s |
//! | `nvme_iokit_sensors` | NVMe controller clock domain crossing | 0.30 | ~1500/s |
//! | `nvme_smart_thermal` | NVMe temperature ADC quantization noise | 0.35 | ~1200/s |
//! | `nvme_raw_device` | Raw block device reads (bypass filesystem) | 0.40 | ~2000/s |
//! | `nvme_passthrough_linux` | NVMe admin ioctl passthrough | 0.45 | ~2500/s |
//! | `multi_source_quantum` | XOR combiner for higher purity | varies | combined |
//!
//! ## Important Note
//!
//! Statistical tests (NIST, Shannon entropy, etc.) CANNOT distinguish quantum
//! from classical randomness. A PRNG scores 99%+ on the same tests!
//!
//! These sources are "quantum" based on PHYSICS arguments, not statistical tests.
//! Only Bell inequality tests can CERTIFY quantum randomness — and those require
//! specialized equipment (entangled photon pairs).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use openentropy_core::sources::quantum::{CosmicMuonSource, NvmeRawDeviceSource};
//!
//! let muon = CosmicMuonSource;
//! let entropy = muon.collect(256);
//!
//! let nvme = NvmeRawDeviceSource;
//! let entropy = nvme.collect(256);
//! ```

pub mod cosmic_muon;
pub mod radioactive;
pub mod multi_source;
pub mod nvme_iokit_sensors;
pub mod nvme_smart_thermal;
pub mod nvme_raw_device;
pub mod nvme_passthrough_linux;

pub use cosmic_muon::CosmicMuonSource;
pub use radioactive::RadioactiveDecaySource;
pub use multi_source::{MultiSourceQuantumSource, estimate_combined_purity};
pub use nvme_iokit_sensors::NvmeIokitSensorsSource;
pub use nvme_smart_thermal::NvmeSmartThermalSource;
pub use nvme_raw_device::NvmeRawDeviceSource;
pub use nvme_passthrough_linux::NvmePassthroughLinuxSource;

/// List all available quantum sources
pub fn available_quantum_sources() -> Vec<&'static str> {
    vec![
        "cosmic_muon",              // Cosmic ray muon detection
        "radioactive_decay",        // Nuclear decay detection
        "multi_source_quantum",     // XOR-combined sources
        "nvme_iokit_sensors",       // NVMe IOKit sensor clock domain crossing
        "nvme_smart_thermal",       // NVMe temperature ADC quantization noise
        "nvme_raw_device",          // Raw block device reads (bypass filesystem)
        "nvme_passthrough_linux",   // NVMe admin passthrough on Linux
    ]
}

/// Get the estimated quantum fraction for a source.
///
/// These are physics-based estimates, NOT statistical measurements.
/// No consumer-hardware test can distinguish quantum from classical noise.
pub fn quantum_fraction(source_name: &str) -> f64 {
    match source_name {
        "camera_noise" => 0.55,             // Shot noise has quantum origin, mixed with read/thermal noise
        "audio_noise" => 0.45,              // Johnson/shot-noise origin, mixed with analog front-end noise
        "cosmic_muon" => 0.95,              // Genuinely quantum (particle physics)
        "radioactive_decay" => 0.99,        // Genuinely quantum (nuclear decay)
        "multi_source_quantum" => 0.90,     // Combined purity depends on inputs
        "nvme_iokit_sensors" => 0.30,       // PLL thermal noise real but IOKit lock contention dominates
        "nvme_smart_thermal" => 0.35,       // ADC thermal noise has genuine Johnson-Nyquist component
        "nvme_raw_device" => 0.40,          // Filesystem removed, closer to NAND charge sensing physics
        "nvme_passthrough_linux" => 0.45,   // Closest to hardware from userspace, NAND timing dominant
        _ => 0.0,
    }
}
