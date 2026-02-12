//! BluetoothNoiseSource — BLE RSSI scanning via system_profiler.
//!
//! Runs `system_profiler SPBluetoothDataType` with a timeout to enumerate nearby
//! Bluetooth devices, parses RSSI values, and extracts LSBs combined with timing
//! jitter. Falls back to timing-only entropy if the command hangs or times out.

use std::process::Command;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Path to system_profiler on macOS.
const SYSTEM_PROFILER_PATH: &str = "/usr/sbin/system_profiler";

/// Timeout for system_profiler command (Python uses 10s, we use 5s).
const BT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

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
/// Always returns the elapsed time even if the command fails/times out.
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

    // Wait with timeout — kill if it hangs
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

/// SHA-256 condition raw bytes to improve entropy density.
/// Uses a chained hash approach: each block includes the previous hash
/// as state so cycling through raw data still produces unique output.
fn condition_bytes(raw: &[u8], n_output: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(n_output);
    let mut state = [0u8; 32];
    let mut offset = 0;
    let mut counter: u64 = 0;
    while output.len() < n_output {
        let end = (offset + 64).min(raw.len());
        let chunk = &raw[offset..end];
        let mut h = Sha256::new();
        h.update(state);
        h.update(chunk);
        h.update(counter.to_le_bytes());
        state = h.finalize().into();
        output.extend_from_slice(&state);
        offset += 64;
        counter += 1;
        if offset >= raw.len() {
            offset = 0;
        }
    }
    output.truncate(n_output);
    output
}

impl EntropySource for BluetoothNoiseSource {
    fn info(&self) -> &SourceInfo {
        &BLUETOOTH_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(SYSTEM_PROFILER_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut raw = Vec::with_capacity(n_samples * 2);

        // Use a time budget of 10 seconds total for all scans.
        // Each scan can take up to BT_COMMAND_TIMEOUT (5s) if it hangs,
        // but typically completes in ~300ms. We keep scanning until we
        // have enough raw data or run out of time.
        let time_budget = Duration::from_secs(10);
        let start = Instant::now();
        let max_scans = 50;

        for _ in 0..max_scans {
            if start.elapsed() >= time_budget {
                break;
            }

            let (bt_info, elapsed_ns) = get_bluetooth_info_timed();

            // Extract full 8 bytes of timing entropy (even from timeouts).
            for shift in (0..64).step_by(8) {
                raw.push((elapsed_ns >> shift) as u8);
            }

            // Parse RSSI values if we got output.
            if let Some(info) = bt_info {
                let rssi_values = parse_rssi_values(&info);
                for rssi in &rssi_values {
                    raw.push((*rssi & 0xFF) as u8);
                }
                // Also add the raw output length as entropy
                raw.push(info.len() as u8);
                raw.push((info.len() >> 8) as u8);
            }

            if raw.len() >= n_samples * 2 {
                break;
            }
        }

        // SHA-256 condition the raw bytes for better entropy density.
        condition_bytes(&raw, n_samples)
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
