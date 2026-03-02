pub mod analyze;
pub mod bench;
pub mod compare;
pub mod monitor;
pub mod record;
pub mod scan;
pub mod server;
pub mod sessions;
pub mod stream;
pub mod telemetry;

use std::time::{SystemTime, UNIX_EPOCH};

use openentropy_core::EntropyPool;
use openentropy_core::EntropySource;
use openentropy_core::analysis::CrossCorrMatrix;
use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::platform::detect_available_sources;

/// Set the QCicada mode before source discovery so that
/// `QCicadaConfig::default()` picks it up at construction time.
/// Uses a thread-safe `OnceLock` instead of `std::env::set_var` to avoid UB.
#[allow(unused)]
pub fn apply_qcicada_mode(mode: Option<&str>) {
    if let Some(m) = mode {
        let _ =
            openentropy_core::sources::quantum::qcicada_source::QCICADA_CLI_MODE.set(m.to_string());
    }
}

/// Build an EntropyPool, optionally filtering sources by name.
/// If no filter is given, only fast sources (is_fast=true) are included to avoid hangs.
/// Use `--sources all` to include every available source.
pub fn make_pool(source_filter: Option<&str>) -> EntropyPool {
    let mut pool = EntropyPool::new(None);

    let sources = openentropy_core::detect_available_sources();

    if let Some(filter) = source_filter {
        if filter == "all" {
            // Include everything
            for source in sources {
                pool.add_source(source);
            }
        } else {
            let names: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
            for source in sources {
                let src_name = source.name().to_lowercase();
                if names.iter().any(|n| src_name.contains(&n.to_lowercase())) {
                    pool.add_source(source);
                }
            }
        }
    } else {
        // Default: fast sources only (derived from SourceInfo.is_fast)
        for source in sources {
            if source.info().is_fast {
                pool.add_source(source);
            }
        }
    }

    if pool.source_count() == 0 && source_filter.is_some() {
        eprintln!("Warning: no sources matched filter, falling back to fast sources");
        return make_pool(None);
    }
    if pool.source_count() == 0 {
        eprintln!("Error: no entropy sources available on this platform.");
        std::process::exit(1);
    }
    pool
}

/// Parse a conditioning mode string into the enum (case-insensitive).
pub fn parse_conditioning(s: &str) -> ConditioningMode {
    match s.to_lowercase().as_str() {
        "raw" => ConditioningMode::Raw,
        "vonneumann" | "von_neumann" | "vn" => ConditioningMode::VonNeumann,
        "sha256" | "sha" => ConditioningMode::Sha256,
        _ => {
            eprintln!("Unknown conditioning mode '{s}', using sha256");
            ConditioningMode::Sha256
        }
    }
}

/// Current Unix timestamp in seconds.
pub fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Find a single source by name (exact match first, then partial).
pub fn find_source(name: &str) -> Option<Box<dyn EntropySource>> {
    let sources = detect_available_sources();
    // Exact match first
    if let Some(idx) = sources.iter().position(|s| s.name() == name) {
        return Some(sources.into_iter().nth(idx).unwrap());
    }
    // Partial match fallback (case-insensitive)
    let lower = name.to_lowercase();
    let idx = sources
        .iter()
        .position(|s| s.name().to_lowercase().contains(&lower))?;
    Some(sources.into_iter().nth(idx).unwrap())
}

/// Find multiple sources by name. Each name is matched exactly first, then partially.
pub fn find_sources(names: &[String]) -> Vec<Box<dyn EntropySource>> {
    let sources = detect_available_sources();
    let mut used_indices = std::collections::HashSet::new();

    for name in names {
        let lower = name.to_lowercase();
        // Exact match first, then partial
        let idx = sources
            .iter()
            .enumerate()
            .find(|(i, s)| !used_indices.contains(i) && s.name() == name)
            .or_else(|| {
                sources.iter().enumerate().find(|(i, s)| {
                    !used_indices.contains(i) && s.name().to_lowercase().contains(&lower)
                })
            });
        if let Some((i, _)) = idx {
            used_indices.insert(i);
        } else {
            eprintln!("Warning: source '{name}' not found, skipping.");
        }
    }

    // Collect in detection order for determinism
    let mut indices: Vec<usize> = used_indices.into_iter().collect();
    indices.sort();
    let mut result = Vec::with_capacity(indices.len());
    for (i, source) in sources.into_iter().enumerate() {
        if indices.contains(&i) {
            result.push(source);
        }
    }
    result
}

