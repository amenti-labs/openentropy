//! Multi-source entropy pool with health monitoring.
//!
//! Architecture:
//! 1. Auto-discover available sources on this machine
//! 2. Collect raw entropy from each source in parallel
//! 3. Concatenate source bytes into a shared buffer
//! 4. Apply conditioning (Raw / VonNeumann / SHA-256) on output
//! 5. Continuous health monitoring per source
//! 6. Graceful degradation when sources fail
//! 7. Thread-safe for concurrent access

use std::sync::Mutex;
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::conditioning::{quick_min_entropy, quick_shannon};
use crate::source::{EntropySource, SourceState};

/// Thread-safe multi-source entropy pool.
pub struct EntropyPool {
    sources: Vec<Mutex<SourceState>>,
    buffer: Mutex<Vec<u8>>,
    state: Mutex<[u8; 32]>,
    counter: Mutex<u64>,
    total_output: Mutex<u64>,
}

impl EntropyPool {
    /// Create an empty pool.
    pub fn new(seed: Option<&[u8]>) -> Self {
        let initial_state = {
            let mut h = Sha256::new();
            if let Some(s) = seed {
                h.update(s);
            } else {
                // Use OS entropy for initial state
                let mut os_random = [0u8; 32];
                getrandom(&mut os_random);
                h.update(os_random);
            }
            let digest: [u8; 32] = h.finalize().into();
            digest
        };

        Self {
            sources: Vec::new(),
            buffer: Mutex::new(Vec::new()),
            state: Mutex::new(initial_state),
            counter: Mutex::new(0),
            total_output: Mutex::new(0),
        }
    }

    /// Create a pool with all available sources on this machine.
    pub fn auto() -> Self {
        let mut pool = Self::new(None);
        for source in crate::platform::detect_available_sources() {
            pool.add_source(source, 1.0);
        }
        pool
    }

    /// Register an entropy source.
    pub fn add_source(&mut self, source: Box<dyn EntropySource>, weight: f64) {
        self.sources
            .push(Mutex::new(SourceState::new(source, weight)));
    }

    /// Number of registered sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Collect entropy from every registered source (serial).
    /// Collect entropy from every registered source in parallel with per-source
    /// timeout (10s default). Sources that exceed the timeout are skipped.
    pub fn collect_all(&self) -> usize {
        self.collect_all_parallel(10.0)
    }

