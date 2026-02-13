//! AudioNoiseSource — Microphone ADC noise via ffmpeg.
//!
//! Captures a short burst of audio from the default input device using ffmpeg's
//! avfoundation backend, then extracts the lower 4 bits of each int16 sample.
//! These LSBs are dominated by Johnson-Nyquist thermal noise.

use crate::source::{EntropySource, SourceCategory, SourceInfo};

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
              impedance. This is genuine quantum-origin entropy: random electron motion \
              in a resistor at temperature T produces voltage noise proportional to \
              \u{221a}(4kT R \u{0394}f).",
    category: SourceCategory::Hardware,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 10000.0,
};

/// Entropy source that harvests thermal noise from the microphone ADC.
pub struct AudioNoiseSource;

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
                "-f",
                "avfoundation",
                "-i",
                ":0",
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
        let src = AudioNoiseSource;
        assert_eq!(src.name(), "audio_noise");
        assert_eq!(src.info().category, SourceCategory::Hardware);
        assert_eq!(src.info().entropy_rate_estimate, 10000.0);
    }
}
