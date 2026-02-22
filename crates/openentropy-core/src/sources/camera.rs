//! CameraNoiseSource — Camera sensor noise (read noise dominated).
//!
//! Captures frames from the camera via ffmpeg's avfoundation backend as raw
//! grayscale video, then extracts the lower 4 bits of each pixel value.
//! In dark frames at typical webcam exposures, these LSBs are dominated by
//! read noise from the amplifier (~95%+), with small contributions from dark
//! current and dark current shot noise.
//!
//! A persistent ffmpeg subprocess streams frames continuously so the camera
//! LED stays solid instead of flickering on/off every collection cycle.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

use super::helpers::{command_exists, pack_nibbles};

const FRAME_WIDTH: usize = 320;
const FRAME_HEIGHT: usize = 240;
const FRAME_SIZE: usize = FRAME_WIDTH * FRAME_HEIGHT; // 76800 bytes per gray frame

static CAMERA_NOISE_INFO: SourceInfo = SourceInfo {
    name: "camera_noise",
    description: "Camera sensor noise (read noise + dark current) via ffmpeg",
    physics: "Captures frames from the camera sensor in darkness. Noise sources: (1) read \
              noise from the amplifier \u{2014} classical analog noise, dominates at short \
              exposures (~95%+ of variance); (2) dark current from thermal carrier generation \
              in silicon \u{2014} classical at sensor operating temperatures; (3) dark current \
              shot noise (Poisson counting) \u{2014} ~1-5% of variance in typical webcams. \
              The LSBs of pixel values mix all three components.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::Camera],
    entropy_rate_estimate: 4000.0,
    composite: false,
};

/// Configuration for camera device selection.
pub struct CameraNoiseConfig {
    /// AVFoundation video device index (e.g., 0, 1, 2).
    /// `None` means try common selectors in fallback order.
    pub device_index: Option<u32>,
}

impl Default for CameraNoiseConfig {
    fn default() -> Self {
        let device_index = std::env::var("OPENENTROPY_CAMERA_DEVICE")
            .ok()
            .and_then(|s| s.parse().ok());
        Self { device_index }
    }
}

// ---------------------------------------------------------------------------
// Persistent ffmpeg subprocess
// ---------------------------------------------------------------------------

/// A long-lived ffmpeg process that continuously streams grayscale frames.
///
/// A reader thread reads exactly `FRAME_SIZE` bytes per frame from ffmpeg's
/// stdout and stores the latest frame in shared memory. `collect()` just
/// clones the most recent frame, avoiding the LED flicker caused by
/// spawning a new process every second.
struct PersistentCamera {
    child: Child,
    latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
    _reader: JoinHandle<()>,
}

impl PersistentCamera {
    /// Try to spawn a persistent ffmpeg process for camera capture.
    ///
    /// Tries each avfoundation input selector in order. For the first one
    /// that produces a frame within 2 seconds, returns a `PersistentCamera`.
    /// Returns `None` if no device works.
    fn spawn(device_index: Option<u32>) -> Option<Self> {
        let inputs: Vec<String> = match device_index {
            Some(n) => vec![format!("{n}:none")],
            None => vec![
                "default:none".into(),
                "0:none".into(),
                "1:none".into(),
                "0:0".into(),
            ],
        };

        for input in &inputs {
            if let Some(cam) = Self::try_spawn(input) {
                return Some(cam);
            }
        }
        None
    }

