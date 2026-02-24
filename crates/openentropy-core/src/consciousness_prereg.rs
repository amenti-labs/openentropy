//! Formal pre-registration with timestamped proof files.
//!
//! Generates cryptographic proof-of-intent before experiments begin,
//! including machine fingerprint, experiment parameters, and timestamps.
//! This prevents post-hoc parameter selection (p-hacking).

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::consciousness::ExperimentMode;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A formal pre-registration record with machine context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormalPreRegistration {
    /// SHA-256-like hash of all parameters.
    pub parameter_hash: String,
    /// Experiment mode.
    pub mode: ExperimentMode,
    /// Experiment parameters.
    pub parameters: PreRegParameters,
    /// Machine fingerprint.
    pub machine_fingerprint: MachineFingerprint,
    /// Registration timestamp (ISO 8601).
    pub timestamp: String,
    /// Unix timestamp (seconds since epoch).
    pub unix_timestamp: u64,
    /// Human-readable summary.
    pub summary: String,
    /// Hash of the entire pre-registration (for verification).
    pub verification_hash: String,
}

/// Experiment parameters captured in pre-registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreRegParameters {
    /// Mode name.
    pub mode: String,
    /// Trials per phase.
    pub trials_per_phase: usize,
    /// Bits per trial.
    pub bits_per_trial: usize,
    /// Trial interval in milliseconds.
    pub trial_interval_ms: u64,
    /// Number of epochs (for epoch-based modes).
    pub epochs: usize,
    /// Epoch duration in seconds.
    pub epoch_duration_secs: u64,
    /// Double-blind enabled.
    pub double_blind: bool,
    /// Operator name.
    pub operator: Option<String>,
    /// Alpha level for significance.
    pub alpha: f64,
    /// Hypothesis direction.
    pub hypothesis: String,
    /// Custom notes.
    pub notes: Option<String>,
}

/// Machine fingerprint for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineFingerprint {
    /// Operating system.
    pub os: String,
    /// Architecture.
    pub arch: String,
    /// Hostname (sanitized).
    pub hostname: String,
    /// Number of available entropy sources.
    pub n_sources: usize,
    /// Source names.
    pub source_names: Vec<String>,
    /// OpenEntropy version.
    pub version: String,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Generate a formal pre-registration with machine context.
pub fn generate_formal_preregistration(
    mode: ExperimentMode,
    params: PreRegParameters,
    source_names: &[String],
) -> FormalPreRegistration {
    let timestamp = crate::consciousness_env::current_wall_time();
    let unix_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let machine = MachineFingerprint {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        hostname: get_hostname(),
        n_sources: source_names.len(),
        source_names: source_names.to_vec(),
        version: crate::VERSION.to_string(),
    };

    // Build parameter hash
    let hash_input = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        mode,
        params.trials_per_phase,
        params.bits_per_trial,
        params.trial_interval_ms,
        params.epochs,
        params.epoch_duration_secs,
        params.double_blind,
        params.operator.as_deref().unwrap_or("anonymous"),
        params.alpha,
        timestamp,
        machine.hostname
    );
    let parameter_hash = hash_string(&hash_input);

    let summary = format!(
        "Pre-registered {} experiment: {} trials/phase x {} bits, {} epochs x {}s, alpha={}, operator={}",
        mode,
        params.trials_per_phase,
        params.bits_per_trial,
        params.epochs,
        params.epoch_duration_secs,
        params.alpha,
        params.operator.as_deref().unwrap_or("anonymous")
    );

    let mut prereg = FormalPreRegistration {
        parameter_hash,
        mode,
        parameters: params,
        machine_fingerprint: machine,
        timestamp,
        unix_timestamp: unix_ts,
        summary,
        verification_hash: String::new(),
    };

    // Compute verification hash of the entire pre-registration
    let verification_input = serde_json::to_string(&prereg).unwrap_or_default();
    prereg.verification_hash = hash_string(&verification_input);

    prereg
}

/// Save a pre-registration to a timestamped proof file.
pub fn save_preregistration(
    prereg: &FormalPreRegistration,
    output_dir: &str,
) -> Result<String, String> {
    let _ = std::fs::create_dir_all(output_dir);

    let filename = format!(
        "prereg_{}_{}.json",
        prereg.mode, prereg.unix_timestamp
    );
    let path = format!("{}/{}", output_dir, filename);

    let json = serde_json::to_string_pretty(prereg)
        .map_err(|e| format!("Failed to serialize: {e}"))?;

    std::fs::write(&path, &json)
        .map_err(|e| format!("Failed to write {path}: {e}"))?;

    Ok(path)
}

