//! QCicadaSource — Crypta Labs QCicada USB QRNG.
//!
//! Reads true quantum random bytes from a QCicada USB device via the `qcicada`
//! crate. The hardware generates entropy from photonic shot noise (LED +
//! photodiode), providing full 8 bits/byte of quantum randomness per NIST
//! SP 800-90B.
//!
//! **On-device conditioning**: The QCicada handles its own conditioning internally.
//! No additional conditioning should be applied by the pool when using this source
//! alone. The device supports three modes:
//! - `raw` — health-tested noise after on-device filtering (default)
//! - `sha256` — NIST SP 800-90B SHA-256 conditioning on-device
//! - `samples` — raw ADC readings from the photodiode, no processing
//!
//! Configuration is via environment variables:
//! - `QCICADA_MODE` — post-processing mode: `raw`, `sha256`, or `samples` (default: `raw`)
//! - `QCICADA_POST_PROCESS` — legacy alias for `QCICADA_MODE`
//! - `QCICADA_PORT` — serial port path (auto-detected if unset)
//! - `QCICADA_TIMEOUT` — connection timeout in ms (default: 5000)

use std::sync::{Mutex, OnceLock};

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

/// Thread-safe override for QCicada mode, set by the CLI before source discovery.
/// Checked by `QCicadaConfig::default()` before falling back to env vars.
/// This avoids the need for `unsafe { std::env::set_var(...) }`.
pub static QCICADA_CLI_MODE: OnceLock<String> = OnceLock::new();

static QCICADA_INFO: SourceInfo = SourceInfo {
    name: "qcicada",
    description: "Crypta Labs QCicada USB QRNG \u{2014} quantum shot noise",
    physics: "Photonic shot noise from an LED/photodiode pair inside the QCicada USB device. \
              Photon emission and detection are inherently quantum processes governed by Poisson \
              statistics. The device digitises photodiode current fluctuations to produce true \
              quantum random numbers at full entropy (8 bits/byte) per NIST SP 800-90B.",
    category: SourceCategory::Quantum,
    platform: Platform::Any,
    requirements: &[Requirement::QCicada],
    entropy_rate_estimate: 8.0,
    composite: false,
    is_fast: false, // USB serial init can take up to timeout_ms (default 5s)
};

/// Configuration for the QCicada QRNG device.
pub struct QCicadaConfig {
    /// Serial port path (e.g. `/dev/tty.usbmodem*`). `None` means auto-detect.
    pub port: Option<String>,
    /// Connection timeout in milliseconds.
    pub timeout_ms: u64,
    /// Post-processing mode: `"raw"`, `"sha256"`, or `"samples"`.
    pub post_process: String,
}

impl Default for QCicadaConfig {
    fn default() -> Self {
        let port = std::env::var("QCICADA_PORT").ok();
        let timeout_ms = std::env::var("QCICADA_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5000);
        let post_process = QCICADA_CLI_MODE
            .get()
            .cloned()
            .or_else(|| std::env::var("QCICADA_MODE").ok())
            .or_else(|| std::env::var("QCICADA_POST_PROCESS").ok())
            .unwrap_or_else(|| "raw".into());
        Self {
            port,
            timeout_ms,
            post_process,
        }
    }
}

/// Entropy source backed by the QCicada USB QRNG hardware.
pub struct QCicadaSource {
    pub config: QCicadaConfig,
    device: Mutex<Option<qcicada::QCicada>>,
    available: Mutex<Option<bool>>,
    /// Runtime-mutable mode, initialised from config.post_process.
    mode: Mutex<String>,
}

impl Default for QCicadaSource {
    fn default() -> Self {
        let config = QCicadaConfig::default();
        let mode = config.post_process.clone();
        Self {
            config,
            device: Mutex::new(None),
            available: Mutex::new(None),
            mode: Mutex::new(mode),
        }
    }
}

impl QCicadaSource {
    /// Parse the current runtime mode into the crate enum.
    fn post_process_mode(&self) -> qcicada::PostProcess {
        let mode = self.mode.lock().unwrap_or_else(|e| e.into_inner());
        match mode.as_str() {
            "sha256" => qcicada::PostProcess::Sha256,
            "samples" => qcicada::PostProcess::RawSamples,
            _ => qcicada::PostProcess::RawNoise,
        }
    }

    /// Try to open the QCicada device with the current config and set post-processing mode.
    fn try_open(&self) -> Option<qcicada::QCicada> {
        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);

        let port_str = self.config.port.as_deref();
        let mut qrng = match qcicada::QCicada::open(port_str, Some(timeout)) {
            Ok(q) => q,
            Err(_) => return None,
        };

        // Set the desired post-processing mode on the device.
        let _ = qrng.set_postprocess(self.post_process_mode());

        Some(qrng)
    }
}

impl EntropySource for QCicadaSource {
    fn info(&self) -> &SourceInfo {
        &QCICADA_INFO
    }

