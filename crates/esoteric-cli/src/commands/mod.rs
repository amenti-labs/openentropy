pub mod bench;
pub mod device;
pub mod monitor;
pub mod pool;
pub mod probe;
pub mod report;
pub mod scan;
pub mod server;
pub mod stream;

use esoteric_core::EntropyPool;

/// Sources that collect in <2 seconds — safe for real-time use.
const FAST_SOURCES: &[&str] = &[
    "clock_jitter", "mach_timing", "sleep_jitter",
    "sysctl_deltas", "vmstat_deltas",
    "disk_io", "memory_timing",
    "dram_row_buffer", "cache_contention", "page_fault_timing", "speculative_execution",
    "cpu_io_beat", "cpu_memory_beat", "multi_domain_beat",
    "hash_timing", "compression_timing",
    "dispatch_queue", "vm_page_timing",
];

/// Build an EntropyPool, optionally filtering sources by name.
/// If no filter is given, only fast sources (<2s) are included to avoid hangs.
/// Use `--sources all` to include every available source.
pub fn make_pool(source_filter: Option<&str>) -> EntropyPool {
    let mut pool = EntropyPool::new(None);

    let sources = esoteric_core::detect_available_sources();

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
