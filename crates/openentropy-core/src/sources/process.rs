//! ProcessSource — Snapshots the process table via `ps` and combines it with
//! getpid() timing jitter for entropy.
//!
//! **Raw output characteristics:** Mix of timing LSBs and process table byte
//! deltas. Shannon entropy ~3-5 bits/byte. The timing jitter component has
//! higher entropy density than the process table bytes.

use std::time::Instant;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

use super::helpers::run_command_raw;

/// Number of getpid() calls to measure for timing jitter.
const JITTER_ROUNDS: usize = 256;

/// Entropy source that snapshots the process table via `ps` and combines it
/// with `getpid()` timing jitter.
///
/// No tunable parameters — the source reads the full process table and
/// automatically extracts entropy from byte-level changes.
pub struct ProcessSource;

static PROCESS_INFO: SourceInfo = SourceInfo {
    name: "process_table",
    description: "Process table snapshots combined with getpid() timing jitter",
    physics: "Snapshots the process table (PIDs, CPU usage, memory) and extracts \
              entropy from the constantly-changing state. New PIDs are allocated \
              semi-randomly, CPU percentages fluctuate with scheduling decisions, and \
              resident memory sizes shift with page reclamation.",
    category: SourceCategory::System,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 400.0,
    composite: false,
};

impl ProcessSource {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProcessSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect timing jitter from repeated getpid() syscalls.
/// Returns raw LSBs of nanosecond timing deltas.
fn collect_getpid_jitter(n_bytes: usize) -> Vec<u8> {
    let rounds = JITTER_ROUNDS.max(n_bytes * 2);
    let mut timings: Vec<u64> = Vec::with_capacity(rounds);

    for _ in 0..rounds {
        let start = Instant::now();
        // SAFETY: getpid() is always safe — it's a simple read-only syscall.
        unsafe {
            libc::getpid();
        }
        let elapsed = start.elapsed().as_nanos() as u64;
        timings.push(elapsed);
    }

    // Extract LSBs of timing deltas
    let mut raw = Vec::with_capacity(n_bytes);
    for pair in timings.windows(2) {
        let delta = pair[1].wrapping_sub(pair[0]);
        raw.push(delta as u8);
        if raw.len() >= n_bytes {
            break;
        }
    }
    raw
}

/// Run `ps -eo pid,pcpu,rss` and return its raw stdout bytes.
fn snapshot_process_table() -> Option<Vec<u8>> {
    run_command_raw("ps", &["-eo", "pid,pcpu,rss"])
}

impl EntropySource for ProcessSource {
    fn info(&self) -> &SourceInfo {
        &PROCESS_INFO
    }

    fn is_available(&self) -> bool {
        super::helpers::command_exists("ps")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut entropy = Vec::with_capacity(n_samples);

        // 1. Extract raw bytes from process table snapshot
        if let Some(stdout) = snapshot_process_table() {
            // XOR consecutive byte pairs for mixing
            for pair in stdout.chunks(2) {
                if pair.len() == 2 {
                    entropy.push(pair[0] ^ pair[1]);
                }
                if entropy.len() >= n_samples {
                    break;
                }
            }
        }

        // 2. Fill remaining with getpid() timing jitter
        if entropy.len() < n_samples {
            let jitter = collect_getpid_jitter(n_samples - entropy.len());
            entropy.extend_from_slice(&jitter);
        }

        entropy.truncate(n_samples);
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_info() {
        let src = ProcessSource::new();
        assert_eq!(src.name(), "process_table");
        assert_eq!(src.info().category, SourceCategory::System);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // Requires ps command
    fn process_collects_bytes() {
        let src = ProcessSource::new();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