/// Resolve source arguments into a list of sources.
///
/// - If positional names are given, look them up.
/// - If `--all` is set, return all available sources.
/// - Otherwise return fast sources only.
pub fn resolve_sources(positional: &[String], all: bool) -> Vec<Box<dyn EntropySource>> {
    if !positional.is_empty() {
        let sources = find_sources(positional);
        if sources.is_empty() {
            eprintln!(
                "No matching sources found. Run 'openentropy scan' to list available sources."
            );
            std::process::exit(1);
        }
        return sources;
    }

    let all_sources = detect_available_sources();

    if all {
        if all_sources.is_empty() {
            eprintln!("No sources available on this platform.");
            std::process::exit(1);
        }
        return all_sources;
    }

    // Default: fast sources only (derived from SourceInfo.is_fast)
    let fast: Vec<Box<dyn EntropySource>> = all_sources
        .into_iter()
        .filter(|s| s.info().is_fast)
        .collect();

    if fast.is_empty() {
        eprintln!("No fast sources available. Try --all to include all sources.");
        std::process::exit(1);
    }

    fast
}

/// Print a cross-correlation matrix summary to stdout.
pub fn print_cross_correlation(matrix: &CrossCorrMatrix, source_count: usize) {
    println!("\n{:=<68}", "");
    println!("Cross-Correlation Matrix ({} sources)", source_count);
    println!("{:=<68}", "");

    if matrix.flagged_count > 0 {
        println!("\n  {} pair(s) with |r| > 0.3:\n", matrix.flagged_count);
    }

    for pair in &matrix.pairs {
        let flag = if pair.flagged { " !" } else { "" };
        if pair.flagged || pair.correlation.abs() > 0.1 {
            println!(
                "  {:20} x {:20}  r = {:+.4}{}",
                pair.source_a, pair.source_b, pair.correlation, flag
            );
        }
    }

    if matrix.flagged_count == 0 {
        println!("  All pairs below r=0.3 threshold — no strong linear correlation detected.");
    }
}

/// Read and parse session.json from a session directory.
pub fn read_session_meta(session_dir: &std::path::Path) -> openentropy_core::session::SessionMeta {
    let json_path = session_dir.join("session.json");
    let contents = match std::fs::read_to_string(&json_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", json_path.display());
            std::process::exit(1);
        }
    };
    match serde_json::from_str(&contents) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to parse {}: {e}", json_path.display());
            std::process::exit(1);
        }
    }
}

/// Read raw.bin from a session directory.
pub fn read_raw_bin(session_dir: &std::path::Path) -> Vec<u8> {
    let raw_path = session_dir.join("raw.bin");
    if !raw_path.exists() {
        eprintln!("Missing raw.bin in {}", session_dir.display());
        std::process::exit(1);
    }
    match std::fs::read(&raw_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", raw_path.display());
            std::process::exit(1);
        }
    }
}

/// Format a millisecond duration as a human-readable string.
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

/// Analysis profile presets. Individual flags always override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisProfile {
    Quick,
    Standard,
    Deep,
    Security,
}

pub struct AnalyzeDefaults {
    pub samples: usize,
    pub conditioning: &'static str,
    pub entropy: bool,
    pub report: bool,
    pub cross_correlation: bool,
    pub chaos: bool,
}

pub struct SessionsDefaults {
    pub entropy: bool,
    pub trials: bool,
    pub implies_analyze: bool,
}

pub struct CompareDefaults {
    pub entropy: bool,
}

