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

/// Build an EntropyPool, optionally filtering sources by name.
pub fn make_pool(source_filter: Option<&str>) -> EntropyPool {
    if let Some(filter) = source_filter {
        let names: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
        let mut pool = EntropyPool::new(None);
        for source in esoteric_core::detect_available_sources() {
            let src_name = source.name().to_lowercase();
            if names.iter().any(|n| src_name.contains(&n.to_lowercase())) {
                pool.add_source(source, 1.0);
            }
        }
        if pool.source_count() == 0 {
            eprintln!("Warning: no sources matched filter '{filter}'");
            return EntropyPool::auto();
        }
        pool
    } else {
        EntropyPool::auto()
    }
}
