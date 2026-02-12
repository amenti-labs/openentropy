//! Integration tests for esoteric-core.
//!
//! These tests verify the full entropy pipeline:
//! source discovery → pool creation → entropy collection → quality checks.

use esoteric_core::{EntropyPool, detect_available_sources, quick_shannon};

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
fn pool_output_not_constant() {
    let pool = EntropyPool::auto();
    let a = pool.get_random_bytes(256);
    let b = pool.get_random_bytes(256);
    // Two consecutive calls should produce different output.
    assert_ne!(a, b, "Two consecutive get_random_bytes calls returned identical data");
}

#[test]
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
