//! IORegistryEntropySource -- Mines the macOS IORegistry for all fluctuating
//! hardware counters.  Takes multiple snapshots of `ioreg -l -w0`, identifies
//! numeric keys that change between snapshots, computes deltas, XORs
//! consecutive deltas, and extracts LSBs.

use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Path to the ioreg binary on macOS.
const IOREG_PATH: &str = "/usr/sbin/ioreg";

/// Delay between ioreg snapshots.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(80);

/// Number of snapshots to collect (3-5 range).
const NUM_SNAPSHOTS: usize = 4;

static IOREGISTRY_INFO: SourceInfo = SourceInfo {
    name: "ioregistry",
    description: "Mines macOS IORegistry for fluctuating hardware counters and extracts LSBs of their deltas",
    physics: "Mines the macOS IORegistry for all fluctuating hardware counters \u{2014} GPU \
              utilization, NVMe SMART counters, memory controller stats, Neural Engine \
              buffer allocations, DART IOMMU activity, Mach port counts, and display \
              vsync counters. Each counter is driven by independent hardware subsystems. \
              The LSBs of their deltas capture silicon-level activity across the entire SoC.",
    category: SourceCategory::System,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 1000.0,
};

/// Entropy source that mines the macOS IORegistry for hardware counter deltas.
pub struct IORegistryEntropySource;

/// Run `ioreg -l -w0` and parse lines matching `"key" = number` patterns into
/// a HashMap of key -> value.
fn snapshot_ioreg() -> Option<HashMap<String, i64>> {
    let output = Command::new(IOREG_PATH).args(["-l", "-w0"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        let trimmed = trimmed.trim_start_matches('|').trim_start_matches('+').trim();

        // Extract all "key"=number patterns from the line (covers both
        // top-level `"key" = 123` and nested dict `"key"=123` formats).
        extract_quoted_key_numbers(trimmed, &mut map);
    }

    Some(map)
}

/// Scan a string for all `"key"=number` or `"key" = number` patterns and
/// insert them into the map. This handles both top-level ioreg properties
/// and values nested inside `{...}` dictionaries on the same line.
fn extract_quoted_key_numbers(s: &str, map: &mut HashMap<String, i64>) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Find next opening quote
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }

        // Extract key between quotes
        let key_start = i + 1;
        let mut key_end = key_start;
        while key_end < len && bytes[key_end] != b'"' {
            key_end += 1;
        }
        if key_end >= len {
            break;
        }

        let key = &s[key_start..key_end];
        let mut j = key_end + 1;

        // Skip optional whitespace then expect '='
        while j < len && bytes[j] == b' ' {
            j += 1;
        }
        if j >= len || bytes[j] != b'=' {
            i = key_end + 1;
            continue;
        }
        j += 1; // skip '='

        // Skip optional whitespace after '='
        while j < len && bytes[j] == b' ' {
            j += 1;
        }

        // Try to parse a decimal integer (possibly negative)
        let num_start = j;
        if j < len && bytes[j] == b'-' {
            j += 1;
        }
        while j < len && bytes[j].is_ascii_digit() {
            j += 1;
        }

        if j > num_start
            && (j >= len || !bytes[j].is_ascii_alphanumeric())
            && let Ok(v) = s[num_start..j].parse::<i64>()
        {
            map.insert(key.to_string(), v);
        }

        i = j.max(key_end + 1);
    }
}

impl EntropySource for IORegistryEntropySource {
    fn info(&self) -> &SourceInfo {
        &IOREGISTRY_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(IOREG_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Take NUM_SNAPSHOTS snapshots with delays between them.
        let mut snapshots: Vec<HashMap<String, i64>> = Vec::with_capacity(NUM_SNAPSHOTS);

        for i in 0..NUM_SNAPSHOTS {
            if i > 0 {
                thread::sleep(SNAPSHOT_DELAY);
            }
            match snapshot_ioreg() {
                Some(snap) => snapshots.push(snap),
                None => return Vec::new(),
            }
        }

        if snapshots.len() < 2 {
            return Vec::new();
        }

        // Find keys present in ALL snapshots.
        let common_keys: Vec<String> = {
            let first = &snapshots[0];
            first
                .keys()
                .filter(|k| snapshots.iter().all(|snap| snap.contains_key(*k)))
                .cloned()
                .collect()
        };

        // For each common key, extract deltas across consecutive snapshots.
        let mut all_deltas: Vec<i64> = Vec::new();

        for key in &common_keys {
            for pair in snapshots.windows(2) {
                let v1 = pair[0][key];
                let v2 = pair[1][key];
                let delta = v2.wrapping_sub(v1);
                if delta != 0 {
                    all_deltas.push(delta);
                }
            }
        }

        // XOR consecutive deltas for extra mixing.
        let xor_deltas: Vec<i64> = if all_deltas.len() >= 2 {
            all_deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            all_deltas.clone()
        };

        // Extract raw bytes from all deltas (not just XOR'd)
        let mut entropy = Vec::with_capacity(n_samples);

        // First: raw bytes from all non-zero deltas
        for d in &all_deltas {
            let bytes = d.to_le_bytes();
            for &b in &bytes {
                entropy.push(b);
            }
            if entropy.len() >= n_samples {
                break;
            }
        }

        // Then: XOR'd delta bytes for more mixing
        if entropy.len() < n_samples {
            for d in &xor_deltas {
                let bytes = d.to_le_bytes();
                for &b in &bytes {
                    entropy.push(b);
                }
                if entropy.len() >= n_samples {
                    break;
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
    use super::super::helpers::extract_lsbs_i64 as extract_lsbs;

    #[test]
    fn ioregistry_info() {
        let src = IORegistryEntropySource;
        assert_eq!(src.name(), "ioregistry");
        assert_eq!(src.info().category, SourceCategory::System);
        assert!((src.info().entropy_rate_estimate - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_lsbs_basic() {
        let deltas = vec![1i64, 2, 3, 4, 5, 6, 7, 8];
        let bytes = extract_lsbs(&deltas);
        // Bits: 1,0,1,0,1,0,1,0 -> 0xAA
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0xAA);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Run with: cargo test -- --ignored
    fn ioregistry_collects_bytes() {
        let src = IORegistryEntropySource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