impl AnalysisProfile {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "quick" => Self::Quick,
            "deep" => Self::Deep,
            "security" => Self::Security,
            _ => Self::Standard,
        }
    }

    pub fn analyze_defaults(self) -> AnalyzeDefaults {
        match self {
            Self::Quick => AnalyzeDefaults {
                samples: 10_000,
                conditioning: "raw",
                entropy: false,
                report: false,
                cross_correlation: false,
                chaos: false,
            },
            Self::Standard => AnalyzeDefaults {
                samples: 50_000,
                conditioning: "raw",
                entropy: false,
                report: false,
                cross_correlation: false,
                chaos: false,
            },
            Self::Deep => AnalyzeDefaults {
                samples: 100_000,
                conditioning: "raw",
                entropy: true,
                report: false,
                cross_correlation: true,
                chaos: true,
            },
            Self::Security => AnalyzeDefaults {
                samples: 50_000,
                conditioning: "sha256",
                entropy: true,
                report: true,
                cross_correlation: false,
                chaos: false,
            },
        }
    }

    pub fn sessions_defaults(self) -> SessionsDefaults {
        match self {
            Self::Quick => SessionsDefaults {
                entropy: false,
                trials: false,
                implies_analyze: true,
            },
            Self::Standard => SessionsDefaults {
                entropy: false,
                trials: false,
                implies_analyze: false,
            },
            Self::Deep => SessionsDefaults {
                entropy: true,
                trials: true,
                implies_analyze: true,
            },
            Self::Security => SessionsDefaults {
                entropy: true,
                trials: false,
                implies_analyze: true,
            },
        }
    }

    pub fn compare_defaults(self) -> CompareDefaults {
        match self {
            Self::Quick | Self::Standard => CompareDefaults { entropy: false },
            Self::Deep | Self::Security => CompareDefaults { entropy: true },
        }
    }
}

/// Write a serializable value as pretty JSON to a file.
pub fn write_json<T: serde::Serialize>(value: &T, path: &str, label: &str) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => match std::fs::write(path, json) {
            Ok(()) => println!("\n{label} written to {path}"),
            Err(e) => eprintln!("\nFailed to write {path}: {e}"),
        },
        Err(e) => eprintln!("\nFailed to serialize {label}: {e}"),
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
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_conditioning("RAW"), ConditioningMode::Raw);
        assert_eq!(parse_conditioning("Sha256"), ConditioningMode::Sha256);
        assert_eq!(
            parse_conditioning("VonNeumann"),
            ConditioningMode::VonNeumann
        );
    }

    #[test]
    fn test_analyze_profile_standard_defaults() {
        let d = AnalysisProfile::Standard.analyze_defaults();
        assert_eq!(d.samples, 50_000);
        assert_eq!(d.conditioning, "raw");
        assert!(!d.entropy);
        assert!(!d.report);
        assert!(!d.cross_correlation);
    }

    #[test]
    fn test_sessions_profile_deep_defaults() {
        let d = AnalysisProfile::Deep.sessions_defaults();
        assert!(d.implies_analyze);
        assert!(d.entropy);
        assert!(d.trials);
    }

    #[test]
    fn test_compare_profile_security_defaults() {
        let d = AnalysisProfile::Security.compare_defaults();
        assert!(d.entropy);
    }

    // -----------------------------------------------------------------------
    // is_fast metadata tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_sources_derived_from_metadata() {
        let sources = openentropy_core::detect_available_sources();
        let fast: Vec<_> = sources.iter().filter(|s| s.info().is_fast).collect();
        assert!(!fast.is_empty(), "Should have at least one fast source");
    }

    #[test]
    fn test_slow_sources_not_fast() {
        let sources = openentropy_core::detect_available_sources();
        for s in &sources {
            let name = s.name();
            if [
                "audio_noise",
                "camera_noise",
                "bluetooth_noise",
                "wifi_rssi",
            ]
            .contains(&name)
            {
                assert!(!s.info().is_fast, "{name} should not be fast");
            }
        }
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