    fn is_available(&self) -> bool {
        let mut cached = self.available.lock().unwrap_or_else(|e| e.into_inner());
        // Positive result is stable (device was found, assume it stays).
        // Negative result is re-checked each call (device may be hot-plugged).
        if *cached == Some(true) {
            return true;
        }
        let avail = !qcicada::discover_devices().is_empty();
        if avail {
            *cached = Some(true);
        }
        avail
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut guard = self.device.lock().unwrap_or_else(|e| e.into_inner());

        // Lazy-init: open device on first call.
        if guard.is_none() {
            *guard = self.try_open();
            if guard.is_none() {
                // USB serial devices need settle time after handle release.
                std::thread::sleep(std::time::Duration::from_millis(500));
                *guard = self.try_open();
            }
        }

        let qrng = match guard.as_mut() {
            Some(q) => q,
            None => return Vec::new(),
        };

        // Clamp to u16::MAX (protocol limit per call).
        let n = n_samples.min(u16::MAX as usize) as u16;
        match qrng.random(n) {
            Ok(bytes) => bytes,
            Err(_) => {
                // Device error — reconnect and retry once.
                *guard = None;
                std::thread::sleep(std::time::Duration::from_millis(300));
                *guard = self.try_open();
                match guard.as_mut() {
                    Some(q) => q.random(n).unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    }

    fn set_config(&self, key: &str, value: &str) -> Result<(), String> {
        if key != "mode" {
            return Err(format!("unknown config key: {key}"));
        }
        match value {
            "raw" | "sha256" | "samples" => {}
            _ => {
                return Err(format!(
                    "invalid mode: {value} (expected raw|sha256|samples)"
                ));
            }
        }
        *self.mode.lock().unwrap_or_else(|e| e.into_inner()) = value.to_string();

        // Drop the current device handle so the next collect() reopens with the
        // new mode. USB serial devices are more reliable when reconnected after
        // a mode switch than when the mode is changed on a live connection.
        *self.device.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    fn config_options(&self) -> Vec<(&'static str, String)> {
        vec![(
            "mode",
            self.mode.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = QCicadaSource::default();
        assert_eq!(src.name(), "qcicada");
        assert_eq!(src.info().category, SourceCategory::Quantum);
        assert_eq!(src.info().platform, Platform::Any);
        assert_eq!(src.info().entropy_rate_estimate, 8.0);
        assert!(!src.info().composite);
        assert!(!src.info().is_fast);
        assert_eq!(src.info().requirements, &[Requirement::QCicada]);
    }

    #[test]
    fn config_default() {
        let config = QCicadaConfig {
            port: None,
            timeout_ms: 5000,
            post_process: "raw".into(),
        };
        assert!(config.port.is_none());
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.post_process, "raw");
    }

    #[test]
    fn config_explicit() {
        let config = QCicadaConfig {
            port: Some("/dev/ttyUSB0".into()),
            timeout_ms: 3000,
            post_process: "sha256".into(),
        };
        let mode = config.post_process.clone();
        let src = QCicadaSource {
            config,
            device: Mutex::new(None),
            available: Mutex::new(None),
            mode: Mutex::new(mode),
        };
        assert_eq!(src.config.port.as_deref(), Some("/dev/ttyUSB0"));
        assert_eq!(src.config.timeout_ms, 3000);
        assert_eq!(src.config.post_process, "sha256");
    }

    #[test]
    fn post_process_mode_parsing() {
        let src = |mode: &str| {
            let config = QCicadaConfig {
                port: None,
                timeout_ms: 5000,
                post_process: mode.into(),
            };
            QCicadaSource {
                config,
                device: Mutex::new(None),
                available: Mutex::new(None),
                mode: Mutex::new(mode.into()),
            }
        };
        assert!(matches!(
            src("sha256").post_process_mode(),
            qcicada::PostProcess::Sha256
        ));
        assert!(matches!(
            src("samples").post_process_mode(),
            qcicada::PostProcess::RawSamples
        ));
        assert!(matches!(
            src("raw").post_process_mode(),
            qcicada::PostProcess::RawNoise
        ));
        assert!(matches!(
            src("anything").post_process_mode(),
            qcicada::PostProcess::RawNoise
        ));
    }

    #[test]
    fn set_config_mode() {
        let src = QCicadaSource::default();
        assert!(src.set_config("mode", "sha256").is_ok());
        assert_eq!(src.config_options(), vec![("mode", "sha256".into())]);
        assert!(src.set_config("mode", "samples").is_ok());
        assert_eq!(src.config_options(), vec![("mode", "samples".into())]);
        assert!(src.set_config("mode", "raw").is_ok());
        assert_eq!(src.config_options(), vec![("mode", "raw".into())]);
    }

    #[test]
    fn set_config_invalid() {
        let src = QCicadaSource::default();
        assert!(src.set_config("mode", "invalid").is_err());
        assert!(src.set_config("unknown_key", "raw").is_err());
    }

    #[test]
    fn source_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<QCicadaSource>();
    }

    #[test]
    #[ignore] // Requires QCicada hardware connected via USB
    fn collects_quantum_bytes() {
        let src = QCicadaSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