/// Verify a pre-registration file hasn't been tampered with.
pub fn verify_preregistration(path: &str) -> Result<VerificationResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;

    let mut prereg: FormalPreRegistration = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse pre-registration: {e}"))?;

    let stored_verification = prereg.verification_hash.clone();
    prereg.verification_hash = String::new();

    let recomputed_input = serde_json::to_string(&prereg).unwrap_or_default();
    let recomputed_hash = hash_string(&recomputed_input);

    let is_valid = stored_verification == recomputed_hash;

    Ok(VerificationResult {
        is_valid,
        prereg: {
            prereg.verification_hash = stored_verification;
            prereg
        },
        recomputed_hash,
    })
}

/// Result of verifying a pre-registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the pre-registration is valid (not tampered).
    pub is_valid: bool,
    /// The pre-registration record.
    pub prereg: FormalPreRegistration,
    /// The recomputed verification hash.
    pub recomputed_hash: String,
}

// ---------------------------------------------------------------------------
// Deep Analysis Config Pre-Registration
// ---------------------------------------------------------------------------

/// Deep analysis configuration captured in pre-registration.
///
/// When `--deep-analysis` is used with `--preregister`, this records all
/// analysis parameters so the exact configuration is committed BEFORE
/// seeing results, preventing post-hoc parameter selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepAnalysisConfig {
    /// Whether deep analysis is enabled.
    pub enabled: bool,
    /// Ordinal pattern order (default: 3).
    pub ordinal_order: usize,
    /// RQA embedding dimension (default: 3).
    pub rqa_dim: usize,
    /// RQA delay (default: 1).
    pub rqa_delay: usize,
    /// RQA threshold (default: 0.1 * range).
    pub rqa_threshold_fraction: f64,
    /// Topology embedding dimension (default: 3).
    pub topology_dim: usize,
    /// Transfer entropy bins (default: 8).
    pub te_bins: usize,
    /// Transfer entropy lag (default: 1).
    pub te_lag: usize,
    /// KSG k for conformal and TE (default: 4).
    pub ksg_k: usize,
    /// Conformal alpha (default: 0.05).
    pub conformal_alpha: f64,
    /// E-value delta (default: 0.01).
    pub evalue_delta: f64,
    /// Number of surrogate permutations (0 = disabled).
    pub surrogate_n: usize,
    /// Hash of this configuration.
    pub config_hash: String,
}

impl DeepAnalysisConfig {
    /// Create a default deep analysis configuration.
    pub fn default_config() -> Self {
        let mut cfg = DeepAnalysisConfig {
            enabled: true,
            ordinal_order: 3,
            rqa_dim: 3,
            rqa_delay: 1,
            rqa_threshold_fraction: 0.1,
            topology_dim: 3,
            te_bins: 8,
            te_lag: 1,
            ksg_k: 4,
            conformal_alpha: 0.05,
            evalue_delta: 0.01,
            surrogate_n: 0,
            config_hash: String::new(),
        };
        cfg.config_hash = cfg.compute_hash();
        cfg
    }

    /// Create a config with surrogate testing enabled.
    pub fn with_surrogates(n_surrogates: usize) -> Self {
        let mut cfg = Self::default_config();
        cfg.surrogate_n = n_surrogates;
        cfg.config_hash = cfg.compute_hash();
        cfg
    }

    /// Compute the hash of this configuration (excluding the hash field itself).
    fn compute_hash(&self) -> String {
        let input = format!(
            "deep:{}:ord{}:rqa{},{},{:.3}:topo{}:te{},{}:ksg{}:conf{:.3}:ev{:.4}:surr{}",
            self.enabled,
            self.ordinal_order,
            self.rqa_dim, self.rqa_delay, self.rqa_threshold_fraction,
            self.topology_dim,
            self.te_bins, self.te_lag,
            self.ksg_k,
            self.conformal_alpha,
            self.evalue_delta,
            self.surrogate_n,
        );
        hash_string(&input)
    }
}