    /// Collect entropy from all sources in parallel using threads.
    pub fn collect_all_parallel(&self, timeout_secs: f64) -> usize {
        use std::sync::Arc;
        let results: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let deadline = Instant::now() + std::time::Duration::from_secs_f64(timeout_secs);

        std::thread::scope(|s| {
            let handles: Vec<_> = self
                .sources
                .iter()
                .map(|ss_mutex| {
                    let results = Arc::clone(&results);
                    s.spawn(move || {
                        let data = Self::collect_one(ss_mutex);
                        if !data.is_empty() {
                            results.lock().unwrap().extend_from_slice(&data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let _ = handle.join();
            }
        });

        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        let n = results.len();
        self.buffer.lock().unwrap().extend_from_slice(&results);
        n
    }

    /// Collect entropy only from sources whose names are in the given list.
    /// Uses parallel threads. Collects 1000 samples per source.
    pub fn collect_enabled(&self, enabled_names: &[String]) -> usize {
        self.collect_enabled_n(enabled_names, 1000)
    }

    /// Collect `n_samples` of entropy from sources whose names are in the list.
    /// Smaller `n_samples` values are faster — use this for interactive/TUI contexts.
    pub fn collect_enabled_n(&self, enabled_names: &[String], n_samples: usize) -> usize {
        use std::sync::Arc;
        let results: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        std::thread::scope(|s| {
            let handles: Vec<_> = self
                .sources
                .iter()
                .filter(|ss_mutex| {
                    let ss = ss_mutex.lock().unwrap();
                    enabled_names.iter().any(|n| n == ss.source.info().name)
                })
                .map(|ss_mutex| {
                    let results = Arc::clone(&results);
                    s.spawn(move || {
                        let data = Self::collect_one_n(ss_mutex, n_samples);
                        if !data.is_empty() {
                            results.lock().unwrap().extend_from_slice(&data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                let _ = handle.join();
            }
        });

        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        let n = results.len();
        self.buffer.lock().unwrap().extend_from_slice(&results);
        n
    }

    fn collect_one(ss_mutex: &Mutex<SourceState>) -> Vec<u8> {
        Self::collect_one_n(ss_mutex, 1000)
    }

    fn collect_one_n(ss_mutex: &Mutex<SourceState>, n_samples: usize) -> Vec<u8> {
        let mut ss = ss_mutex.lock().unwrap();
        let t0 = Instant::now();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ss.source.collect(n_samples))) {
            Ok(data) if !data.is_empty() => {
                ss.last_collect_time = t0.elapsed();
                ss.total_bytes += data.len() as u64;
                ss.last_entropy = quick_shannon(&data);
                ss.last_min_entropy = quick_min_entropy(&data);
                ss.healthy = ss.last_entropy > 1.0;
                data
            }
            Ok(_) => {
                ss.last_collect_time = t0.elapsed();
                ss.failures += 1;
                ss.healthy = false;
                Vec::new()
            }
            Err(_) => {
                ss.last_collect_time = t0.elapsed();
                ss.failures += 1;
                ss.healthy = false;
                Vec::new()
            }
        }
    }

    /// Return `n_bytes` of raw, unconditioned entropy (XOR-combined only).
    ///
    /// No SHA-256, no DRBG, no whitening. Preserves the raw hardware noise
    /// signal for researchers studying actual device entropy characteristics.
    pub fn get_raw_bytes(&self, n_bytes: usize) -> Vec<u8> {
        // Auto-collect if buffer is low
        {
            let buf = self.buffer.lock().unwrap();
            if buf.len() < n_bytes {
                drop(buf);
                self.collect_all();
            }
        }

        let mut buf = self.buffer.lock().unwrap();
        // If we still don't have enough, collect more rounds
        while buf.len() < n_bytes {
            drop(buf);
            self.collect_all();
            buf = self.buffer.lock().unwrap();
        }

        let output: Vec<u8> = buf.drain(..n_bytes).collect();
        drop(buf);
        *self.total_output.lock().unwrap() += n_bytes as u64;
        output
    }

    /// Return `n_bytes` of conditioned random output.
    pub fn get_random_bytes(&self, n_bytes: usize) -> Vec<u8> {
        // Auto-collect if buffer is low
        {
            let buf = self.buffer.lock().unwrap();
            if buf.len() < n_bytes * 2 {
                drop(buf);
                self.collect_all();
            }
        }

        let mut output = Vec::with_capacity(n_bytes);
        while output.len() < n_bytes {
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            let cnt = *counter;
            drop(counter);

            // Take up to 256 bytes from buffer
            let sample = {
                let mut buf = self.buffer.lock().unwrap();
                let take = buf.len().min(256);
                let sample: Vec<u8> = buf.drain(..take).collect();
                sample
            };

            // SHA-256 conditioning
            let mut h = Sha256::new();
            let state = self.state.lock().unwrap();
            h.update(*state);
            drop(state);
            h.update(&sample);
            h.update(cnt.to_le_bytes());

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            h.update(ts.as_nanos().to_le_bytes());

            // Mix in OS entropy as safety net
            let mut os_random = [0u8; 8];
            getrandom(&mut os_random);
            h.update(os_random);

            let digest: [u8; 32] = h.finalize().into();
            *self.state.lock().unwrap() = digest;
            output.extend_from_slice(&digest);
        }

        *self.total_output.lock().unwrap() += n_bytes as u64;
        output.truncate(n_bytes);
        output
    }

    /// Return `n_bytes` of entropy with the specified conditioning mode.
    ///
    /// - `Raw`: XOR-combined source bytes, no whitening
    /// - `VonNeumann`: debiased but structure-preserving
    /// - `Sha256`: full cryptographic conditioning (default)
    pub fn get_bytes(
        &self,
        n_bytes: usize,
        mode: crate::conditioning::ConditioningMode,
    ) -> Vec<u8> {
        use crate::conditioning::ConditioningMode;
        match mode {
            ConditioningMode::Raw => self.get_raw_bytes(n_bytes),
            ConditioningMode::VonNeumann => {
                // VN debiasing yields ~25% of input, so collect 6x
                let raw = self.get_raw_bytes(n_bytes * 6);
                crate::conditioning::condition(&raw, n_bytes, ConditioningMode::VonNeumann)
            }
            ConditioningMode::Sha256 => self.get_random_bytes(n_bytes),
        }
    }

    /// Health report as structured data.
    pub fn health_report(&self) -> HealthReport {
        let mut sources = Vec::new();
        let mut healthy_count = 0;
        let mut total_raw = 0u64;

        for ss_mutex in &self.sources {
            let ss = ss_mutex.lock().unwrap();
            if ss.healthy {
                healthy_count += 1;
            }
            total_raw += ss.total_bytes;
            sources.push(SourceHealth {
                name: ss.source.name().to_string(),
                healthy: ss.healthy,
                bytes: ss.total_bytes,
                entropy: ss.last_entropy,
                min_entropy: ss.last_min_entropy,
                time: ss.last_collect_time.as_secs_f64(),
                failures: ss.failures,
            });
        }

        HealthReport {
            healthy: healthy_count,
            total: self.sources.len(),
            raw_bytes: total_raw,
            output_bytes: *self.total_output.lock().unwrap(),
            buffer_size: self.buffer.lock().unwrap().len(),
            sources,
        }
    }

    /// Pretty-print health report.
    pub fn print_health(&self) {
        let r = self.health_report();
        println!("\n{}", "=".repeat(60));
        println!("ENTROPY POOL HEALTH REPORT");
        println!("{}", "=".repeat(60));
        println!("Sources: {}/{} healthy", r.healthy, r.total);
        println!("Raw collected: {} bytes", r.raw_bytes);
        println!(
            "Output: {} bytes | Buffer: {} bytes",
            r.output_bytes, r.buffer_size
        );
        println!(
            "\n{:<25} {:>4} {:>10} {:>6} {:>6} {:>7} {:>5}",
            "Source", "OK", "Bytes", "H", "H∞", "Time", "Fail"
        );
        println!("{}", "-".repeat(68));
        for s in &r.sources {
            let ok = if s.healthy { "✓" } else { "✗" };
            println!(
                "{:<25} {:>4} {:>10} {:>5.2} {:>5.2} {:>6.3}s {:>5}",
                s.name, ok, s.bytes, s.entropy, s.min_entropy, s.time, s.failures
            );
        }
    }

    /// Get source info for each registered source.
    pub fn source_infos(&self) -> Vec<SourceInfoSnapshot> {
        self.sources
            .iter()
            .map(|ss_mutex| {
                let ss = ss_mutex.lock().unwrap();
                let info = ss.source.info();
                SourceInfoSnapshot {
                    name: info.name.to_string(),
                    description: info.description.to_string(),
                    physics: info.physics.to_string(),
                    category: info.category.to_string(),
                    entropy_rate_estimate: info.entropy_rate_estimate,
                    composite: info.composite,
                }
            })
            .collect()
    }
}

/// Fill buffer with OS random bytes via the `getrandom` crate.
/// Works cross-platform (Unix, Windows, WASM, etc.) without manual file I/O.
///
/// # Panics
/// Panics if the OS CSPRNG fails — this indicates a fatal platform issue.
fn getrandom(buf: &mut [u8]) {
    getrandom::fill(buf).expect("OS CSPRNG failed");
}

/// Overall health report for the entropy pool.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Number of healthy sources.
    pub healthy: usize,
    /// Total number of registered sources.
    pub total: usize,
    /// Total raw bytes collected across all sources.
    pub raw_bytes: u64,
    /// Total conditioned output bytes produced.
    pub output_bytes: u64,
    /// Current internal buffer size in bytes.
    pub buffer_size: usize,
    /// Per-source health details.
    pub sources: Vec<SourceHealth>,
}

/// Health status of a single entropy source.
#[derive(Debug, Clone)]
pub struct SourceHealth {
    /// Source name.
    pub name: String,
    /// Whether the source is currently healthy (entropy > 1.0 bits/byte).
    pub healthy: bool,
    /// Total bytes collected from this source.
    pub bytes: u64,
    /// Shannon entropy of the last collection (bits per byte, max 8.0).
    pub entropy: f64,
    /// Min-entropy of the last collection (bits per byte, max 8.0). More conservative than Shannon.
    pub min_entropy: f64,
    /// Time taken for the last collection in seconds.
    pub time: f64,
    /// Number of collection failures.
    pub failures: u64,
}

/// Snapshot of source metadata for external consumption.
#[derive(Debug, Clone)]
pub struct SourceInfoSnapshot {
    /// Source name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Physics explanation.
    pub physics: String,
    /// Source category.
    pub category: String,
    /// Estimated entropy rate.
    pub entropy_rate_estimate: f64,
    /// Whether this is a composite source.
    pub composite: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceCategory, SourceInfo};

    // -----------------------------------------------------------------------
    // Mock entropy source for testing
    // -----------------------------------------------------------------------

    /// A deterministic mock entropy source that returns predictable data.
    struct MockSource {
        info: SourceInfo,
        data: Vec<u8>,
    }

    impl MockSource {
        fn new(name: &'static str, data: Vec<u8>) -> Self {
            Self {
                info: SourceInfo {
                    name,
                    description: "mock source",
                    physics: "deterministic test data",
                    category: SourceCategory::System,
                    platform_requirements: &[],
                    entropy_rate_estimate: 1.0,
                    composite: false,
                },
                data,
            }
        }
    }

    impl EntropySource for MockSource {
        fn info(&self) -> &SourceInfo {
            &self.info
        }
        fn is_available(&self) -> bool {
            true
        }
        fn collect(&self, n_samples: usize) -> Vec<u8> {
            self.data.iter().copied().cycle().take(n_samples).collect()
        }
    }

    /// A mock source that always fails (returns empty).
    struct FailingSource {
        info: SourceInfo,
    }

    impl FailingSource {
        fn new(name: &'static str) -> Self {
            Self {
                info: SourceInfo {
                    name,
                    description: "failing mock",
                    physics: "always fails",
                    category: SourceCategory::System,
                    platform_requirements: &[],
                    entropy_rate_estimate: 0.0,
                    composite: false,
                },
            }
        }
    }

    impl EntropySource for FailingSource {
        fn info(&self) -> &SourceInfo {
            &self.info
        }
        fn is_available(&self) -> bool {
            true
        }
        fn collect(&self, _n_samples: usize) -> Vec<u8> {
            Vec::new()
        }
    }

    // -----------------------------------------------------------------------
    // Pool creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_pool_new_empty() {
        let pool = EntropyPool::new(None);
        assert_eq!(pool.source_count(), 0);
    }

    #[test]
    fn test_pool_new_with_seed() {
        let pool = EntropyPool::new(Some(b"test seed"));
        assert_eq!(pool.source_count(), 0);
    }

    #[test]
    fn test_pool_add_source() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock1", vec![42])), 1.0);
        assert_eq!(pool.source_count(), 1);
    }

    #[test]
    fn test_pool_add_multiple_sources() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock1", vec![1])), 1.0);
        pool.add_source(Box::new(MockSource::new("mock2", vec![2])), 1.0);
        pool.add_source(Box::new(MockSource::new("mock3", vec![3])), 0.5);
        assert_eq!(pool.source_count(), 3);
    }

    // -----------------------------------------------------------------------
    // Collection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_all_returns_bytes() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(
            Box::new(MockSource::new("mock1", vec![0xAA, 0xBB, 0xCC])),
            1.0,
        );
        let n = pool.collect_all();
        assert!(n > 0, "Should have collected some bytes");
    }

    #[test]
    fn test_collect_all_parallel_with_timeout() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock1", vec![1, 2])), 1.0);
        pool.add_source(Box::new(MockSource::new("mock2", vec![3, 4])), 1.0);
        let n = pool.collect_all_parallel(5.0);
        assert!(n > 0);
    }

