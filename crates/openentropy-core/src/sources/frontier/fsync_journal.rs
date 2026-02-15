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
//! PoC measured H∞ ≈ 7.1 bits/byte for 4KB fsync cycles — excellent.

use std::io::Write;

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static FSYNC_JOURNAL_INFO: SourceInfo = SourceInfo {
    name: "fsync_journal",
    description: "APFS journal commit timing from full storage stack traversal",
    physics: "Creates a file, writes data, and calls fsync to force a full journal commit. \
              Each commit traverses the entire storage stack: CPU \u{2192} APFS filesystem \
              (B-tree update, copy-on-write allocation, checksum) \u{2192} NVMe controller \
              (command queuing, arbitration) \u{2192} NAND flash (cell programming, charge \
              injection timing, wear-dependent oxide characteristics) \u{2192} barrier flush \
              (controller firmware scheduling). Every layer contributes independent \
              timing noise from physically distinct sources. \
              PoC measured H\u{221e} \u{2248} 7.1 bits/byte.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
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
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let write_data = [0xAAu8; 512];

        for i in 0..raw_count {
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

            let t0 = std::time::Instant::now();
            if tmpfile.write_all(&buf).is_err() {
                continue;
            }
            if tmpfile.flush().is_err() {
                continue;
            }
            // fsync forces the full journal commit.
            let file = tmpfile.as_file();
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
        assert_eq!(src.info().category, SourceCategory::Frontier);
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
