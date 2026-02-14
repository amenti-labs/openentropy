pub mod bench;
pub mod device;
pub mod entropy;
pub mod monitor;
pub mod pool;
pub mod probe;
pub mod report;
pub mod scan;
pub mod server;
pub mod stream;

use openentropy_core::EntropyPool;
use openentropy_core::conditioning::ConditioningMode;

/// Sources that collect in <2 seconds — safe for real-time use.
const FAST_SOURCES: &[&str] = &[
    "clock_jitter",
    "mach_timing",
    "sleep_jitter",
    "sysctl_deltas",
    "vmstat_deltas",
    "disk_io",
    "memory_timing",
    "dram_row_buffer",
    "cache_contention",
    "page_fault_timing",
    "speculative_execution",
    "cpu_io_beat",
    "cpu_memory_beat",
    "multi_domain_beat",
    "hash_timing",
    "compression_timing",
    "dispatch_queue",
    "vm_page_timing",
    // Frontier sources (all < 0.1s)
    "amx_timing",
    "thread_lifecycle",
    "mach_ipc",
    "tlb_shootdown",
    "pipe_buffer",
    "kqueue_events",
    "dvfs_race",
    "cas_contention",
    "dmp_confusion",
];

/// Build an EntropyPool, optionally filtering sources by name.
/// If no filter is given, only fast sources (<2s) are included to avoid hangs.
/// Use `--sources all` to include every available source.
pub fn make_pool(source_filter: Option<&str>) -> EntropyPool {
    let mut pool = EntropyPool::new(None);

    let sources = openentropy_core::detect_available_sources();

    if let Some(filter) = source_filter {
        if filter == "all" {
            // Include everything
            for source in sources {
                pool.add_source(source, 1.0);
            }
        } else {
            let names: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
            for source in sources {
                let src_name = source.name().to_lowercase();
                if names.iter().any(|n| src_name.contains(&n.to_lowercase())) {
                    pool.add_source(source, 1.0);
                }
            }
        }
    } else {
        // Default: fast sources only
        for source in sources {
            if FAST_SOURCES.contains(&source.name()) {
                pool.add_source(source, 1.0);
            }
        }
    }

    if pool.source_count() == 0 {
        eprintln!("Warning: no sources matched filter, using all fast sources");
        return make_pool(None);
    }
    pool
}

/// Parse a conditioning mode string into the enum.
pub fn parse_conditioning(s: &str) -> ConditioningMode {
    match s {
        "raw" => ConditioningMode::Raw,
        "vonneumann" | "von_neumann" | "vn" => ConditioningMode::VonNeumann,
        "sha256" | "sha" => ConditioningMode::Sha256,
        _ => {
            eprintln!("Unknown conditioning mode '{s}', using sha256");
            ConditioningMode::Sha256
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_conditioning tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_raw() {
        assert_eq!(parse_conditioning("raw"), ConditioningMode::Raw);
    }

    #[test]
    fn test_parse_vonneumann_variants() {
        assert_eq!(
            parse_conditioning("vonneumann"),
            ConditioningMode::VonNeumann
        );
        assert_eq!(
            parse_conditioning("von_neumann"),
            ConditioningMode::VonNeumann
        );
        assert_eq!(parse_conditioning("vn"), ConditioningMode::VonNeumann);
    }

    #[test]
    fn test_parse_sha256_variants() {
        assert_eq!(parse_conditioning("sha256"), ConditioningMode::Sha256);
        assert_eq!(parse_conditioning("sha"), ConditioningMode::Sha256);
    }

    #[test]
    fn test_parse_unknown_defaults_sha256() {
        assert_eq!(parse_conditioning("unknown"), ConditioningMode::Sha256);
        assert_eq!(parse_conditioning(""), ConditioningMode::Sha256);
        assert_eq!(parse_conditioning("RAW"), ConditioningMode::Sha256); // case-sensitive
    }

    // -----------------------------------------------------------------------
    // FAST_SOURCES constant tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_sources_not_empty() {
        assert!(!FAST_SOURCES.is_empty());
    }

    #[test]
    fn test_fast_sources_contains_expected() {
        assert!(FAST_SOURCES.contains(&"clock_jitter"));
        assert!(FAST_SOURCES.contains(&"mach_timing"));
        assert!(FAST_SOURCES.contains(&"sleep_jitter"));
        assert!(FAST_SOURCES.contains(&"disk_io"));
    }

    #[test]
    fn test_fast_sources_excludes_slow() {
        // These slow sources should not be in the fast list
        assert!(!FAST_SOURCES.contains(&"audio_noise"));
        assert!(!FAST_SOURCES.contains(&"camera_noise"));
        assert!(!FAST_SOURCES.contains(&"bluetooth_rssi"));
        assert!(!FAST_SOURCES.contains(&"wifi_rssi"));
    }

    // -----------------------------------------------------------------------
    // make_pool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_make_pool_default_has_sources() {
        // Default pool should include fast sources (on macOS at least some will be available)
        let pool = make_pool(None);
        // On any supported platform, at least the timing sources should work
        assert!(
            pool.source_count() > 0,
            "Default pool should have at least one source"
        );
    }

    #[test]
    fn test_make_pool_all_sources() {
        let pool = make_pool(Some("all"));
        // "all" should include everything available
        assert!(pool.source_count() > 0);
    }

    #[test]
    fn test_make_pool_filter_by_name() {
        let pool = make_pool(Some("clock_jitter"));
        // Should find the clock_jitter source if available on this platform
        // (may be 0 on non-macOS, but the function handles that gracefully)
        // Just verify it doesn't panic
        let _ = pool.source_count();
    }

    #[test]
    fn test_make_pool_filter_comma_separated() {
        let pool = make_pool(Some("clock_jitter,sleep_jitter"));
        // Should accept comma-separated names without panicking
        let _ = pool.source_count();
    }
}