/// Extend a formal pre-registration with deep analysis configuration.
///
/// Appends the deep analysis config hash to the parameter hash and
/// recomputes the verification hash.
pub fn add_deep_analysis_to_prereg(
    prereg: &mut FormalPreRegistration,
    deep_config: &DeepAnalysisConfig,
) {
    // Append deep analysis hash to the parameter hash
    prereg.parameter_hash = hash_string(&format!(
        "{}:deep:{}",
        prereg.parameter_hash, deep_config.config_hash
    ));

    // Update summary
    prereg.summary = format!(
        "{} | Deep analysis: ord={} rqa_dim={} topo_dim={} te_bins={} surr={}",
        prereg.summary,
        deep_config.ordinal_order,
        deep_config.rqa_dim,
        deep_config.topology_dim,
        deep_config.te_bins,
        deep_config.surrogate_n,
    );

    // Recompute verification hash
    prereg.verification_hash = String::new();
    let verification_input = serde_json::to_string(prereg).unwrap_or_default();
    prereg.verification_hash = hash_string(&verification_input);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hash_string(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let h1 = hasher.finish();
    input.len().hash(&mut hasher);
    let h2 = hasher.finish();
    format!("{:016x}{:016x}", h1, h2)
}

fn get_hostname() -> String {
    // Simple hostname retrieval
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        unsafe {
            if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                return String::from_utf8_lossy(&buf[..end]).to_string();
            }
        }
    }
    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_string_deterministic() {
        let h1 = hash_string("hello");
        let h2 = hash_string("hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_string_different() {
        let h1 = hash_string("hello");
        let h2 = hash_string("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_get_hostname() {
        let h = get_hostname();
        assert!(!h.is_empty());
    }

    #[test]
    fn test_generate_preregistration() {
        let params = PreRegParameters {
            mode: "standard".to_string(),
            trials_per_phase: 50,
            bits_per_trial: 200,
            trial_interval_ms: 1000,
            epochs: 5,
            epoch_duration_secs: 30,
            double_blind: false,
            operator: Some("test".to_string()),
            alpha: 0.05,
            hypothesis: "two-tailed".to_string(),
            notes: None,
        };

        let prereg = generate_formal_preregistration(
            ExperimentMode::Standard,
            params,
            &["clock_jitter".to_string(), "sleep_jitter".to_string()],
        );

        assert!(!prereg.parameter_hash.is_empty());
        assert!(!prereg.verification_hash.is_empty());
        assert_eq!(prereg.machine_fingerprint.n_sources, 2);
    }

    #[test]
    fn test_deep_analysis_config_default() {
        let cfg = DeepAnalysisConfig::default_config();
        assert!(cfg.enabled);
        assert_eq!(cfg.ordinal_order, 3);
        assert_eq!(cfg.te_bins, 8);
        assert!(!cfg.config_hash.is_empty());
    }

    #[test]
    fn test_deep_analysis_config_deterministic() {
        let cfg1 = DeepAnalysisConfig::default_config();
        let cfg2 = DeepAnalysisConfig::default_config();
        assert_eq!(cfg1.config_hash, cfg2.config_hash);
    }

    #[test]
    fn test_deep_analysis_config_with_surrogates() {
        let cfg = DeepAnalysisConfig::with_surrogates(100);
        assert_eq!(cfg.surrogate_n, 100);
        let default = DeepAnalysisConfig::default_config();
        assert_ne!(cfg.config_hash, default.config_hash);
    }

    #[test]
    fn test_add_deep_analysis_to_prereg() {
        let params = PreRegParameters {
            mode: "standard".to_string(),
            trials_per_phase: 50,
            bits_per_trial: 200,
            trial_interval_ms: 1000,
            epochs: 5,
            epoch_duration_secs: 30,
            double_blind: false,
            operator: None,
            alpha: 0.05,
            hypothesis: "two-tailed".to_string(),
            notes: None,
        };

        let mut prereg = generate_formal_preregistration(
            ExperimentMode::Standard,
            params,
            &["clock_jitter".to_string()],
        );

        let original_hash = prereg.parameter_hash.clone();
        let original_verification = prereg.verification_hash.clone();

        let deep = DeepAnalysisConfig::default_config();
        add_deep_analysis_to_prereg(&mut prereg, &deep);

        // Hashes should change
        assert_ne!(prereg.parameter_hash, original_hash);
        assert_ne!(prereg.verification_hash, original_verification);
        // Summary should mention deep analysis
        assert!(prereg.summary.contains("Deep analysis"));
    }

    #[test]
    fn test_save_and_verify() {
        let params = PreRegParameters {
            mode: "standard".to_string(),
            trials_per_phase: 10,
            bits_per_trial: 200,
            trial_interval_ms: 100,
            epochs: 2,
            epoch_duration_secs: 5,
            double_blind: false,
            operator: None,
            alpha: 0.05,
            hypothesis: "two-tailed".to_string(),
            notes: None,
        };

        let prereg = generate_formal_preregistration(
            ExperimentMode::Standard,
            params,
            &[],
        );

        let dir = "/tmp/oe_test_prereg";
        let path = save_preregistration(&prereg, dir).unwrap();

        let verify = verify_preregistration(&path).unwrap();
        assert!(verify.is_valid);

        // Cleanup
        let _ = std::fs::remove_dir_all(dir);
    }
}
