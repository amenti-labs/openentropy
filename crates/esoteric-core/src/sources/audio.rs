//! AudioNoiseSource — Microphone ADC noise via ffmpeg.
//!
//! Captures a short burst of audio from the default input device using ffmpeg's
//! avfoundation backend, then extracts the lower 4 bits of each int16 sample.
//! These LSBs are dominated by Johnson-Nyquist thermal noise.

use std::process::Command;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

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

/// Check if a command exists by running `which`.
fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
        let result = Command::new("ffmpeg")
            .args([
                "-f", "avfoundation",
                "-i", ":0",
                "-t", CAPTURE_DURATION,
                "-f", "s16le",
                "-ar", SAMPLE_RATE,
                "-ac", "1",
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
        let mut output = Vec::with_capacity(n_samples);

        // Process pairs of bytes as int16 samples.
        let samples = raw_audio.chunks_exact(2);
        let mut nibble_buf: u8 = 0;
        let mut nibble_count: u8 = 0;

        for sample_bytes in samples {
            let sample = i16::from_le_bytes([sample_bytes[0], sample_bytes[1]]);

            // Extract lower 4 bits (the noise floor).
            let noise_nibble = (sample & 0x0F) as u8;

            // Pack two 4-bit nibbles into one byte.
            if nibble_count == 0 {
                nibble_buf = noise_nibble << 4;
                nibble_count = 1;
            } else {
                nibble_buf |= noise_nibble;
                output.push(nibble_buf);
                nibble_count = 0;

                if output.len() >= n_samples {
                    break;
                }
            }
        }

        // If we have an odd nibble left and still need more, include it.
        if nibble_count == 1 && output.len() < n_samples {
            output.push(nibble_buf);
        }

        output.truncate(n_samples);
        output
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
