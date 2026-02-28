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
    /// Signal processing entropy.
    Signal,
    /// Hardware sensor readings.
    Sensor,
    /// True quantum random number generators.
    Quantum,
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
            Self::Signal => write!(f, "signal"),
            Self::Sensor => write!(f, "sensor"),
            Self::Quantum => write!(f, "quantum"),
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
    /// Raw block device access (/dev/rdiskN, /dev/nvmeXnY).
    RawBlockDevice,
    /// QCicada QRNG hardware (USB serial).
    QCicada,
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
            Self::RawBlockDevice => write!(f, "raw_block_device"),
            Self::QCicada => write!(f, "qcicada"),
        }
    }
}

impl Requirement {
    /// Emoji icon for hardware requirements.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::QCicada => "🔮",
            Self::Camera => "📷",
            Self::AudioUnit => "🎤",
            Self::Metal => "🎮",
            Self::Wifi => "📶",
            Self::Bluetooth => "📡",
            Self::Usb => "🔌",
            Self::RawBlockDevice => "💾",
            _ => "", // IOKit, IOSurface, SecurityFramework, AppleSilicon — no icon
        }
    }

    /// Human-readable hardware label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::QCicada => "QCicada QRNG (USB)",
            Self::Camera => "Camera",
            Self::AudioUnit => "Microphone (CoreAudio)",
            Self::Metal => "GPU (Metal)",
            Self::Wifi => "WiFi adapter",
            Self::Bluetooth => "Bluetooth",
            Self::Usb => "USB subsystem",
            Self::RawBlockDevice => "Raw block device",
            Self::AppleSilicon => "Apple Silicon",
            Self::IOKit => "IOKit",
            Self::IOSurface => "IOSurface",
            Self::SecurityFramework => "Security Framework",
        }
    }

    /// Parse a requirement from its `Display` name (inverse of `to_string()`).
    pub fn from_display_name(name: &str) -> Option<Self> {
        match name {
            "metal" => Some(Self::Metal),
            "audio_unit" => Some(Self::AudioUnit),
            "wifi" => Some(Self::Wifi),
            "usb" => Some(Self::Usb),
            "camera" => Some(Self::Camera),
            "apple_silicon" => Some(Self::AppleSilicon),
            "bluetooth" => Some(Self::Bluetooth),
            "iokit" => Some(Self::IOKit),
            "iosurface" => Some(Self::IOSurface),
            "security_framework" => Some(Self::SecurityFramework),
            "raw_block_device" => Some(Self::RawBlockDevice),
            "qcicada" => Some(Self::QCicada),
            _ => None,
        }
    }

    /// Look up the icon for a requirement by its `Display` name.
    ///
    /// This is the canonical mapping used by CLI/TUI code that only has the
    /// serialised string form (e.g. from [`SourceInfoSnapshot`]).
    pub fn icon_for_display_name(name: &str) -> &'static str {
        Self::from_display_name(name).map_or("", |r| r.icon())
    }

    /// Look up the label for a requirement by its `Display` name.
    ///
    /// Returns the human-readable label if the name is recognised, or a
    /// generic `"Unknown"` for unrecognised names.
    pub fn label_for_display_name(name: &str) -> &'static str {
        Self::from_display_name(name).map_or("Unknown", |r| r.label())
    }
}

/// Returns the first non-empty icon from a requirements list.
pub fn best_icon(requirements: &[Requirement]) -> &'static str {
    requirements
        .iter()
        .map(Requirement::icon)
        .find(|icon| !icon.is_empty())
        .unwrap_or("")
}

/// Returns the first non-empty icon from a list of requirement display names.
pub fn best_icon_from_names(names: &[String]) -> &'static str {
    names
        .iter()
        .map(|n| Requirement::icon_for_display_name(n))
        .find(|icon| !icon.is_empty())
        .unwrap_or("")
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
    /// Whether this source collects in <2 seconds and is safe for real-time use.
    pub is_fast: bool,
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

    /// Optional runtime configuration. Returns Err if unsupported.
    fn set_config(&self, _key: &str, _value: &str) -> Result<(), String> {
        Err("source does not support runtime configuration".into())
    }

    /// List configurable keys and their current values.
    fn config_options(&self) -> Vec<(&'static str, String)> {
        vec![]
    }
}

/// Runtime state for a registered source in the pool.
pub struct SourceState {
    pub source: std::sync::Arc<dyn EntropySource>,
    pub total_bytes: u64,
    pub failures: u64,
    pub last_entropy: f64,
    pub last_min_entropy: f64,
    pub last_autocorrelation: f64,
    pub last_collect_time: Duration,
    pub healthy: bool,
}

impl SourceState {
    pub fn new(source: Box<dyn EntropySource>) -> Self {
        Self {
            source: std::sync::Arc::from(source),
            total_bytes: 0,
            failures: 0,
            last_entropy: 0.0,
            last_min_entropy: 0.0,
            last_autocorrelation: 0.0,
            last_collect_time: Duration::ZERO,
            healthy: true,
        }
    }
}
