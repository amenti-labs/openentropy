//! WiFi RSSI entropy source.
//!
//! Reads WiFi signal strength (RSSI) and noise floor values on macOS.
//! Fluctuations in RSSI arise from multipath fading, constructive/destructive
//! interference, Rayleigh fading, atmospheric absorption, and thermal noise in
//! the radio receiver.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

const MEASUREMENT_DELAY: Duration = Duration::from_millis(10);
const SAMPLES_PER_COLLECT: usize = 8;

/// Entropy source that harvests WiFi RSSI and noise floor fluctuations.
///
/// On macOS, it attempts multiple methods to read the current RSSI:
///
/// 1. `networksetup -listallhardwareports` to discover the Wi-Fi device name,
///    then `ipconfig getsummary <device>` to read RSSI/noise.
/// 2. Fallback: the `airport -I` command from Apple's private framework.
///
/// The raw entropy is a combination of RSSI LSBs, successive RSSI deltas,
/// noise floor LSBs, and measurement timing jitter.
pub struct WiFiRSSISource {
    info: SourceInfo,
}

impl WiFiRSSISource {
    pub fn new() -> Self {
        Self {
            info: SourceInfo {
                name: "wifi_rssi",
                description: "WiFi signal strength (RSSI) and noise floor fluctuations",
                physics: "Reads WiFi signal strength (RSSI) and noise floor via CoreWLAN \
                          framework. RSSI fluctuates due to: multipath fading (reflections \
                          off walls/objects), constructive/destructive interference at \
                          2.4/5/6 GHz, Rayleigh fading from moving objects, atmospheric \
                          absorption, and thermal noise in the radio receiver's LNA.",
                category: SourceCategory::Hardware,
                platform_requirements: &["macos", "wifi"],
                entropy_rate_estimate: 30.0,
            },
        }
    }
}

impl Default for WiFiRSSISource {
    fn default() -> Self {
        Self::new()
    }
}

/// A single RSSI/noise measurement.
#[derive(Debug, Clone, Copy)]
struct WifiMeasurement {
    rssi: i32,
    noise: i32,
    /// Nanoseconds taken to perform the measurement.
    timing_nanos: u128,
}

/// Discover the Wi-Fi hardware device name (e.g. "en0") by parsing
/// `networksetup -listallhardwareports`.
fn discover_wifi_device() -> Option<String> {
    let output = Command::new("/usr/sbin/networksetup")
        .arg("-listallhardwareports")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut found_wifi = false;

    for line in text.lines() {
        if line.contains("Wi-Fi") || line.contains("AirPort") {
            found_wifi = true;
            continue;
        }
        if found_wifi && line.starts_with("Device:") {
            let device = line.trim_start_matches("Device:").trim();
            if !device.is_empty() {
                return Some(device.to_string());
            }
        }
        // Reset if we hit the next hardware port block without finding a device
        if found_wifi && line.starts_with("Hardware Port:") {
            found_wifi = false;
        }
    }
    None
}

/// Try to read RSSI/noise via `ipconfig getsummary <device>`.
fn read_via_ipconfig(device: &str) -> Option<(i32, i32)> {
    let output = Command::new("/usr/sbin/ipconfig")
        .args(["getsummary", device])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let rssi = parse_field_value(&text, "RSSI")?;
    let noise = parse_field_value(&text, "Noise").unwrap_or(rssi - 30); // estimate if absent
    Some((rssi, noise))
}

/// Try to read RSSI/noise via the `airport -I` command.
fn read_via_airport() -> Option<(i32, i32)> {
    let output = Command::new(
        "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport",
    )
    .arg("-I")
    .output()
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let rssi = parse_field_value(&text, "agrCtlRSSI")?;
    let noise = parse_field_value(&text, "agrCtlNoise").unwrap_or(rssi - 30);
    Some((rssi, noise))
}

