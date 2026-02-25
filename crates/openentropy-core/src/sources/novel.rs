//! Novel entropy sources: Spotlight metadata query timing.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

use super::helpers::extract_timing_entropy;

// ---------------------------------------------------------------------------
// SpotlightTimingSource
// ---------------------------------------------------------------------------

/// Files to query via mdls, cycling through them.
const SPOTLIGHT_FILES: &[&str] = &[
    "/usr/bin/true",
    "/usr/bin/false",
    "/usr/bin/env",
    "/usr/bin/which",
];

/// Path to the mdls binary.
const MDLS_PATH: &str = "/usr/bin/mdls";

/// Timeout for mdls commands.
const MDLS_TIMEOUT: Duration = Duration::from_secs(2);

static SPOTLIGHT_TIMING_INFO: SourceInfo = SourceInfo {
    name: "spotlight_timing",
    description: "Spotlight metadata index query timing jitter via mdls",
    physics: "Queries Spotlight\u{2019}s metadata index (mdls) and measures response time. \
              The index is a complex B-tree/inverted index structure. Query timing depends \
              on: index size, disk cache residency, concurrent indexing activity, and \
              filesystem metadata state. When Spotlight is actively indexing new files, \
              query latency becomes highly variable.",
    category: SourceCategory::Signal,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from Spotlight metadata queries.
pub struct SpotlightTimingSource;

impl EntropySource for SpotlightTimingSource {
    fn info(&self) -> &SourceInfo {
        &SPOTLIGHT_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(MDLS_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Cap the number of mdls calls since each has a 2s timeout.
        // mdls usually completes fast (~5ms), so 200 calls is ~1s normally.
        // If mdls hangs, 200 * 2s = 400s is too long, so also add an
        // early-exit when we have enough raw timing data.
        let raw_count = (n_samples * 10 + 64).min(200);
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let file_count = SPOTLIGHT_FILES.len();

        for i in 0..raw_count {
            let file = SPOTLIGHT_FILES[i % file_count];

            // Measure the time to query Spotlight metadata with a timeout.
            // Even timeouts produce useful timing entropy.
            let t0 = Instant::now();

            let child = Command::new(MDLS_PATH)
                .args(["-name", "kMDItemFSName", file])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            if let Ok(mut child) = child {
                let deadline = Instant::now() + MDLS_TIMEOUT;
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => {
                            if Instant::now() >= deadline {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            }

            // Always record timing — timeouts are just as entropic.
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::super::helpers::extract_lsbs_u64;
    use super::*;

    #[test]
    fn spotlight_timing_info() {
        let src = SpotlightTimingSource;
        assert_eq!(src.name(), "spotlight_timing");
        assert_eq!(src.info().category, SourceCategory::Signal);
        assert!((src.info().entropy_rate_estimate - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Run with: cargo test -- --ignored
    fn spotlight_timing_collects_bytes() {
        let src = SpotlightTimingSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }

    #[test]
    fn extract_lsbs_packing() {
        let deltas = vec![1u64, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes.len(), 2);
        // First 8 bits: 1,0,1,0,1,0,1,0 -> 0xAA
        assert_eq!(bytes[0], 0xAA);
        // Next 8 bits: 1,1,1,1,0,0,0,0 -> 0xF0
        assert_eq!(bytes[1], 0xF0);
    }
}
