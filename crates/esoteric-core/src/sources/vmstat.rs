//! VmstatSource — Runs macOS `vm_stat`, parses counter output, takes multiple
//! snapshots, and extracts entropy from the deltas of changing counters.

use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Delay between consecutive vm_stat snapshots.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(50);

/// Number of snapshot rounds to collect.
const NUM_ROUNDS: usize = 4;

pub struct VmstatSource {
    info: SourceInfo,
}

impl VmstatSource {
    pub fn new() -> Self {
        Self {
            info: SourceInfo {
                name: "vmstat_deltas",
                description: "Samples macOS vm_stat counters and extracts entropy from memory management deltas",
                physics: "Samples macOS vm_stat counters (page faults, pageins, pageouts, \
                    compressions, decompressions, swap activity). These track physical memory \
                    management \u{2014} each counter changes when hardware page table walks, TLB \
                    misses, or memory pressure triggers compressor/swap.",
                category: SourceCategory::System,
                platform_requirements: &["macos"],
                entropy_rate_estimate: 1000.0,
            },
        }
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
    let output = Command::new("which").arg("vm_stat").output().ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
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
    let output = Command::new(path).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
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

/// Extract LSBs from deltas, packing 8 bits per byte.
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

impl EntropySource for VmstatSource {
    fn info(&self) -> &SourceInfo {
        &self.info
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

        // XOR consecutive deltas for extra mixing
        let xor_deltas: Vec<i64> = if all_deltas.len() >= 2 {
            all_deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            all_deltas.clone()
        };

        // Extract LSBs
        let mut entropy = extract_lsbs(&xor_deltas);

        // Hash-extend if we don't have enough
        if entropy.len() < n_samples {
            let mut hasher = Sha256::new();
            hasher.update(&entropy);

            // Mix in raw deltas
            for d in &all_deltas {
                hasher.update(d.to_le_bytes());
            }

            // Timestamp
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            hasher.update(ts.as_nanos().to_le_bytes());

            let seed: [u8; 32] = hasher.finalize().into();

            // Extend via repeated hashing
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