/// Parse a line of the form `  key: value` or `key : value` and return the
/// integer value.  Handles negative numbers.
fn parse_field_value(text: &str, field: &str) -> Option<i32> {
    for line in text.lines() {
        let trimmed = line.trim();
        // Match "FIELD : VALUE" or "FIELD: VALUE"
        if let Some(rest) = trimmed.strip_prefix(field) {
            let rest = rest.trim_start();
            if let Some(val_str) = rest.strip_prefix(':') {
                let val_str = val_str.trim();
                if let Ok(v) = val_str.parse::<i32>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Take a single RSSI/noise measurement using the best available method.
fn measure_once(device: &Option<String>) -> Option<WifiMeasurement> {
    let start = Instant::now();

    let (rssi, noise) = if let Some(dev) = device {
        read_via_ipconfig(dev).or_else(read_via_airport)?
    } else {
        read_via_airport()?
    };

    let timing_nanos = start.elapsed().as_nanos();
    Some(WifiMeasurement {
        rssi,
        noise,
        timing_nanos,
    })
}

impl EntropySource for WiFiRSSISource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        let device = discover_wifi_device();
        measure_once(&device).is_some()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut entropy = Vec::with_capacity(n_samples);
        let device = discover_wifi_device();

        let mut measurements = Vec::with_capacity(SAMPLES_PER_COLLECT);

        // Collect a burst of RSSI measurements with small delays between them.
        // We keep collecting bursts until we have enough entropy bytes.
        while entropy.len() < n_samples {
            measurements.clear();

            for _ in 0..SAMPLES_PER_COLLECT {
                if let Some(m) = measure_once(&device) {
                    measurements.push(m);
                }
                thread::sleep(MEASUREMENT_DELAY);
            }

            if measurements.is_empty() {
                // No measurements possible; bail out.
                break;
            }

            // Extract entropy from the burst:
            for i in 0..measurements.len() {
                if entropy.len() >= n_samples {
                    break;
                }
                let m = &measurements[i];

                // 1. RSSI least-significant byte
                entropy.push(m.rssi as u8);
                if entropy.len() >= n_samples {
                    break;
                }

                // 2. Noise floor least-significant byte
                entropy.push(m.noise as u8);
                if entropy.len() >= n_samples {
                    break;
                }

                // 3. Measurement timing jitter (LSB of nanoseconds)
                let timing_bytes = m.timing_nanos.to_le_bytes();
                entropy.push(timing_bytes[0]);
                if entropy.len() >= n_samples {
                    break;
                }

                // 4. RSSI delta from previous measurement (inter-sample jitter)
                if i > 0 {
                    let prev = &measurements[i - 1];
                    let rssi_delta = (m.rssi - prev.rssi) as u8;
                    entropy.push(rssi_delta);
                    if entropy.len() >= n_samples {
                        break;
                    }

                    // 5. Timing delta
                    let timing_delta = if m.timing_nanos > prev.timing_nanos {
                        m.timing_nanos - prev.timing_nanos
                    } else {
                        prev.timing_nanos - m.timing_nanos
                    };
                    entropy.push(timing_delta.to_le_bytes()[0]);
                    if entropy.len() >= n_samples {
                        break;
                    }

                    // 6. XOR of RSSI and noise (cross-domain mixing)
                    entropy.push((m.rssi ^ m.noise) as u8);
                }
            }
        }

        entropy.truncate(n_samples);
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rssi_from_airport_output() {
        let sample = "\
             agrCtlRSSI: -62\n\
             agrCtlNoise: -90\n\
             state: running\n";
        assert_eq!(parse_field_value(sample, "agrCtlRSSI"), Some(-62));
        assert_eq!(parse_field_value(sample, "agrCtlNoise"), Some(-90));
    }

    #[test]
    fn parse_rssi_from_ipconfig_output() {
        let sample = "\
             SSID : MyNetwork\n\
             RSSI : -55\n\
             Noise : -88\n";
        assert_eq!(parse_field_value(sample, "RSSI"), Some(-55));
        assert_eq!(parse_field_value(sample, "Noise"), Some(-88));
    }

    #[test]
    fn parse_field_missing() {
        assert_eq!(parse_field_value("nothing here", "RSSI"), None);
    }

    #[test]
    fn source_info() {
        let src = WiFiRSSISource::new();
        assert_eq!(src.info().name, "wifi_rssi");
        assert_eq!(src.info().category, SourceCategory::Hardware);
        assert!((src.info().entropy_rate_estimate - 30.0).abs() < f64::EPSILON);
        assert_eq!(src.info().platform_requirements, &["macos", "wifi"]);
    }
}
