//! Filesystem journal commit timing — full storage stack entropy.
//!
//! APFS uses copy-on-write with a journal. Each fsync crosses:
//!   CPU → filesystem → NVMe controller → NAND flash → back
//!
//! Each layer adds independent noise:
//! - Checksum computation (CPU pipeline state)
//! - NVMe command queuing and arbitration
//! - Flash cell program timing (temperature-dependent)
//! - B-tree update (memory allocation nondeterminism)
//! - Barrier flush (controller firmware scheduling)
//!
//! Different from disk_io because this specifically measures the full
//! journal commit path, not just raw block reads.
//!

use std::io::Write;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static FSYNC_JOURNAL_INFO: SourceInfo = SourceInfo {
    name: "fsync_journal",
    description: "Filesystem journal commit timing from full storage stack traversal",
    physics: "Creates a file, writes data, and calls fsync to force a full journal commit. \
              Each commit traverses the entire storage stack: CPU \u{2192} filesystem \
              (journal/CoW update, metadata allocation, checksum) \u{2192} storage controller \
              (command queuing, arbitration) \u{2192} storage media (NAND cell programming or \
              magnetic head seek). Every layer contributes independent timing noise from \
              physically distinct sources. On macOS this exercises APFS; on Linux, ext4/XFS.",
    category: SourceCategory::IO,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
    is_fast: false,
};

/// Entropy source from filesystem journal commit timing.
pub struct FsyncJournalSource;

impl EntropySource for FsyncJournalSource {
    fn info(&self) -> &SourceInfo {
        &FSYNC_JOURNAL_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 2 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let write_data = [0xAAu8; 512];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);

        for i in 0..raw_count {
            if i % 64 == 0 && std::time::Instant::now() >= deadline {
                break;
            }
            // Create a new temp file each iteration to exercise the full
            // APFS allocation + B-tree insert + journal commit path.
            let mut tmpfile = match tempfile::NamedTempFile::new() {
                Ok(f) => f,
                Err(_) => continue,
            };

            // Vary the first bytes to prevent APFS deduplication.
            let mut buf = write_data;
            buf[0] = (i & 0xFF) as u8;
            buf[1] = ((i >> 8) & 0xFF) as u8;

            if tmpfile.write_all(&buf).is_err() {
                continue;
            }
            if tmpfile.flush().is_err() {
                continue;
            }
            // Time only the fsync (journal commit) — not the write+flush above.
            let file = tmpfile.as_file();
            let t0 = std::time::Instant::now();
            if file.sync_all().is_err() {
                continue;
            }
            let elapsed = t0.elapsed();

            timings.push(elapsed.as_nanos() as u64);
            // tmpfile is automatically deleted on drop.
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = FsyncJournalSource;
        assert_eq!(src.name(), "fsync_journal");
        assert_eq!(src.info().category, SourceCategory::IO);
        assert!(!src.info().composite);
    }

    #[test]
    #[ignore] // I/O dependent
    fn collects_bytes() {
        let src = FsyncJournalSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }
}
