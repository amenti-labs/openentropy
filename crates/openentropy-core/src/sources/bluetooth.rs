//! BluetoothNoiseSource — BLE RSSI scanning via system_profiler.
//!
//! Runs `system_profiler SPBluetoothDataType` with a timeout to enumerate nearby
//! Bluetooth devices, parses RSSI values, and extracts LSBs combined with timing
//! jitter. Falls back to timing-only entropy if the command hangs or times out.
//!
//! **Raw output characteristics:** Mix of RSSI LSBs and timing bytes.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

/// Path to system_profiler on macOS.
const SYSTEM_PROFILER_PATH: &str = "/usr/sbin/system_profiler";

/// Timeout for system_profiler command.
const BT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

static BLUETOOTH_NOISE_INFO: SourceInfo = SourceInfo {
    name: "bluetooth_noise",
    description: "BLE RSSI values and scanning timing jitter",
    physics: "Scans BLE advertisements via CoreBluetooth and collects RSSI values from \
              nearby devices. Each RSSI reading reflects: 2.4 GHz multipath propagation, \
              frequency hopping across 40 channels, advertising interval jitter (\u{00b1}10ms), \
              transmit power variation, and receiver thermal noise.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::Bluetooth],
    entropy_rate_estimate: 50.0,
    composite: false,
};

/// Entropy source that harvests randomness from Bluetooth RSSI and timing jitter.
pub struct BluetoothNoiseSource;

/// Parse RSSI values from system_profiler SPBluetoothDataType output.
fn parse_rssi_values(output: &str) -> Vec<i32> {
    let mut rssi_values = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if lower.contains("rssi") {
            for token in trimmed.split(&[':', '=', ' '][..]) {
                let clean = token.trim();
                if let Ok(v) = clean.parse::<i32>() {
                    rssi_values.push(v);
                }
            }
        }
    }
    rssi_values
}

/// Run system_profiler with a timeout, returning (output_option, elapsed_ns).
fn get_bluetooth_info_timed() -> (Option<String>, u64) {
    let t0 = Instant::now();

    let child = Command::new(SYSTEM_PROFILER_PATH)
        .arg("SPBluetoothDataType")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(_) => return (None, t0.elapsed().as_nanos() as u64),
    };

    let deadline = Instant::now() + BT_COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed = t0.elapsed().as_nanos() as u64;
                if !status.success() {
                    return (None, elapsed);
                }
                let stdout = child
                    .wait_with_output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
                return (stdout, elapsed);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (None, t0.elapsed().as_nanos() as u64);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return (None, t0.elapsed().as_nanos() as u64),
        }
    }
}

impl EntropySource for BluetoothNoiseSource {
    fn info(&self) -> &SourceInfo {
        &BLUETOOTH_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(SYSTEM_PROFILER_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut raw = Vec::with_capacity(n_samples);

        let time_budget = Duration::from_secs(10);
        let start = Instant::now();
        let max_scans = 50;

        for _ in 0..max_scans {
            if start.elapsed() >= time_budget || raw.len() >= n_samples {
                break;
            }

            let (bt_info, elapsed_ns) = get_bluetooth_info_timed();

            // Extract timing bytes (raw nanosecond LSBs)
            for shift in (0..64).step_by(8) {
                raw.push((elapsed_ns >> shift) as u8);
            }

            // Parse RSSI values — raw LSBs
            if let Some(info) = bt_info {
                let rssi_values = parse_rssi_values(&info);
                for rssi in &rssi_values {
                    raw.push((*rssi & 0xFF) as u8);
                }
                raw.push(info.len() as u8);
                raw.push((info.len() >> 8) as u8);
            }
        }

        raw.truncate(n_samples);
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_noise_info() {
        let src = BluetoothNoiseSource;
        assert_eq!(src.name(), "bluetooth_noise");
        assert_eq!(src.info().category, SourceCategory::Sensor);
    }

    #[test]
    fn parse_rssi_values_works() {
        let sample = r#"
            Connected: Yes
            RSSI: -45
            Some Device:
              RSSI: -72
              Name: Test
        "#;
        let values = parse_rssi_values(sample);
        assert_eq!(values, vec![-45, -72]);
    }

    #[test]
    fn parse_rssi_empty() {
        let sample = "No bluetooth data here";
        let values = parse_rssi_values(sample);
        assert!(values.is_empty());
    }

    #[test]
    fn bluetooth_composite_flag() {
        let src = BluetoothNoiseSource;
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires Bluetooth hardware
    fn bluetooth_noise_collects_bytes() {
        let src = BluetoothNoiseSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
