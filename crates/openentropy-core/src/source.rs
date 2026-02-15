//! Abstract entropy source trait and runtime state.
//!
//! Every entropy source implements the [`EntropySource`] trait, which provides
//! metadata via [`SourceInfo`], availability checking, and raw sample collection.

use std::time::Duration;

/// Category of entropy source based on physical mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceCategory {
    /// Thermal noise in circuits/oscillators.
    Thermal,
    /// CPU/memory timing jitter.
    Timing,
    /// OS scheduler nondeterminism.
    Scheduling,
    /// Storage/peripheral latency variance.
    IO,
    /// Inter-process/kernel communication jitter.
    IPC,
    /// CPU microarchitecture race conditions.
    Microarch,
    /// Graphics pipeline nondeterminism.
    GPU,
    /// Network timing/signal noise.
    Network,
    /// OS counters/state.
    System,
    /// Combines multiple sources.
    Composite,
    /// Signal processing entropy.
    Signal,
    /// Hardware sensor readings.
    Sensor,
}

impl std::fmt::Display for SourceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Thermal => write!(f, "thermal"),
            Self::Timing => write!(f, "timing"),
            Self::Scheduling => write!(f, "scheduling"),
            Self::IO => write!(f, "io"),
            Self::IPC => write!(f, "ipc"),
            Self::Microarch => write!(f, "microarch"),
            Self::GPU => write!(f, "gpu"),
            Self::Network => write!(f, "network"),
            Self::System => write!(f, "system"),
            Self::Composite => write!(f, "composite"),
            Self::Signal => write!(f, "signal"),
            Self::Sensor => write!(f, "sensor"),
        }
    }
}

/// Target platform for an entropy source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Works on any platform.
    Any,
    /// Requires macOS.
    MacOS,
    /// Requires Linux.
    Linux,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => write!(f, "any"),
            Self::MacOS => write!(f, "macos"),
            Self::Linux => write!(f, "linux"),
        }
    }
}

/// Hardware/software requirement for an entropy source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Requirement {
    /// GPU compute (Metal framework).
    Metal,
    /// CoreAudio (AudioUnit/AudioObject).
    AudioUnit,
    /// WiFi hardware.
    Wifi,
    /// USB subsystem.
    Usb,
    /// Camera hardware.
    Camera,
    /// Apple Silicon specific features (AMX, etc.).
    AppleSilicon,
    /// Bluetooth hardware.
    Bluetooth,
    /// IOKit framework.
    IOKit,
    /// IOSurface framework.
    IOSurface,
    /// Security framework (Keychain).
    SecurityFramework,
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metal => write!(f, "metal"),
            Self::AudioUnit => write!(f, "audio_unit"),
            Self::Wifi => write!(f, "wifi"),
            Self::Usb => write!(f, "usb"),
            Self::Camera => write!(f, "camera"),
            Self::AppleSilicon => write!(f, "apple_silicon"),
            Self::Bluetooth => write!(f, "bluetooth"),
            Self::IOKit => write!(f, "iokit"),
            Self::IOSurface => write!(f, "iosurface"),
            Self::SecurityFramework => write!(f, "security_framework"),
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
    /// Target platform.
    pub platform: Platform,
    /// Hardware/software requirements beyond the platform.
    pub requirements: &'static [Requirement],
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