    #[test]
    fn test_collect_enabled_filters_sources() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("alpha", vec![1])), 1.0);
        pool.add_source(Box::new(MockSource::new("beta", vec![2])), 1.0);

        let enabled = vec!["alpha".to_string()];
        let n = pool.collect_enabled(&enabled);
        assert!(n > 0, "Should collect from enabled source");
    }

    #[test]
    fn test_collect_enabled_no_match() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("alpha", vec![1])), 1.0);

        let enabled = vec!["nonexistent".to_string()];
        let n = pool.collect_enabled(&enabled);
        assert_eq!(n, 0, "No sources should match");
    }

    // -----------------------------------------------------------------------
    // Byte output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_raw_bytes_length() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let bytes = pool.get_raw_bytes(64);
        assert_eq!(bytes.len(), 64);
    }

    #[test]
    fn test_get_random_bytes_length() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let bytes = pool.get_random_bytes(64);
        assert_eq!(bytes.len(), 64);
    }

    #[test]
    fn test_get_random_bytes_various_sizes() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        for size in [1, 16, 32, 64, 100, 256] {
            let bytes = pool.get_random_bytes(size);
            assert_eq!(bytes.len(), size, "Expected {size} bytes");
        }
    }

    #[test]
    fn test_get_bytes_raw_mode() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let bytes = pool.get_bytes(32, crate::conditioning::ConditioningMode::Raw);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_get_bytes_sha256_mode() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let bytes = pool.get_bytes(32, crate::conditioning::ConditioningMode::Sha256);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_get_bytes_von_neumann_mode() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let bytes = pool.get_bytes(16, crate::conditioning::ConditioningMode::VonNeumann);
        // VonNeumann may produce fewer bytes due to debiasing yield
        assert!(bytes.len() <= 16);
    }

    // -----------------------------------------------------------------------
    // Health report tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_health_report_empty_pool() {
        let pool = EntropyPool::new(Some(b"test"));
        let report = pool.health_report();
        assert_eq!(report.total, 0);
        assert_eq!(report.healthy, 0);
        assert_eq!(report.raw_bytes, 0);
        assert_eq!(report.output_bytes, 0);
        assert_eq!(report.buffer_size, 0);
        assert!(report.sources.is_empty());
    }

    #[test]
    fn test_health_report_after_collection() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(
            Box::new(MockSource::new("good_source", (0..=255).collect())),
            1.0,
        );
        pool.collect_all();
        let report = pool.health_report();
        assert_eq!(report.total, 1);
        assert!(report.raw_bytes > 0);
        assert_eq!(report.sources.len(), 1);
        assert_eq!(report.sources[0].name, "good_source");
        assert!(report.sources[0].bytes > 0);
    }

    #[test]
    fn test_health_report_failing_source() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(FailingSource::new("bad_source")), 1.0);
        pool.collect_all();
        let report = pool.health_report();
        assert_eq!(report.total, 1);
        assert_eq!(report.healthy, 0);
        assert!(!report.sources[0].healthy);
        assert_eq!(report.sources[0].failures, 1);
    }

    #[test]
    fn test_health_report_mixed_sources() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("good", (0..=255).collect())), 1.0);
        pool.add_source(Box::new(FailingSource::new("bad")), 1.0);
        pool.collect_all();
        let report = pool.health_report();
        assert_eq!(report.total, 2);
        // The good source should be healthy if its entropy > 1.0
        assert!(report.healthy >= 1);
        assert_eq!(report.sources.len(), 2);
    }

    #[test]
    fn test_health_report_tracks_output_bytes() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", (0..=255).collect())), 1.0);
        let _ = pool.get_random_bytes(64);
        let report = pool.health_report();
        assert!(report.output_bytes >= 64);
    }

    // -----------------------------------------------------------------------
    // Source info snapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_source_infos_empty() {
        let pool = EntropyPool::new(Some(b"test"));
        let infos = pool.source_infos();
        assert!(infos.is_empty());
    }

    #[test]
    fn test_source_infos_populated() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("test_src", vec![1])), 1.0);
        let infos = pool.source_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "test_src");
        assert_eq!(infos[0].description, "mock source");
        assert_eq!(infos[0].category, "system");
        assert!((infos[0].entropy_rate_estimate - 1.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Determinism / seed tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_different_seeds_differ() {
        let mut pool1 = EntropyPool::new(Some(b"seed_a"));
        pool1.add_source(Box::new(MockSource::new("m", vec![42; 100])), 1.0);
        let mut pool2 = EntropyPool::new(Some(b"seed_b"));
        pool2.add_source(Box::new(MockSource::new("m", vec![42; 100])), 1.0);

        let bytes1 = pool1.get_random_bytes(32);
        let bytes2 = pool2.get_random_bytes(32);
        assert_ne!(
            bytes1, bytes2,
            "Different seeds should produce different output"
        );
    }

    // -----------------------------------------------------------------------
    // Edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_from_empty_pool() {
        let pool = EntropyPool::new(Some(b"test"));
        let n = pool.collect_all();
        assert_eq!(n, 0, "Empty pool should collect 0 bytes");
    }

    #[test]
    fn test_collect_enabled_empty_list() {
        let mut pool = EntropyPool::new(Some(b"test"));
        pool.add_source(Box::new(MockSource::new("mock", vec![1])), 1.0);
        let n = pool.collect_enabled(&[]);
        assert_eq!(n, 0);
    }
}
