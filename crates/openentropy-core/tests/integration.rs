//! Integration tests for openentropy-core.
//!
//! These tests verify the full entropy pipeline:
//! source discovery → pool creation → entropy collection → quality checks.

use openentropy_core::{
    EntropyPool, SessionConfig, SessionWriter, detect_available_sources, quick_shannon,
};

#[test]
fn detect_sources_finds_sources() {
    let sources = detect_available_sources();
    // On any platform we should find at least a few timing-based sources.
    assert!(
        sources.len() >= 3,
        "Expected at least 3 sources, found {}",
        sources.len()
    );
}

#[test]
fn pool_auto_creates_with_sources() {
    let pool = EntropyPool::auto();
    assert!(
        pool.source_count() >= 3,
        "Expected at least 3 sources in auto pool, found {}",
        pool.source_count()
    );
}

#[test]
#[ignore] // Run with: cargo test -- --ignored
fn pool_produces_requested_byte_count() {
    let pool = EntropyPool::auto();
    for size in [1, 32, 64, 128, 256, 1024] {
        let bytes = pool.get_random_bytes(size);
        assert_eq!(
            bytes.len(),
            size,
            "Expected {} bytes, got {}",
            size,
            bytes.len()
        );
    }
}

#[test]
#[ignore] // Run with: cargo test -- --ignored
fn pool_output_has_high_entropy() {
    let pool = EntropyPool::auto();
    let bytes = pool.get_random_bytes(5000);

    let shannon = quick_shannon(&bytes);
    // Pool output should have near-perfect entropy (SHA-256 conditioned).
    assert!(
        shannon > 7.5,
        "Pool output entropy too low: {:.3}/8.0",
        shannon
    );
}

#[test]
#[ignore] // Run with: cargo test -- --ignored
fn pool_output_not_constant() {
    let pool = EntropyPool::auto();
    let a = pool.get_random_bytes(256);
    let b = pool.get_random_bytes(256);
    // Two consecutive calls should produce different output.
    assert_ne!(
        a, b,
        "Two consecutive get_random_bytes calls returned identical data"
    );
}

#[test]
#[ignore] // Run with: cargo test -- --ignored
fn pool_health_report_structure() {
    let pool = EntropyPool::auto();
    let _ = pool.get_random_bytes(64);

    let report = pool.health_report();
    assert!(report.total > 0);
    assert_eq!(report.sources.len(), report.total);
    assert!(report.output_bytes > 0);
}

#[test]
fn pool_source_infos() {
    let pool = EntropyPool::auto();
    let infos = pool.source_infos();
    assert!(!infos.is_empty());

    for info in &infos {
        assert!(!info.name.is_empty(), "Source name should not be empty");
        assert!(
            !info.description.is_empty(),
            "Source description should not be empty"
        );
        assert!(
            !info.physics.is_empty(),
            "Source physics should not be empty"
        );
    }
}

#[test]
fn empty_pool_still_produces_bytes() {
    // An empty pool (no sources) should still produce output via OS entropy.
    let pool = EntropyPool::new(None);
    let bytes = pool.get_random_bytes(32);
    assert_eq!(bytes.len(), 32);
}

#[test]
fn session_recording_from_clock_jitter() {
    use std::time::{Duration, Instant};

    let tmp = tempfile::tempdir().unwrap();
    let config = SessionConfig {
        sources: vec!["clock_jitter".to_string()],
        output_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let mut writer = SessionWriter::new(config).unwrap();

    // Build a pool with clock_jitter
    let mut pool = EntropyPool::new(None);
    for source in detect_available_sources() {
        if source.name() == "clock_jitter" {
            pool.add_source(source, 1.0);
        }
    }
    assert!(
        pool.source_count() > 0,
        "clock_jitter source not available on this platform"
    );

    // Record for ~2 seconds
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let names = vec!["clock_jitter".to_string()];
        pool.collect_enabled_n(&names, 1000);
        let raw = pool.get_bytes(1000, openentropy_core::ConditioningMode::Raw);
        writer.write_sample("clock_jitter", &raw).unwrap();
    }

    assert!(
        writer.total_samples() > 0,
        "Should have recorded at least 1 sample"
    );
    let dir = writer.finish().unwrap();

    // Verify all 4 files exist
    assert!(dir.join("session.json").exists(), "session.json missing");
    assert!(dir.join("samples.csv").exists(), "samples.csv missing");
    assert!(dir.join("raw.bin").exists(), "raw.bin missing");
    assert!(dir.join("raw_index.csv").exists(), "raw_index.csv missing");

    // Verify session.json is valid
    let json_str = std::fs::read_to_string(dir.join("session.json")).unwrap();
    let meta: openentropy_core::SessionMeta = serde_json::from_str(&json_str).unwrap();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.sources, vec!["clock_jitter"]);
    assert!(meta.total_samples > 0);
    assert!(meta.duration_ms >= 1000); // at least ~1s (allowing some margin)
    assert_eq!(meta.conditioning, "raw");

    // Verify samples.csv has header + data rows
    let csv = std::fs::read_to_string(dir.join("samples.csv")).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines.len() > 1, "CSV should have header + data");
    assert_eq!(
        lines[0],
        "timestamp_ns,source,value_hex,shannon,min_entropy"
    );
    assert!(lines[1].contains("clock_jitter"));

    // Verify raw.bin is non-empty
    let raw = std::fs::read(dir.join("raw.bin")).unwrap();
    assert!(!raw.is_empty(), "raw.bin should not be empty");

    // Verify raw_index.csv has entries
    let index = std::fs::read_to_string(dir.join("raw_index.csv")).unwrap();
    let idx_lines: Vec<&str> = index.lines().collect();
    assert!(
        idx_lines.len() > 1,
        "raw_index.csv should have header + data"
    );
}
