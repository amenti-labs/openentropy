//! BluetoothNoiseSource — BLE RSSI scanning via system_profiler.
//!
//! Runs `system_profiler SPBluetoothDataType` to enumerate nearby Bluetooth
//! devices, parses RSSI values, and extracts LSBs combined with timing jitter.

use std::process::Command;
use std::time::Instant;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Path to system_profiler on macOS.
const SYSTEM_PROFILER_PATH: &str = "/usr/sbin/system_profiler";

static BLUETOOTH_NOISE_INFO: SourceInfo = SourceInfo {
    name: "bluetooth_noise",
    description: "BLE RSSI values and scanning timing jitter",
    physics: "Scans BLE advertisements via CoreBluetooth and collects RSSI values from \
              nearby devices. Each RSSI reading reflects: 2.4 GHz multipath propagation, \
              frequency hopping across 40 channels, advertising interval jitter (\u{00b1}10ms), \
              transmit power variation, and receiver thermal noise.",
    category: SourceCategory::Hardware,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 50.0,
};

/// Entropy source that harvests randomness from Bluetooth RSSI and timing jitter.
pub struct BluetoothNoiseSource;

/// Parse RSSI values from system_profiler SPBluetoothDataType output.
/// Looks for lines containing "RSSI" with numeric values.
fn parse_rssi_values(output: &str) -> Vec<i32> {
    let mut rssi_values = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Look for lines like:
        //   RSSI: -45
        //   "RSSI" = -67
        //   rssi: -72
        let lower = trimmed.to_lowercase();
        if lower.contains("rssi") {
            // Extract numeric value from the line.
            // Try splitting on ':', '=', or just finding a negative number.
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

/// Run system_profiler and return the Bluetooth data.
fn get_bluetooth_info() -> Option<String> {
    let output = Command::new(SYSTEM_PROFILER_PATH)
        .arg("SPBluetoothDataType")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

impl EntropySource for BluetoothNoiseSource {
    fn info(&self) -> &SourceInfo {
        &BLUETOOTH_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(SYSTEM_PROFILER_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(n_samples);

        // Perform multiple scans to accumulate entropy.
        // Each scan gives us RSSI values + timing jitter.
        let scans_needed = (n_samples / 4).max(2); // at least 2 scans

        for _ in 0..scans_needed {
            let t0 = Instant::now();

            let bt_info = match get_bluetooth_info() {
                Some(info) => info,
                None => continue,
            };

            let scan_time_ns = t0.elapsed().as_nanos() as u64;

            // Parse RSSI values from the scan.
            let rssi_values = parse_rssi_values(&bt_info);

            // Extract entropy from RSSI LSBs.
            for rssi in &rssi_values {
                // The LSBs of RSSI reflect multipath fading randomness.
                let rssi_lsb = (*rssi & 0xFF) as u8;
                output.push(rssi_lsb);

                if output.len() >= n_samples {
                    break;
                }
            }

            // Also extract entropy from the scan timing.
            // The time to complete the Bluetooth scan has jitter from
            // radio scheduling, channel hopping, and kernel scheduling.
            output.push(scan_time_ns as u8);
            output.push((scan_time_ns >> 8) as u8);

            if output.len() >= n_samples {
                break;
            }

            // If no RSSI values were found, use timing-only entropy.
            if rssi_values.is_empty() {
                // XOR timing bytes with themselves shifted for extra mixing.
                let mixed = (scan_time_ns as u8) ^ ((scan_time_ns >> 16) as u8);
                output.push(mixed);
            }
        }

        // If we still don't have enough bytes (e.g., no BT devices nearby),
        // extend with timing jitter from repeated scans.
        while output.len() < n_samples {
            let t0 = Instant::now();
            let _ = get_bluetooth_info();
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            output.push(elapsed_ns as u8);
            output.push((elapsed_ns >> 8) as u8);
        }

        output.truncate(n_samples);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_noise_info() {
        let src = BluetoothNoiseSource;
        assert_eq!(src.name(), "bluetooth_noise");
        assert_eq!(src.info().category, SourceCategory::Hardware);
        assert_eq!(src.info().entropy_rate_estimate, 50.0);
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
}
