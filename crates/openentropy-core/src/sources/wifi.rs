//! WiFi RSSI entropy source.
//!
//! Reads WiFi signal strength (RSSI) and noise floor values on macOS.
//! Fluctuations in RSSI arise from multipath fading, constructive/destructive
//! interference, Rayleigh fading, atmospheric absorption, and thermal noise in
//! the radio receiver.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};

const MEASUREMENT_DELAY: Duration = Duration::from_millis(10);
const SAMPLES_PER_COLLECT: usize = 8;

/// Timeout for external WiFi commands.
const WIFI_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

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
///
/// No tunable parameters — automatically discovers the Wi-Fi device and
/// selects the best available measurement method.
pub struct WiFiRSSISource;

static WIFI_RSSI_INFO: SourceInfo = SourceInfo {
    name: "wifi_rssi",
    description: "WiFi signal strength (RSSI) and noise floor fluctuations",
    physics: "Reads WiFi signal strength (RSSI) and noise floor via CoreWLAN \
              framework. RSSI fluctuates due to: multipath fading (reflections \
              off walls/objects), constructive/destructive interference at \
              2.4/5/6 GHz, Rayleigh fading from moving objects, atmospheric \
              absorption, and thermal noise in the radio receiver's LNA.",
    category: SourceCategory::Network,
    platform: Platform::MacOS,
    requirements: &[Requirement::Wifi],
    entropy_rate_estimate: 30.0,
    composite: false,
};

impl WiFiRSSISource {
    pub fn new() -> Self {
        Self
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

/// Run a command with a timeout. Returns (stdout_option, elapsed_ns).
/// Always returns elapsed time even if the command fails or times out.
fn run_command_timed(cmd: &str, args: &[&str], timeout: Duration) -> (Option<String>, u64) {
    let t0 = Instant::now();

    let child = Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(_) => return (None, t0.elapsed().as_nanos() as u64),
    };

    let deadline = Instant::now() + timeout;
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

/// Discover the Wi-Fi hardware device name (e.g. "en0") by parsing
/// `networksetup -listallhardwareports`.
fn discover_wifi_device() -> Option<String> {
    let (output, _) = run_command_timed(
        "/usr/sbin/networksetup",
        &["-listallhardwareports"],
        WIFI_COMMAND_TIMEOUT,
    );

    let text = output?;
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
/// Returns ((rssi, noise), elapsed_ns) on success, or just elapsed_ns on failure.
fn read_via_ipconfig(device: &str) -> (Option<(i32, i32)>, u64) {
    let (output, elapsed) = run_command_timed(
        "/usr/sbin/ipconfig",
        &["getsummary", device],
        WIFI_COMMAND_TIMEOUT,
    );

    let text = match output {
        Some(t) => t,
        None => return (None, elapsed),
    };

    let rssi = match parse_field_value(&text, "RSSI") {
        Some(v) => v,
        None => return (None, elapsed),
    };
    let noise = parse_field_value(&text, "Noise").unwrap_or(rssi - 30);
    (Some((rssi, noise)), elapsed)
}

/// Try to read RSSI/noise via the `airport -I` command.
/// Returns ((rssi, noise), elapsed_ns) on success, or just elapsed_ns on failure.
fn read_via_airport() -> (Option<(i32, i32)>, u64) {
    let (output, elapsed) = run_command_timed(
        "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport",
        &["-I"],
        WIFI_COMMAND_TIMEOUT,
    );

    let text = match output {
        Some(t) => t,
        None => return (None, elapsed),
    };

    let rssi = match parse_field_value(&text, "agrCtlRSSI") {
        Some(v) => v,
        None => return (None, elapsed),
    };
    let noise = parse_field_value(&text, "agrCtlNoise").unwrap_or(rssi - 30);
    (Some((rssi, noise)), elapsed)
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
/// Always returns timing even if RSSI reading fails (for timing entropy).
fn measure_once(device: &Option<String>) -> WifiMeasurement {
    let start = Instant::now();

    let result = if let Some(dev) = device {
        let (ipconfig_result, elapsed1) = read_via_ipconfig(dev);
        if let Some(vals) = ipconfig_result {
            Some((vals, elapsed1))
        } else {
            let (airport_result, elapsed2) = read_via_airport();
            airport_result.map(|vals| (vals, elapsed1 + elapsed2))
        }
    } else {
        let (airport_result, elapsed) = read_via_airport();
        airport_result.map(|vals| (vals, elapsed))
    };

    let timing_nanos = start.elapsed().as_nanos();
    match result {
        Some(((rssi, noise), _)) => WifiMeasurement {
            rssi,
            noise,
            timing_nanos,
        },
        None => WifiMeasurement {
            rssi: 0,
            noise: 0,
            timing_nanos,
        },
    }
}

impl EntropySource for WiFiRSSISource {
    fn info(&self) -> &SourceInfo {
        &WIFI_RSSI_INFO
    }

    fn is_available(&self) -> bool {
        if !cfg!(target_os = "macos") {
            return false;
        }
        let device = discover_wifi_device();
        let m = measure_once(&device);
        // Available if we got a real RSSI (not the zero fallback)
        m.rssi != 0 || m.noise != 0
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut raw = Vec::with_capacity(n_samples * 4);
        let device = discover_wifi_device();

        let mut measurements = Vec::with_capacity(SAMPLES_PER_COLLECT);

        // Collect bursts of measurements. Always extract timing entropy
        // even if RSSI reading fails (timeout timing is still entropic).
        // Cap at 4 bursts since each measurement uses commands with timeouts.
        let max_bursts = 4;
        for _ in 0..max_bursts {
            measurements.clear();

            for _ in 0..SAMPLES_PER_COLLECT {
                let m = measure_once(&device);
                measurements.push(m);
                thread::sleep(MEASUREMENT_DELAY);
            }

            for i in 0..measurements.len() {
                let m = &measurements[i];

                // Always extract timing entropy (works even on timeout)
                let t_bytes = m.timing_nanos.to_le_bytes();
                raw.push(t_bytes[0]);
                raw.push(t_bytes[1]);
                raw.push(t_bytes[2]);
                raw.push(t_bytes[3]);

                // Extract RSSI/noise if we got real values
                if m.rssi != 0 || m.noise != 0 {
                    raw.push(m.rssi as u8);
                    raw.push(m.noise as u8);
                }

                // Deltas from previous measurement
                if i > 0 {
                    let prev = &measurements[i - 1];
                    raw.push((m.rssi.wrapping_sub(prev.rssi)) as u8);
                    let timing_delta = m.timing_nanos.abs_diff(prev.timing_nanos);
                    raw.push(timing_delta.to_le_bytes()[0]);
                    raw.push((m.rssi ^ m.noise) as u8);
                }
            }

            if raw.len() >= n_samples * 2 {
                break;
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
        assert_eq!(src.info().category, SourceCategory::Network);
        assert!((src.info().entropy_rate_estimate - 30.0).abs() < f64::EPSILON);
        assert_eq!(src.info().platform, Platform::MacOS);
        assert_eq!(src.info().requirements, &[Requirement::Wifi]);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires WiFi hardware
    fn wifi_rssi_collects_bytes() {
        let src = WiFiRSSISource::new();
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
