//! IORegistryEntropySource -- Mines the macOS IORegistry for all fluctuating
//! hardware counters.  Takes multiple snapshots of `ioreg -l -w0`, identifies
//! numeric keys that change between snapshots, computes deltas, XORs
//! consecutive deltas, and extracts LSBs.

use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use sha2::{Digest, Sha256};

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
        // Look for patterns like:  "SomeKey" = 12345
        // Trim leading whitespace, then parse the key/value.
        let trimmed = line.trim();

        // Must start with a quoted key name
        if !trimmed.starts_with('"') {
            continue;
        }

        // Find the closing quote for the key
        let rest = &trimmed[1..];
        let close_quote = match rest.find('"') {
            Some(idx) => idx,
            None => continue,
        };

        let key = &rest[..close_quote];
        let after_key = rest[close_quote + 1..].trim();

        // Must be followed by " = "
        if !after_key.starts_with('=') {
            continue;
        }

        let val_str = after_key[1..].trim();

        // Try to parse as an integer (decimal)
        if let Ok(v) = val_str.parse::<i64>() {
            map.insert(key.to_string(), v);
        }
    }

    Some(map)
}

/// Extract LSBs from a slice of i64 deltas, packing 8 bits per byte.
fn extract_lsbs(deltas: &[i64]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::with_capacity(deltas.len());
    for d in deltas {
        bits.push((d & 1) as u8);
    }

    let mut bytes = Vec::with_capacity(bits.len() / 8 + 1);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
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

        // Extract LSBs.
        let mut entropy = extract_lsbs(&xor_deltas);

        // If insufficient entropy, hash-extend with SHA-256.
        if entropy.len() < n_samples {
            let mut hasher = Sha256::new();
            hasher.update(&entropy);

            // Mix in the raw deltas as additional material.
            for d in &all_deltas {
                hasher.update(d.to_le_bytes());
            }

            // Add a timestamp for extra uniqueness.
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            hasher.update(ts.as_nanos().to_le_bytes());

            let seed: [u8; 32] = hasher.finalize().into();

            // Repeatedly hash to extend the output.
            let mut state = seed;
            while entropy.len() < n_samples {
                let mut h = Sha256::new();
                h.update(state);
                h.update((entropy.len() as u64).to_le_bytes());
                state = h.finalize().into();
                entropy.extend_from_slice(&state);
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
    fn ioregistry_collects_bytes() {
        let src = IORegistryEntropySource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
