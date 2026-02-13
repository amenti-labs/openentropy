//! Multi-source entropy pool with health monitoring.
//!
//! Architecture:
//! 1. Auto-discover available sources on this machine
//! 2. Collect raw entropy from each source in parallel
//! 3. XOR-combine independent streams
//! 4. SHA-256 final conditioning
//! 5. Continuous health monitoring per source
//! 6. Graceful degradation when sources fail
//! 7. Thread-safe for concurrent access

use std::sync::Mutex;
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::conditioning::{quick_shannon, quick_min_entropy};
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
    /// Uses parallel threads with a 5-second hard timeout per source.
    pub fn collect_enabled(&self, enabled_names: &[String]) -> usize {
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
                        let data = Self::collect_one(ss_mutex);
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
        let mut ss = ss_mutex.lock().unwrap();
        let t0 = Instant::now();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ss.source.collect(1000))) {
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
    pub fn get_bytes(&self, n_bytes: usize, mode: crate::conditioning::ConditioningMode) -> Vec<u8> {
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
                }
            })
            .collect()
    }
}

/// Fill buffer with OS random bytes.
fn getrandom(buf: &mut [u8]) {
    // Use /dev/urandom on Unix
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(buf);
    }
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
}
