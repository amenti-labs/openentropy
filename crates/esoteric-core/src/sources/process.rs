//! ProcessSource — Snapshots the process table via `ps`, hashes the output
//! with SHA-256, and combines it with getpid() timing jitter for extra entropy.

use std::process::Command;
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Number of getpid() calls to measure for timing jitter.
const JITTER_ROUNDS: usize = 256;

pub struct ProcessSource {
    info: SourceInfo,
}

impl ProcessSource {
    pub fn new() -> Self {
        Self {
            info: SourceInfo {
                name: "process_table",
                description: "Snapshots the process table (PIDs, CPU, memory) and hashes it with SHA-256",
                physics: "Snapshots the process table (PIDs, CPU usage, memory) and extracts \
                    entropy from the constantly-changing state. New PIDs are allocated \
                    semi-randomly, CPU percentages fluctuate with scheduling decisions, and \
                    resident memory sizes shift with page reclamation.",
                category: SourceCategory::System,
                platform_requirements: &[],
                entropy_rate_estimate: 400.0,
            },
        }
    }
}

impl Default for ProcessSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect timing jitter from repeated getpid() syscalls.
///
/// Measures the nanosecond-resolution duration of each getpid() call. The LSBs
/// of these timings carry entropy from cache state, TLB pressure, interrupt
/// timing, and scheduler preemption.
fn collect_getpid_jitter() -> Vec<u8> {
    let mut timings: Vec<u64> = Vec::with_capacity(JITTER_ROUNDS);

    for _ in 0..JITTER_ROUNDS {
        let start = Instant::now();
        // SAFETY: getpid() is always safe to call; it has no preconditions
        // and simply returns the process ID.
        unsafe {
            libc::getpid();
        }
        let elapsed = start.elapsed().as_nanos() as u64;
        timings.push(elapsed);
    }

    // Hash all the timings together
    let mut hasher = Sha256::new();
    for t in &timings {
        hasher.update(t.to_le_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    digest.to_vec()
}

/// Run `ps -eo pid,pcpu,rss` and return its raw stdout bytes.
fn snapshot_process_table() -> Option<Vec<u8>> {
    let output = Command::new("ps")
        .args(["-eo", "pid,pcpu,rss"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(output.stdout)
}

impl EntropySource for ProcessSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        // `ps` is available on virtually all Unix-like systems.
        // Attempt a quick check.
        Command::new("ps")
            .arg("--version")
            .output()
            .is_ok()
            || Command::new("ps")
                .output()
                .is_ok()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // 1. Hash the process table snapshot
        let ps_hash = match snapshot_process_table() {
            Some(stdout) => {
                let mut h = Sha256::new();
                h.update(&stdout);

                // Add a timestamp so identical process tables still yield unique output
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                h.update(ts.as_nanos().to_le_bytes());

                let digest: [u8; 32] = h.finalize().into();
                digest
            }
            None => [0u8; 32],
        };

        // 2. Collect getpid() timing jitter
        let jitter = collect_getpid_jitter();

        // 3. Combine the two by XOR-ing and then hashing together
        let mut combined = Sha256::new();
        combined.update(ps_hash);
        combined.update(&jitter);

        // Add a second timestamp to capture the time delta from the collection itself
        let ts2 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        combined.update(ts2.as_nanos().to_le_bytes());

        let seed: [u8; 32] = combined.finalize().into();

        // 4. Extend output if more than 32 bytes are needed
        let mut entropy = seed.to_vec();

        if entropy.len() < n_samples {
            let mut state = seed;
            while entropy.len() < n_samples {
                let mut h = Sha256::new();
                h.update(state);
                h.update((entropy.len() as u64).to_le_bytes());

                // Take another process table snapshot for fresh material
                if let Some(stdout) = snapshot_process_table() {
                    h.update(&stdout);
                }

                // And more jitter
                let jitter = collect_getpid_jitter();
                h.update(&jitter);

                state = h.finalize().into();
                entropy.extend_from_slice(&state);
            }
        }

        entropy.truncate(n_samples);
        entropy
    }
}
