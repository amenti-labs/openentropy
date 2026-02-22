//! AudioNoiseSource — Microphone ADC noise via ffmpeg.
//!
//! Captures a short burst of audio from the default input device using ffmpeg's
//! avfoundation backend, then extracts the lower 4 bits of each int16 sample.
//! These LSBs are dominated by Johnson-Nyquist thermal noise.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

use super::helpers::{command_exists, pack_nibbles};

/// Duration of audio capture in seconds.
const CAPTURE_DURATION: &str = "0.1";

/// Sample rate for audio capture.
const SAMPLE_RATE: &str = "44100";

static AUDIO_NOISE_INFO: SourceInfo = SourceInfo {
    name: "audio_noise",
    description: "Microphone ADC thermal noise (Johnson-Nyquist) via ffmpeg",
    physics: "Records from the microphone ADC with no signal present. The LSBs capture \
              Johnson-Nyquist noise \u{2014} thermal agitation of electrons in the input \
              impedance. At audio frequencies (up to ~44 kHz), this noise is entirely \
              classical: hf \u{226a} kT by a factor of ~10^8 at room temperature. Laptop \
              audio codecs use CMOS input stages where channel thermal noise and 1/f \
              flicker noise dominate; shot noise is negligible. \
              Voltage noise \u{221d} \u{221a}(4kT R \u{0394}f).",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::AudioUnit],
    entropy_rate_estimate: 10000.0,
    composite: false,
};

/// Configuration for audio device selection.
pub struct AudioNoiseConfig {
    /// AVFoundation audio device index (e.g., 0, 1, 2).
    /// `None` means use default (`:0`).
    pub device_index: Option<u32>,
}

impl Default for AudioNoiseConfig {
    fn default() -> Self {
        let device_index = std::env::var("OPENENTROPY_AUDIO_DEVICE")
            .ok()
            .and_then(|s| s.parse().ok());
        Self { device_index }
    }
}

/// Entropy source that harvests thermal noise from the microphone ADC.
#[derive(Default)]
pub struct AudioNoiseSource {
    pub config: AudioNoiseConfig,
}

impl EntropySource for AudioNoiseSource {
    fn info(&self) -> &SourceInfo {
        &AUDIO_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && command_exists("ffmpeg")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Capture raw signed 16-bit PCM audio from the default input device.
        // ffmpeg -f avfoundation -i ":0" -t 0.1 -f s16le -ar 44100 -ac 1 pipe:1
        let result = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "avfoundation",
                "-i",
                &format!(":{}", self.config.device_index.unwrap_or(0)),
                "-t",
                CAPTURE_DURATION,
                "-f",
                "s16le",
                "-ar",
                SAMPLE_RATE,
                "-ac",
                "1",
                "pipe:1",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let raw_audio = match result {
            Ok(output) if output.status.success() => output.stdout,
            _ => return Vec::new(),
        };

        // Each sample is 2 bytes (signed 16-bit little-endian).
        // Extract the lower 4 bits of each sample as entropy.
        let nibbles = raw_audio.chunks_exact(2).map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            (sample & 0x0F) as u8
        });

        pack_nibbles(nibbles, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_noise_info() {
        let src = AudioNoiseSource::default();
        assert_eq!(src.name(), "audio_noise");
        assert_eq!(src.info().category, SourceCategory::Sensor);
        assert_eq!(src.info().entropy_rate_estimate, 10000.0);
        assert!(!src.info().composite);
    }

    #[test]
    fn audio_config_default_is_none() {
        let config = AudioNoiseConfig { device_index: None };
        assert!(config.device_index.is_none());
    }

    #[test]
    fn audio_config_explicit_device() {
        let src = AudioNoiseSource {
            config: AudioNoiseConfig {
                device_index: Some(2),
            },
        };
        assert_eq!(src.config.device_index, Some(2));
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires microphone and ffmpeg
    fn audio_noise_collects_bytes() {
        let src = AudioNoiseSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
