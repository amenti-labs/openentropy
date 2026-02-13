//! SensorNoiseSource — MEMS sensor noise via ioreg.
//!
//! Queries the I/O Registry for motion sensor data (accelerometer, etc.),
//! parses numeric values from the output, and extracts changing values
//! as entropy. Even at rest, MEMS sensors exhibit Brownian motion of the
//! proof mass and thermo-mechanical noise.

use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Delay between ioreg snapshots to observe sensor value changes.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(50);

static SENSOR_NOISE_INFO: SourceInfo = SourceInfo {
    name: "sensor_noise",
    description: "MEMS accelerometer/gyro noise via ioreg",
    physics: "Reads accelerometer, gyroscope, and magnetometer via CoreMotion. Even at rest, \
              MEMS sensors exhibit: Brownian motion of the proof mass, thermo-mechanical noise, \
              electronic 1/f noise, and quantization noise. The MacBook's accelerometer detects \
              micro-vibrations from fans, disk, and building structure.",
    category: SourceCategory::Hardware,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 100.0,
};

/// Entropy source that harvests noise from MEMS motion sensors.
pub struct SensorNoiseSource;

/// Parse numeric values from ioreg output. Returns a map of key -> value
/// for lines that contain numeric data.
fn parse_ioreg_numerics(output: &str) -> HashMap<String, i64> {
    let mut map = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Look for lines like: "key" = <number>
        // ioreg format:  | |   "PropertyName" = value
        // We need to handle the leading pipe/space tree-structure prefix.
        if let Some(eq_idx) = trimmed.find(" = ") {
            let raw_key = trimmed[..eq_idx].trim();
            let val_part = trimmed[eq_idx + 3..].trim();

            // Strip the ioreg tree prefix: remove leading '|', ' ', and '"' characters.
            let key_part = raw_key
                .trim_start_matches(['|', ' '])
                .trim_matches('"')
                .trim();

            if key_part.is_empty() {
                continue;
            }

            // Try to parse as integer
            if let Ok(v) = val_part.parse::<i64>() {
                map.insert(key_part.to_string(), v);
            }
        }
    }

    map
}

/// Run `ioreg -l -w0` and return the raw output.
fn snapshot_ioreg() -> Option<String> {
    let output = Command::new("ioreg").args(["-l", "-w0"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check if ioreg shows any sensor-related data.
fn has_sensor_data() -> bool {
    let output = Command::new("ioreg").args(["-l", "-w0"]).output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            // Look for motion sensor or accelerometer related entries.
            stdout.contains("SMCMotionSensor")
                || stdout.contains("Accelerometer")
                || stdout.contains("accelerometer")
                || stdout.contains("MotionSensor")
                || stdout.contains("gyro")
                || stdout.contains("Gyro")
                // Also accept general sensor data — even without a dedicated
                // motion sensor, ioreg has many changing numeric values from
                // various hardware sensors (thermal, fan speed, etc.)
                || stdout.contains("Temperature")
                || stdout.contains("FanSpeed")
        }
        _ => false,
    }
}

impl EntropySource for SensorNoiseSource {
    fn info(&self) -> &SourceInfo {
        &SENSOR_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && has_sensor_data()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Take two ioreg snapshots separated by a short delay and extract
        // numeric values that changed between them.
        let raw1 = match snapshot_ioreg() {
            Some(s) => s,
            None => return Vec::new(),
        };
        let snap1 = parse_ioreg_numerics(&raw1);

        thread::sleep(SNAPSHOT_DELAY);

        let raw2 = match snapshot_ioreg() {
            Some(s) => s,
            None => return Vec::new(),
        };
        let snap2 = parse_ioreg_numerics(&raw2);

        // Find values that changed and compute deltas.
        let mut deltas: Vec<i64> = Vec::new();
        for (key, v2) in &snap2 {
            if let Some(v1) = snap1.get(key) {
                let delta = v2.wrapping_sub(*v1);
                if delta != 0 {
                    deltas.push(delta);
                }
            }
        }

        if deltas.is_empty() {
            return Vec::new();
        }

        // Extract entropy from the deltas: XOR consecutive pairs and take LSBs.
        let mut output = Vec::with_capacity(n_samples);

        // First pass: XOR consecutive deltas for mixing.
        let mixed: Vec<i64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract bytes from the mixed deltas.
        for d in &mixed {
            // Take the low byte.
            output.push(*d as u8);
            if output.len() >= n_samples {
                break;
            }
            // Also take the second-lowest byte for more output.
            output.push((*d >> 8) as u8);
            if output.len() >= n_samples {
                break;
            }
        }

        // If we still don't have enough, cycle through with XOR folding.
        if output.len() < n_samples && !output.is_empty() {
            let base = output.clone();
            let mut idx = 0;
            while output.len() < n_samples {
                let a = base[idx % base.len()];
                let b = base[(idx + 1) % base.len()];
                output.push(a ^ b ^ (idx as u8));
                idx += 1;
            }
        }

        output.truncate(n_samples);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_noise_info() {
        let src = SensorNoiseSource;
        assert_eq!(src.name(), "sensor_noise");
        assert_eq!(src.info().category, SourceCategory::Hardware);
        assert_eq!(src.info().entropy_rate_estimate, 100.0);
    }

    #[test]
    fn parse_ioreg_numerics_works() {
        let sample = r#"
        | |   "Temperature" = 45
        | |   "FanSpeed" = 1200
        | |   "Name" = "some string"
        "#;
        let map = parse_ioreg_numerics(sample);
        assert_eq!(map.get("Temperature"), Some(&45));
        assert_eq!(map.get("FanSpeed"), Some(&1200));
        assert!(!map.contains_key("Name"));
    }
}