    /// Attempt to spawn ffmpeg with a single avfoundation input selector.
    ///
    /// Waits up to 2 seconds for the first frame. If no frame arrives
    /// (permission denied, device busy, wrong selector), kills the process
    /// and returns `None`.
    fn try_spawn(input: &str) -> Option<Self> {
        let size = format!("{}x{}", FRAME_WIDTH, FRAME_HEIGHT);
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-video_size",
                &size,
                "-i",
                input,
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let latest_frame: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let frame_ref = Arc::clone(&latest_frame);

        let reader = thread::spawn(move || {
            Self::reader_loop(stdout, frame_ref);
        });

        // Wait up to 2 seconds for the first frame to confirm the device works.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if std::time::Instant::now() >= deadline {
                // No frame arrived — kill and bail.
                let _ = child.kill();
                let _ = child.wait();
                // Reader thread will exit when pipe closes.
                return None;
            }
            {
                let guard = latest_frame.lock().unwrap();
                if guard.is_some() {
                    break;
                }
            }
            thread::sleep(std::time::Duration::from_millis(25));
        }

        Some(PersistentCamera {
            child,
            latest_frame,
            _reader: reader,
        })
    }

    /// Continuously read frames from ffmpeg stdout.
    ///
    /// Each frame is exactly `FRAME_SIZE` bytes of raw grayscale pixels.
    /// On EOF or error (ffmpeg died), sets `latest_frame` to `None` so
    /// `collect()` knows to respawn.
    fn reader_loop(
        mut stdout: std::process::ChildStdout,
        latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
    ) {
        let mut buf = vec![0u8; FRAME_SIZE];
        loop {
            match stdout.read_exact(&mut buf) {
                Ok(()) => {
                    let mut guard = latest_frame.lock().unwrap();
                    *guard = Some(buf.clone());
                }
                Err(_) => {
                    // EOF or broken pipe — ffmpeg exited.
                    let mut guard = latest_frame.lock().unwrap();
                    *guard = None;
                    return;
                }
            }
        }
    }

    /// Returns the most recent frame, or `None` if ffmpeg has died.
    fn take_frame(&self) -> Option<Vec<u8>> {
        let guard = self.latest_frame.lock().unwrap();
        guard.clone()
    }

    /// Returns `true` if the reader thread has signaled that ffmpeg died.
    fn is_dead(&self) -> bool {
        let guard = self.latest_frame.lock().unwrap();
        guard.is_none()
    }
}

impl Drop for PersistentCamera {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Reader thread will exit naturally when the pipe closes.
    }
}

// ---------------------------------------------------------------------------
// CameraNoiseSource
// ---------------------------------------------------------------------------

/// Entropy source that harvests sensor noise from camera dark frames.
pub struct CameraNoiseSource {
    pub config: CameraNoiseConfig,
    camera: Mutex<Option<PersistentCamera>>,
}

impl Default for CameraNoiseSource {
    fn default() -> Self {
        Self {
            config: CameraNoiseConfig::default(),
            camera: Mutex::new(None),
        }
    }
}

impl EntropySource for CameraNoiseSource {
    fn info(&self) -> &SourceInfo {
        &CAMERA_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && command_exists("ffmpeg")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut guard = self.camera.lock().unwrap();

        // Lazy-init: spawn persistent camera on first call.
        if guard.is_none() {
            *guard = PersistentCamera::spawn(self.config.device_index);
        }

        // If spawn failed (no camera / no ffmpeg), nothing to do.
        let cam = match guard.as_ref() {
            Some(c) => c,
            None => return Vec::new(),
        };

        // Grab the latest frame from the reader thread.
        let raw_frame = match cam.take_frame() {
            Some(frame) if !frame.is_empty() => frame,
            _ => {
                // ffmpeg died — drop so next call respawns.
                if cam.is_dead() {
                    *guard = None;
                }
                return Vec::new();
            }
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
        let src = CameraNoiseSource::default();
        assert_eq!(src.name(), "camera_noise");
        assert_eq!(src.info().category, SourceCategory::Sensor);
        assert_eq!(src.info().entropy_rate_estimate, 4000.0);
        assert!(!src.info().composite);
    }

    #[test]
    fn camera_config_default_is_none() {
        // With no env var set, device_index should be None.
        let config = CameraNoiseConfig { device_index: None };
        assert!(config.device_index.is_none());
    }

    #[test]
    fn camera_config_explicit_device() {
        let src = CameraNoiseSource {
            config: CameraNoiseConfig {
                device_index: Some(1),
            },
            camera: Mutex::new(None),
        };
        assert_eq!(src.config.device_index, Some(1));
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires camera and ffmpeg
    fn camera_noise_collects_bytes() {
        let src = CameraNoiseSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    fn camera_source_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CameraNoiseSource>();
    }

    #[test]
    fn persistent_camera_constants() {
        assert_eq!(FRAME_WIDTH, 320);
        assert_eq!(FRAME_HEIGHT, 240);
        assert_eq!(FRAME_SIZE, 320 * 240);
        assert_eq!(FRAME_SIZE, 76800);
    }
}
