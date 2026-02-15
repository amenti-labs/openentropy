//! SysctlSource — Batch-reads kernel counters via `sysctl -a`, takes multiple
//! snapshots, finds keys that change between snapshots, extracts deltas of
//! changing values, XORs consecutive deltas, and extracts LSBs.

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

use super::helpers::{extract_delta_bytes_i64, run_command};

/// Path to the sysctl binary on macOS.
const SYSCTL_PATH: &str = "/usr/sbin/sysctl";

/// Delay between the two sysctl snapshots.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(100);

/// Entropy source that batch-reads kernel counters via `sysctl -a` and extracts
/// deltas from the ~40-60 that change within 200ms.
///
/// No tunable parameters — the source reads all available sysctl keys and
/// automatically identifies the ones that change between snapshots.
pub struct SysctlSource;

static SYSCTL_INFO: SourceInfo = SourceInfo {
    name: "sysctl_deltas",
    description: "Batch-reads ~1600 kernel counters via sysctl -a and extracts deltas from the ~40-60 that change within 200ms",
    physics: "Batch-reads ~1600 kernel counters via sysctl and extracts deltas from \
              the ~40-60 that change within 200ms. These counters track page faults, context \
              switches, TCP segments, interrupts \u{2014} each driven by independent processes. \
              The LSBs of their deltas reflect the unpredictable micro-timing of the entire \
              operating system\u{2019}s activity.",
    category: SourceCategory::System,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 5000.0,
    composite: false,
};

impl SysctlSource {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SysctlSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `sysctl -a` and parse every line that has a numeric value into a HashMap.
///
/// Handles both `key: value` (macOS) and `key = value` (Linux) formats.
fn snapshot_sysctl() -> Option<HashMap<String, i64>> {
    let stdout = run_command(SYSCTL_PATH, &["-a"])?;
    let mut map = HashMap::new();

    for line in stdout.lines() {
        // Try "key: value" first (macOS style), then "key = value" (Linux style)
        let (key, val_str) = if let Some(idx) = line.find(": ") {
            (&line[..idx], line[idx + 2..].trim())
        } else if let Some(idx) = line.find(" = ") {
            (&line[..idx], line[idx + 3..].trim())
        } else {
            continue;
        };

        // Only keep entries whose value is a plain integer
        if let Ok(v) = val_str.parse::<i64>() {
            map.insert(key.to_string(), v);
        }
    }

    Some(map)
}

impl EntropySource for SysctlSource {
    fn info(&self) -> &SourceInfo {
        &SYSCTL_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(SYSCTL_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Take two snapshots separated by a small delay
        let snap1 = match snapshot_sysctl() {
            Some(s) => s,
            None => return Vec::new(),
        };

        thread::sleep(SNAPSHOT_DELAY);

        let snap2 = match snapshot_sysctl() {
            Some(s) => s,
            None => return Vec::new(),
        };

        // Find keys that changed between the two snapshots and compute deltas
        let mut deltas: Vec<i64> = Vec::new();
        for (key, v2) in &snap2 {
            if let Some(v1) = snap1.get(key) {
                let delta = v2.wrapping_sub(*v1);
                if delta != 0 {
                    deltas.push(delta);
                }
            }
        }

        extract_delta_bytes_i64(&deltas, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysctl_info() {
        let src = SysctlSource::new();
        assert_eq!(src.name(), "sysctl_deltas");
        assert_eq!(src.info().category, SourceCategory::System);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires sysctl binary
    fn sysctl_collects_bytes() {
        let src = SysctlSource::new();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
