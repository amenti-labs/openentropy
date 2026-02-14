//! CameraNoiseSource — Camera sensor dark current noise.
//!
//! Captures a single frame from the camera via ffmpeg's avfoundation backend
//! as raw grayscale video, then extracts the lower 4 bits of each pixel value.
//! In darkness, these LSBs are dominated by shot noise and dark current.

use crate::source::{EntropySource, SourceCategory, SourceInfo};

use super::helpers::{command_exists, pack_nibbles};

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
    composite: false,
};

/// Entropy source that harvests sensor noise from camera dark frames.
pub struct CameraNoiseSource;

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
        let result = std::process::Command::new("ffmpeg")
            .args([
                "-f",
                "avfoundation",
                "-i",
                "0",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
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

        // Extract the lower 4 bits of each pixel value and pack nibbles.
        let nibbles = raw_frame.iter().map(|pixel| pixel & 0x0F);
        pack_nibbles(nibbles, n_samples)
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
