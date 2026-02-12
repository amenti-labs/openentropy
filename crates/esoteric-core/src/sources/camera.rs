//! CameraNoiseSource — Camera sensor dark current noise.
//!
//! Captures a single frame from the camera via ffmpeg's avfoundation backend
//! as raw grayscale video, then extracts the lower 4 bits of each pixel value.
//! In darkness, these LSBs are dominated by shot noise and dark current.

use std::process::Command;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

static CAMERA_NOISE_INFO: SourceInfo = SourceInfo {
    name: "camera_noise",
    description: "Camera sensor dark current and shot noise via ffmpeg",
    physics: "Captures frames from the camera sensor in darkness. The sensor's photodiodes \
              generate dark current from thermal electron-hole pair generation in silicon \
              \u{2014} a quantum process. Read noise from the amplifier adds further randomness. \
              The LSBs of pixel values in dark frames are dominated by shot noise \
              (Poisson-distributed photon counting).",
    category: SourceCategory::Hardware,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 50000.0,
};

/// Entropy source that harvests sensor noise from camera dark frames.
pub struct CameraNoiseSource;

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

impl EntropySource for CameraNoiseSource {
    fn info(&self) -> &SourceInfo {
        &CAMERA_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && command_exists("ffmpeg")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Capture one frame of raw grayscale video from the default camera.
        // ffmpeg -f avfoundation -i "0" -frames:v 1 -f rawvideo -pix_fmt gray pipe:1
        let result = Command::new("ffmpeg")
            .args([
                "-f", "avfoundation",
                "-i", "0",
                "-frames:v", "1",
                "-f", "rawvideo",
                "-pix_fmt", "gray",
                "pipe:1",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let raw_frame = match result {
            Ok(output) if output.status.success() => output.stdout,
            _ => return Vec::new(),
        };

        // Extract the lower 4 bits of each pixel value.
        // Pack two 4-bit nibbles into one byte for efficient output.
        let mut output = Vec::with_capacity(n_samples);
        let mut nibble_buf: u8 = 0;
        let mut nibble_count: u8 = 0;

        for &pixel in &raw_frame {
            // Lower 4 bits contain noise-dominated LSBs.
            let noise_nibble = pixel & 0x0F;

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
    fn camera_noise_info() {
        let src = CameraNoiseSource;
        assert_eq!(src.name(), "camera_noise");
        assert_eq!(src.info().category, SourceCategory::Hardware);
        assert_eq!(src.info().entropy_rate_estimate, 50000.0);
    }
}
