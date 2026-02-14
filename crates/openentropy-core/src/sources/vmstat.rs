//! VmstatSource — Runs macOS `vm_stat`, parses counter output, takes multiple
//! snapshots, and extracts entropy from the deltas of changing counters.

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

use super::helpers::{extract_delta_bytes_i64, run_command};

/// Delay between consecutive vm_stat snapshots.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(50);

/// Number of snapshot rounds to collect.
const NUM_ROUNDS: usize = 4;

/// Entropy source that samples macOS `vm_stat` counters and extracts entropy
/// from memory management deltas (page faults, pageins, compressions, etc.).
///
/// No tunable parameters — the source reads all vm_stat counters and
/// automatically extracts deltas from those that change between snapshots.
pub struct VmstatSource;

static VMSTAT_INFO: SourceInfo = SourceInfo {
    name: "vmstat_deltas",
    description: "Samples macOS vm_stat counters and extracts entropy from memory management deltas",
    physics: "Samples macOS vm_stat counters (page faults, pageins, pageouts, \
              compressions, decompressions, swap activity). These track physical memory \
              management \u{2014} each counter changes when hardware page table walks, TLB \
              misses, or memory pressure triggers compressor/swap.",
    category: SourceCategory::System,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 1000.0,
    composite: false,
};

impl VmstatSource {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VmstatSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Locate the `vm_stat` binary. Checks the standard macOS path first, then PATH.
fn vm_stat_path() -> Option<String> {
    let standard = "/usr/bin/vm_stat";
    if std::path::Path::new(standard).exists() {
        return Some(standard.to_string());
    }

    // Fall back to searching PATH via `which`
    let path = run_command("which", &["vm_stat"])?;
    let path = path.trim().to_string();
    if !path.is_empty() {
        return Some(path);
    }

    None
}

/// Run `vm_stat` and parse output into a map of counter names to values.
///
/// vm_stat output looks like:
/// ```text
/// Mach Virtual Memory Statistics: (page size of 16384 bytes)
/// Pages free:                               12345.
/// Pages active:                             67890.
/// ```
///
/// We strip the trailing period and parse the integer.
fn snapshot_vmstat(path: &str) -> Option<HashMap<String, i64>> {
    let stdout = run_command(path, &[])?;
    let mut map = HashMap::new();

    for line in stdout.lines() {
        // Skip the header line
        if line.starts_with("Mach") || line.is_empty() {
            continue;
        }

        // Lines look like: "Pages active:                             67890."
        if let Some(colon_idx) = line.rfind(':') {
            let key = line[..colon_idx].trim().to_string();
            let val_str = line[colon_idx + 1..].trim().trim_end_matches('.');

            if let Ok(v) = val_str.parse::<i64>() {
                map.insert(key, v);
            }
        }
    }

    Some(map)
}

impl EntropySource for VmstatSource {
    fn info(&self) -> &SourceInfo {
        &VMSTAT_INFO
    }

    fn is_available(&self) -> bool {
        vm_stat_path().is_some()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let path = match vm_stat_path() {
            Some(p) => p,
            None => return Vec::new(),
        };

        // Take NUM_ROUNDS snapshots with delays between them
        let mut snapshots: Vec<HashMap<String, i64>> = Vec::with_capacity(NUM_ROUNDS);

        for i in 0..NUM_ROUNDS {
            if i > 0 {
                thread::sleep(SNAPSHOT_DELAY);
            }
            match snapshot_vmstat(&path) {
                Some(snap) => snapshots.push(snap),
                None => return Vec::new(),
            }
        }

        // Compute deltas between consecutive rounds
        let mut all_deltas: Vec<i64> = Vec::new();

        for pair in snapshots.windows(2) {
            let prev = &pair[0];
            let curr = &pair[1];

            for (key, curr_val) in curr {
                if let Some(prev_val) = prev.get(key) {
                    let delta = curr_val.wrapping_sub(*prev_val);
                    if delta != 0 {
                        all_deltas.push(delta);
                    }
                }
            }
        }

        extract_delta_bytes_i64(&all_deltas, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmstat_info() {
        let src = VmstatSource::new();
        assert_eq!(src.name(), "vmstat_deltas");
        assert_eq!(src.info().category, SourceCategory::System);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires vm_stat binary
    fn vmstat_collects_bytes() {
        let src = VmstatSource::new();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
