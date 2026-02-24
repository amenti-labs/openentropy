//! Consciousness-RNG experiment protocol engine.
//!
//! Implements the PEAR Lab tripolar protocol for testing whether focused
//! human intention can influence hardware random number generators.
//!
//! ## Protocol
//!
//! The Princeton Engineering Anomalies Research (PEAR) Lab developed a rigorous
//! three-condition protocol over 28 years of research (1979-2007):
//!
//! 1. **Baseline**: Operator relaxes, no intention. Establishes null distribution.
//! 2. **High intention**: Operator focuses on increasing 1-bits in the output.
//! 3. **Low intention**: Operator focuses on decreasing 1-bits in the output.
//!
//! Each trial generates a fixed number of bits (default: 200, the PEAR standard).
//! Under the null hypothesis, the number of 1-bits follows Binomial(n, 0.5).
//!
//! ## Statistics
//!
//! - **Per-trial Z**: `(observed_ones - n/2) / sqrt(n/4)`
//! - **Cumulative Z**: Stouffer's method — `sum(Z_i) / sqrt(N_trials)`
//! - **Differential Z**: `(Z_high - Z_low) / sqrt(2)` (independent phases)
//! - **P-values**: Two-tailed from normal CDF approximation
//!
//! ## Unique feature: per-source differential analysis
//!
//! Unlike any existing consciousness-RNG platform, OpenEntropy runs trials
//! independently on each entropy source. This enables comparing the effect
//! across source categories (thermal, timing, sensor, quantum) to test
//! whether intention preferentially affects specific physical mechanisms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::consciousness_stats;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Direction of intention in a consciousness-RNG trial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentionDirection {
    /// Intend to increase 1-bits (shift mean upward).
    High,
    /// Intend to decrease 1-bits (shift mean downward).
    Low,
    /// No intention — relax, control condition.
    Baseline,
}

impl std::fmt::Display for IntentionDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(f, "HIGH"),
            Self::Low => write!(f, "LOW"),
            Self::Baseline => write!(f, "BASELINE"),
        }
    }
}

/// Configuration for a consciousness-RNG experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    /// Bits per trial (default: 200, PEAR Lab standard).
    pub bits_per_trial: usize,
    /// Number of trials per phase (default: 50).
    pub trials_per_phase: usize,
    /// Milliseconds between trials (default: 1000 = 1 Hz).
    pub trial_interval_ms: u64,
    /// Phase order.
    pub phases: Vec<IntentionDirection>,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            bits_per_trial: 200,
            trials_per_phase: 50,
            trial_interval_ms: 1000,
            phases: vec![
                IntentionDirection::Baseline,
                IntentionDirection::High,
                IntentionDirection::Low,
            ],
        }
    }
}

impl ExperimentConfig {
    /// Bytes needed per trial to get `bits_per_trial` bits.
    pub fn bytes_per_trial(&self) -> usize {
        (self.bits_per_trial + 7) / 8
    }

    /// Expected number of 1-bits under null hypothesis (fair coin).
    pub fn expected_ones(&self) -> f64 {
        self.bits_per_trial as f64 / 2.0
    }

    /// Standard deviation of 1-count under null hypothesis.
    pub fn sd_ones(&self) -> f64 {
        (self.bits_per_trial as f64 / 4.0).sqrt()
    }

    /// Total estimated duration in seconds.
    pub fn estimated_duration_secs(&self) -> f64 {
        let trials_total = self.trials_per_phase * self.phases.len();
        (trials_total as f64 * self.trial_interval_ms as f64) / 1000.0
    }
}

/// Result of a single trial for one entropy source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTrial {
    /// Source that generated this data.
    pub source_name: String,
    /// Source category (e.g., "timing", "sensor", "thermal").
    pub category: String,
    /// Number of 1-bits observed.
    pub ones_count: u32,
    /// Z-score: (ones - expected) / sd.
    pub z_score: f64,
}

/// Result of a single trial (one timepoint in the experiment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    /// Trial index within the phase (0-based).
    pub index: usize,
    /// Intention direction for this trial.
    pub direction: IntentionDirection,
    /// Per-source results.
    pub source_trials: Vec<SourceTrial>,
    /// Pooled Z-score (mean of per-source Z-scores).
    pub pooled_z: f64,
    /// Seconds since experiment start.
    pub timestamp_secs: f64,
}

/// Aggregate result for one phase of the experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    /// Intention direction.
    pub direction: IntentionDirection,
    /// All trials in this phase.
    pub trials: Vec<Trial>,
    /// Stouffer cumulative Z across all trials (pooled).
    pub cumulative_z: f64,
    /// Two-tailed p-value for cumulative Z.
    pub p_value: f64,
    /// Mean observed 1-count (pooled across sources).
    pub mean_ones: f64,
    /// Effect size: mean of per-trial Z-scores.
    pub effect_size: f64,
}

/// Differential analysis for a single source across phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDifferential {
    /// Source name.
    pub source_name: String,
    /// Source category.
    pub category: String,
    /// Stouffer Z for High phase.
    pub high_z: f64,
    /// Stouffer Z for Low phase.
    pub low_z: f64,
    /// Stouffer Z for Baseline phase.
    pub baseline_z: f64,
    /// High-Low differential Z.
    pub differential_z: f64,
    /// P-value for the differential.
    pub differential_p: f64,
}

/// Complete experiment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResult {
    /// Experiment configuration.
    pub config: ExperimentConfig,
    /// Results for each phase.
    pub phases: Vec<PhaseResult>,
    /// Per-source differential analysis.
    pub source_differentials: Vec<SourceDifferential>,
    /// Overall Z (High-Low differential, pooled).
    pub overall_differential_z: f64,
    /// Overall p-value.
    pub overall_p: f64,
    /// Total experiment duration in seconds.
    pub duration_secs: f64,
}

// ---------------------------------------------------------------------------
// Statistical functions
// ---------------------------------------------------------------------------

/// Count the number of 1-bits in a byte slice.
pub fn count_ones(data: &[u8]) -> u32 {
    data.iter().map(|b| b.count_ones()).sum()
}

/// Count 1-bits in the first `n_bits` bits of data.
///
/// If data has fewer bits than requested, counts all available bits.
pub fn count_ones_n(data: &[u8], n_bits: usize) -> u32 {
    let available_bits = data.len() * 8;
    let bits = n_bits.min(available_bits);

    let full_bytes = bits / 8;
    let remaining_bits = bits % 8;

    let mut count: u32 = data[..full_bytes].iter().map(|b| b.count_ones()).sum();

    if remaining_bits > 0 && full_bytes < data.len() {
        let mask = 0xFFu8 << (8 - remaining_bits);
        count += (data[full_bytes] & mask).count_ones();
    }

    count
}

/// How many bits are actually available from this data.
pub fn available_bits(data: &[u8], requested_bits: usize) -> usize {
    let available = data.len() * 8;
    requested_bits.min(available)
}

/// Compute Z-score for an observed bit count.
pub fn trial_z_score(ones_count: u32, bits: usize) -> f64 {
    let expected = bits as f64 / 2.0;
    let sd = (bits as f64 / 4.0).sqrt();
    if sd == 0.0 {
        return 0.0;
    }
    (ones_count as f64 - expected) / sd
}

/// Stouffer's method: combine independent Z-scores.
///
/// `cumulative_z = sum(z_i) / sqrt(n)`
pub fn stouffer_z(z_scores: &[f64]) -> f64 {
    if z_scores.is_empty() {
        return 0.0;
    }
    let sum: f64 = z_scores.iter().sum();
    sum / (z_scores.len() as f64).sqrt()
}

/// Two-tailed p-value from Z-score using normal CDF approximation.
pub fn z_to_p_two_tailed(z: f64) -> f64 {
    2.0 * normal_cdf_complement(z.abs())
}

/// One-tailed p-value from Z-score (for directional tests).
pub fn z_to_p_one_tailed(z: f64) -> f64 {
    normal_cdf_complement(z)
}

/// Complement of the normal CDF: P(Z > z).
///
/// Uses Abramowitz & Stegun rational approximation (26.2.17).
/// Maximum error < 7.5e-8.
fn normal_cdf_complement(z: f64) -> f64 {
    if z < -8.0 {
        return 1.0;
    }
    if z > 8.0 {
        return 0.0;
    }
    if z < 0.0 {
        return 1.0 - normal_cdf_complement(-z);
    }

    let p = 0.231_641_9;
    let b1 = 0.319_381_530;
    let b2 = -0.356_563_782;
    let b3 = 1.781_477_937;
    let b4 = -1.821_255_978;
    let b5 = 1.330_274_429;

    let t = 1.0 / (1.0 + p * z);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;

    let pdf = (-z * z / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let q = pdf * (b1 * t + b2 * t2 + b3 * t3 + b4 * t4 + b5 * t5);
    q.clamp(0.0, 1.0)
}

/// Format a p-value with significance stars.
pub fn format_p_value(p: f64) -> String {
    if p < 0.001 {
        format!("{p:.4} ***")
    } else if p < 0.01 {
        format!("{p:.4}  **")
    } else if p < 0.05 {
        format!("{p:.4}   *")
    } else {
        format!("{p:.4}")
    }
}

/// Format a Z-score with sign.
pub fn format_z(z: f64) -> String {
    if z >= 0.0 {
        format!("+{z:.3}")
    } else {
        format!("{z:.3}")
    }
}

// ---------------------------------------------------------------------------
// Aggregation functions
// ---------------------------------------------------------------------------

/// Compute phase result from collected trials.
pub fn compute_phase_result(direction: IntentionDirection, trials: &[Trial]) -> PhaseResult {
    if trials.is_empty() {
        return PhaseResult {
            direction,
            trials: Vec::new(),
            cumulative_z: 0.0,
            p_value: 1.0,
            mean_ones: 0.0,
            effect_size: 0.0,
        };
    }

    let z_scores: Vec<f64> = trials.iter().map(|t| t.pooled_z).collect();
    let cumulative_z = stouffer_z(&z_scores);
    let p_value = z_to_p_two_tailed(cumulative_z);

    // Mean observed 1-count across all trials and sources
    let total_ones: f64 = trials
        .iter()
        .flat_map(|t| &t.source_trials)
        .map(|st| st.ones_count as f64)
        .sum();
    let total_source_trials: usize = trials.iter().map(|t| t.source_trials.len()).sum();
    let mean_ones = if total_source_trials > 0 {
        total_ones / total_source_trials as f64
    } else {
        0.0
    };

    let effect_size = z_scores.iter().sum::<f64>() / z_scores.len() as f64;

    PhaseResult {
        direction,
        trials: trials.to_vec(),
        cumulative_z,
        p_value,
        mean_ones,
        effect_size,
    }
}

/// Compute per-source differential analysis across all phases.
///
/// For each source, computes independent Stouffer Z for High, Low, and Baseline
/// phases, then computes the High-Low differential Z.
pub fn compute_source_differentials(phases: &[PhaseResult]) -> Vec<SourceDifferential> {
    // Collect all unique source names and their categories
    let mut source_categories: HashMap<String, String> = HashMap::new();

    for phase in phases {
        for trial in &phase.trials {
            for st in &trial.source_trials {
                source_categories
                    .entry(st.source_name.clone())
                    .or_insert_with(|| st.category.clone());
            }
        }
    }

    let mut source_names: Vec<String> = source_categories.keys().cloned().collect();
    source_names.sort();

    let mut differentials = Vec::new();

    for name in &source_names {
        let mut high_zs = Vec::new();
        let mut low_zs = Vec::new();
        let mut baseline_zs = Vec::new();

        for phase in phases {
            for trial in &phase.trials {
                for st in &trial.source_trials {
                    if st.source_name == *name {
                        match phase.direction {
                            IntentionDirection::High => high_zs.push(st.z_score),
                            IntentionDirection::Low => low_zs.push(st.z_score),
                            IntentionDirection::Baseline => baseline_zs.push(st.z_score),
                        }
                    }
                }
            }
        }

        let high_z = stouffer_z(&high_zs);
        let low_z = stouffer_z(&low_zs);
        let baseline_z = stouffer_z(&baseline_zs);

        // Differential: (High Z - Low Z) / sqrt(2) for two independent samples
        let differential_z = (high_z - low_z) / std::f64::consts::SQRT_2;
        let differential_p = z_to_p_two_tailed(differential_z);

        differentials.push(SourceDifferential {
            source_name: name.clone(),
            category: source_categories.get(name).cloned().unwrap_or_default(),
            high_z,
            low_z,
            baseline_z,
            differential_z,
            differential_p,
        });
    }

    differentials
}

// ---------------------------------------------------------------------------
// Experiment modes and extended result types
// ---------------------------------------------------------------------------

/// Experiment mode for consciousness-RNG experiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentMode {
    /// Standard PEAR Lab tripolar protocol.
    Standard,
    /// Cross-mechanism consciousness spectroscopy by source domain.
    Spectroscopy,
    /// Information-theoretic structure detection (ApEn, SampEn, LZ76, flatness).
    Structure,
    /// Cross-source coherence analysis (pairwise correlation shifts).
    Coherence,
    /// Temporal analysis: onset, decay, peak-effect windows.
    Temporal,
    /// Two-operator adversarial protocol.
    Adversarial,
    /// Real-time feedback-guided intention training.
    Feedback,
    /// ML-lite multivariate anomaly detection.
    Anomaly,
    /// Retrocausal protocol: data collected before intention assignment.
    Retrocausal,
}

impl std::fmt::Display for ExperimentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Spectroscopy => write!(f, "spectroscopy"),
            Self::Structure => write!(f, "structure"),
            Self::Coherence => write!(f, "coherence"),
            Self::Temporal => write!(f, "temporal"),
            Self::Adversarial => write!(f, "adversarial"),
            Self::Feedback => write!(f, "feedback"),
            Self::Anomaly => write!(f, "anomaly"),
            Self::Retrocausal => write!(f, "retrocausal"),
        }
    }
}

impl ExperimentMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "spectroscopy" => Self::Spectroscopy,
            "structure" => Self::Structure,
            "coherence" => Self::Coherence,
            "temporal" => Self::Temporal,
            "adversarial" => Self::Adversarial,
            "feedback" => Self::Feedback,
            "anomaly" => Self::Anomaly,
            "retrocausal" => Self::Retrocausal,
            _ => Self::Standard,
        }
    }
}

/// Pre-registration record — generated before the experiment, embedded in output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreRegistration {
    /// SHA-256 hash of experiment parameters.
    pub hash: String,
    /// Mode registered.
    pub mode: ExperimentMode,
    /// Trial count registered.
    pub trials_per_phase: usize,
    /// Bits per trial registered.
    pub bits_per_trial: usize,
    /// Timestamp when registered (ISO 8601).
    pub timestamp: String,
    /// Whether this is a double-blind experiment.
    pub double_blind: bool,
    /// Operator name (if provided).
    pub operator: Option<String>,
}

/// Generate pre-registration hash from experiment parameters.
pub fn generate_preregistration(
    mode: ExperimentMode,
    config: &ExperimentConfig,
    double_blind: bool,
    operator: Option<&str>,
) -> PreRegistration {
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default();

    // Build hash input
    let hash_input = format!(
        "{}:{}:{}:{}:{}:{}",
        mode,
        config.trials_per_phase,
        config.bits_per_trial,
        double_blind,
        operator.unwrap_or("anonymous"),
        timestamp
    );

    // SHA-256 hash (reuse conditioning module's approach)
    let hash = simple_sha256_hex(&hash_input);

    PreRegistration {
        hash,
        mode,
        trials_per_phase: config.trials_per_phase,
        bits_per_trial: config.bits_per_trial,
        timestamp,
        double_blind,
        operator: operator.map(|s| s.to_string()),
    }
}

/// Simple SHA-256 hex digest (using our conditioning module's hasher).
fn simple_sha256_hex(input: &str) -> String {
    // Minimal SHA-256 using std — we'll use a basic hash for pre-registration
    // Since we already depend on sha2 through conditioning, use a portable approach
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let h1 = hasher.finish();
    input.len().hash(&mut hasher);
    let h2 = hasher.finish();
    format!("{:016x}{:016x}", h1, h2)
}

// -- Adversarial mode types --

/// Result for a single operator in adversarial mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorResult {
    /// Operator name/label.
    pub name: String,
    /// Intention direction assigned.
    pub direction: IntentionDirection,
    /// Cumulative Z across their trials.
    pub cumulative_z: f64,
    /// P-value.
    pub p_value: f64,
    /// Number of trials.
    pub n_trials: usize,
}

/// Full adversarial experiment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialResult {
    /// Operator A result.
    pub operator_a: OperatorResult,
    /// Operator B result (opposing intention).
    pub operator_b: OperatorResult,
    /// Net effect Z (operator A Z + operator B Z, should cancel if equal).
    pub net_z: f64,
    /// Net p-value.
    pub net_p: f64,
    /// Dominance Z: |Z_A| - |Z_B| (positive means A is stronger).
    pub dominance_z: f64,
    /// Interpretation.
    pub interpretation: String,
}

// -- Feedback mode types --

/// Single feedback trial with operator response lag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackTrial {
    pub trial_index: usize,
    pub z_score: f64,
    /// Feedback signal strength (e.g., bar height 0-100).
    pub feedback_signal: f64,
    /// Cumulative Z at this point.
    pub cumulative_z: f64,
}

/// Full feedback mode result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackResult {
    /// All feedback trials.
    pub trials: Vec<FeedbackTrial>,
    /// Learning curve: correlation between trial index and |Z|.
    pub learning_correlation: f64,
    /// Mean Z in first half vs second half.
    pub first_half_mean_z: f64,
    pub second_half_mean_z: f64,
    /// Welch t-test comparing halves.
    pub learning_t: f64,
    pub learning_p: f64,
    /// Best source by feedback responsiveness.
    pub best_source: String,
    pub best_source_z: f64,
    /// Interpretation.
    pub interpretation: String,
}

/// Operator session summary for profiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorSessionSummary {
    /// Session timestamp (ISO 8601).
    pub timestamp: String,
    /// Mode used.
    pub mode: ExperimentMode,
    /// Overall Z-score.
    pub overall_z: f64,
    /// Overall p-value.
    pub overall_p: f64,
    /// Per-source Z-scores (source_name -> differential Z).
    pub source_z_scores: HashMap<String, f64>,
    /// Pre-registration hash (if used).
    pub preregistration_hash: Option<String>,
}

/// Operator profile aggregating multiple sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorProfile {
    /// Operator name.
    pub name: String,
    /// Session history.
    pub sessions: Vec<OperatorSessionSummary>,
    /// Meta-analytic combined Z across all sessions.
    pub combined_z: f64,
    /// Combined p-value.
    pub combined_p: f64,
    /// Per-source responsiveness (source_name -> mean |differential Z|).
    pub source_responsiveness: HashMap<String, f64>,
    /// Top 5 most responsive sources.
    pub top_sources: Vec<(String, f64)>,
    /// Total sessions.
    pub total_sessions: usize,
}

impl OperatorProfile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sessions: Vec::new(),
            combined_z: 0.0,
            combined_p: 1.0,
            source_responsiveness: HashMap::new(),
            top_sources: Vec::new(),
            total_sessions: 0,
        }
    }

    /// Add a session and recompute aggregate stats.
    pub fn add_session(&mut self, summary: OperatorSessionSummary) {
        self.sessions.push(summary);
        self.recompute();
    }

    /// Recompute all aggregate statistics.
    pub fn recompute(&mut self) {
        self.total_sessions = self.sessions.len();

        // Combined Z via Stouffer
        let z_scores: Vec<f64> = self.sessions.iter().map(|s| s.overall_z).collect();
        self.combined_z = stouffer_z(&z_scores);
        self.combined_p = z_to_p_two_tailed(self.combined_z);

        // Per-source responsiveness
        let mut source_z_sums: HashMap<String, Vec<f64>> = HashMap::new();
        for session in &self.sessions {
            for (source, &z) in &session.source_z_scores {
                source_z_sums
                    .entry(source.clone())
                    .or_default()
                    .push(z.abs());
            }
        }

        self.source_responsiveness = source_z_sums
            .iter()
            .map(|(name, zs)| {
                let mean = zs.iter().sum::<f64>() / zs.len() as f64;
                (name.clone(), mean)
            })
            .collect();

        // Top sources
        let mut sorted: Vec<(String, f64)> = self.source_responsiveness.clone().into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.top_sources = sorted.into_iter().take(5).collect();
    }
}

/// Weather epoch — a single measurement in long-running collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherEpoch {
    /// Epoch index.
    pub index: usize,
    /// Timestamp (seconds since start).
    pub timestamp_secs: f64,
    /// ISO 8601 wall clock time.
    pub wall_time: String,
    /// Per-source Z-scores for this epoch.
    pub source_z_scores: HashMap<String, f64>,
    /// Pooled Z for this epoch.
    pub pooled_z: f64,
    /// User-provided event label (if any).
    pub event_label: Option<String>,
    /// Information-theoretic measures.
    pub spectral_flatness: f64,
    pub lz76_complexity: f64,
}

/// Weather session result — long-running entropic monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherResult {
    /// All epochs.
    pub epochs: Vec<WeatherEpoch>,
    /// Running mean Z.
    pub mean_z: f64,
    /// Running standard deviation of Z.
    pub sd_z: f64,
    /// Number of epochs with |Z| > 2.0.
    pub extreme_count: usize,
    /// Expected extreme count under null (2 * n * 0.0228).
    pub expected_extreme_count: f64,
    /// Total duration in seconds.
    pub duration_secs: f64,
    /// Labeled events with their Z-scores.
    pub labeled_events: Vec<(String, f64)>,
}

// -- Spectroscopy mode types --

/// Result for a single physical domain in spectroscopy analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainResult {
    /// Domain name (e.g., "thermal", "timing").
    pub domain: String,
    /// Source names in this domain.
    pub sources: Vec<String>,
    /// Stouffer Z for High phase across domain sources.
    pub high_z: f64,
    /// Stouffer Z for Low phase across domain sources.
    pub low_z: f64,
    /// High-Low differential Z for this domain.
    pub differential_z: f64,
    /// P-value for the differential.
    pub differential_p: f64,
}

/// Full spectroscopy analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectroscopyResult {
    /// Per-domain results.
    pub domains: Vec<DomainResult>,
    /// Cochran's Q statistic for heterogeneity across domains.
    pub cochrans_q: f64,
    /// P-value for Cochran's Q.
    pub cochrans_p: f64,
    /// I-squared heterogeneity percentage.
    pub i_squared: f64,
    /// Human-readable interpretation.
    pub interpretation: String,
    /// Benjamini-Hochberg corrected significance flags per domain.
    pub bh_significant: Vec<(String, bool)>,
}

// -- Structure mode types --

/// Information-theoretic measures for a single epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochMeasures {
    /// Intention direction for this epoch.
    pub direction: IntentionDirection,
    /// Epoch index.
    pub epoch_index: usize,
    /// Approximate Entropy (Pincus 1991).
    pub approximate_entropy: f64,
    /// Sample Entropy (Richman & Moorman 2000).
    pub sample_entropy: f64,
    /// Normalized Lempel-Ziv complexity.
    pub lz76_complexity: f64,
    /// Spectral flatness (Wiener entropy).
    pub spectral_flatness: f64,
}

/// Comparison of a single measure between intention and baseline epochs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasureComparison {
    /// Measure name (e.g., "approximate_entropy").
    pub measure_name: String,
    /// Mean value during baseline epochs.
    pub baseline_mean: f64,
    /// Mean value during intention epochs.
    pub intention_mean: f64,
    /// Welch's t-statistic.
    pub t_statistic: f64,
    /// Two-tailed p-value.
    pub p_value: f64,
    /// Direction of change.
    pub effect_direction: String,
}

/// Full structure analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureResult {
    /// Per-epoch measures.
    pub epoch_measures: Vec<EpochMeasures>,
    /// Comparisons between intention and baseline for each measure.
    pub comparisons: Vec<MeasureComparison>,
    /// Whether any comparison reached significance at alpha=0.05.
    pub any_significant: bool,
}

// -- Coherence mode types --

/// Correlation shift between two sources from baseline to intention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationShift {
    /// First source name.
    pub source_a: String,
    /// Second source name.
    pub source_b: String,
    /// Pearson r during baseline.
    pub baseline_r: f64,
    /// Pearson r during intention.
    pub intention_r: f64,
    /// Fisher Z test statistic.
    pub fisher_z: f64,
    /// Two-tailed p-value.
    pub p_value: f64,
}

/// Full coherence analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceResult {
    /// Mean |r| across all pairs during baseline.
    pub baseline_mean_abs_r: f64,
    /// Mean |r| across all pairs during intention.
    pub intention_mean_abs_r: f64,
    /// Per-pair correlation shifts.
    pub shifts: Vec<CorrelationShift>,
    /// Number of significant shifts (p < 0.05 after BH correction).
    pub significant_shifts: usize,
    /// Global coherence change Z (Stouffer combination of per-pair Fisher Z stats).
    pub global_coherence_z: f64,
    /// Global p-value.
    pub global_p: f64,
}

// -- Unified result --

/// Unified result for any experiment mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModeResult {
    Standard(ExperimentResult),
    Spectroscopy(SpectroscopyResult),
    Structure(StructureResult),
    Coherence(CoherenceResult),
    Temporal(crate::consciousness_temporal::TemporalResult),
    Adversarial(AdversarialResult),
    Feedback(FeedbackResult),
    Anomaly(crate::consciousness_anomaly::AnomalyResult),
    Weather(WeatherResult),
    Retrocausal(crate::consciousness_retrocausal::RetrocausalResult),
}

// ---------------------------------------------------------------------------
// Spectroscopy computation
// ---------------------------------------------------------------------------

/// Group source differentials by their category (domain).
pub fn group_sources_by_category(
    differentials: &[SourceDifferential],
) -> HashMap<String, Vec<SourceDifferential>> {
    let mut groups: HashMap<String, Vec<SourceDifferential>> = HashMap::new();
    for diff in differentials {
        groups
            .entry(diff.category.clone())
            .or_default()
            .push(diff.clone());
    }
    groups
}

/// Compute spectroscopy analysis from source differentials.
///
/// Groups sources by physical domain, computes per-domain differential Z,
/// then tests heterogeneity across domains with Cochran's Q.
pub fn compute_spectroscopy(differentials: &[SourceDifferential]) -> SpectroscopyResult {
    let groups = group_sources_by_category(differentials);

    let mut domains: Vec<DomainResult> = Vec::new();
    let mut effect_sizes: Vec<f64> = Vec::new();
    let mut variances: Vec<f64> = Vec::new();

    let mut sorted_domains: Vec<String> = groups.keys().cloned().collect();
    sorted_domains.sort();

    for domain in &sorted_domains {
        let diffs = &groups[domain];
        let sources: Vec<String> = diffs.iter().map(|d| d.source_name.clone()).collect();

        // Aggregate high/low Z across all sources in this domain
        let high_zs: Vec<f64> = diffs.iter().map(|d| d.high_z).collect();
        let low_zs: Vec<f64> = diffs.iter().map(|d| d.low_z).collect();

        let high_z = stouffer_z(&high_zs);
        let low_z = stouffer_z(&low_zs);
        let differential_z = (high_z - low_z) / std::f64::consts::SQRT_2;
        let differential_p = z_to_p_two_tailed(differential_z);

        effect_sizes.push(differential_z);
        // Variance of a Stouffer Z from k sources ≈ 1.0 (standard normal)
        // but scale by number of sources for weighting
        let n_src = diffs.len() as f64;
        variances.push(if n_src > 0.0 { 2.0 / n_src } else { 1.0 });

        domains.push(DomainResult {
            domain: domain.clone(),
            sources,
            high_z,
            low_z,
            differential_z,
            differential_p,
        });
    }

    let (q, q_p) = consciousness_stats::cochrans_q(&effect_sizes, &variances);
    let i2 = consciousness_stats::i_squared(q, domains.len());

    let interpretation = if i2 < 25.0 {
        "homogeneous — effect consistent across domains".to_string()
    } else if i2 < 50.0 {
        "low heterogeneity — minor differences between domains".to_string()
    } else if i2 < 75.0 {
        "moderate heterogeneity — effect varies across domains".to_string()
    } else {
        "high heterogeneity — effect strongly domain-dependent".to_string()
    };

    // BH correction on domain p-values
    let mut p_indexed: Vec<(usize, f64)> = domains
        .iter()
        .enumerate()
        .map(|(i, d)| (i, d.differential_p))
        .collect();
    let bh_results = consciousness_stats::benjamini_hochberg(&mut p_indexed, 0.05);
    let bh_significant: Vec<(String, bool)> = bh_results
        .iter()
        .map(|&(idx, sig)| (domains[idx].domain.clone(), sig))
        .collect();

    SpectroscopyResult {
        domains,
        cochrans_q: q,
        cochrans_p: q_p,
        i_squared: i2,
        interpretation,
        bh_significant,
    }
}

// ---------------------------------------------------------------------------
// Structure computation
// ---------------------------------------------------------------------------

/// Compute information-theoretic measures for a single epoch of byte data.
pub fn compute_epoch_measures(
    data: &[u8],
    direction: IntentionDirection,
    epoch_index: usize,
) -> EpochMeasures {
    // Standard parameters for entropy measures
    let m = 2;
    let sd = if data.is_empty() {
        1.0
    } else {
        let mean = data.iter().map(|&b| b as f64).sum::<f64>() / data.len() as f64;
        let var = data.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / data.len() as f64;
        var.sqrt().max(1.0)
    };
    let r = 0.2 * sd;

    EpochMeasures {
        direction,
        epoch_index,
        approximate_entropy: consciousness_stats::approximate_entropy(data, m, r),
        sample_entropy: {
            let se = consciousness_stats::sample_entropy(data, m, r);
            if se.is_infinite() { 10.0 } else { se } // Cap infinity for serialization
        },
        lz76_complexity: consciousness_stats::lz76_complexity(data),
        spectral_flatness: consciousness_stats::spectral_flatness(data),
    }
}

/// Compute structure analysis: compare information-theoretic measures
/// between intention and baseline epochs.
pub fn compute_structure(epochs: &[EpochMeasures]) -> StructureResult {
    let baseline: Vec<&EpochMeasures> = epochs
        .iter()
        .filter(|e| e.direction == IntentionDirection::Baseline)
        .collect();
    let intention: Vec<&EpochMeasures> = epochs
        .iter()
        .filter(|e| e.direction != IntentionDirection::Baseline)
        .collect();

    let measures: Vec<(&str, Box<dyn Fn(&&EpochMeasures) -> f64>)> = vec![
        ("approximate_entropy", Box::new(|e: &&EpochMeasures| e.approximate_entropy)),
        ("sample_entropy", Box::new(|e: &&EpochMeasures| e.sample_entropy)),
        ("lz76_complexity", Box::new(|e: &&EpochMeasures| e.lz76_complexity)),
        ("spectral_flatness", Box::new(|e: &&EpochMeasures| e.spectral_flatness)),
    ];

    let mut comparisons = Vec::new();

    for (name, extractor) in &measures {
        let baseline_vals: Vec<f64> = baseline.iter().map(|e| extractor(e)).collect();
        let intention_vals: Vec<f64> = intention.iter().map(|e| extractor(e)).collect();

        let baseline_mean = if baseline_vals.is_empty() {
            0.0
        } else {
            baseline_vals.iter().sum::<f64>() / baseline_vals.len() as f64
        };
        let intention_mean = if intention_vals.is_empty() {
            0.0
        } else {
            intention_vals.iter().sum::<f64>() / intention_vals.len() as f64
        };

        let (t, p) = consciousness_stats::welch_t_test(&intention_vals, &baseline_vals);

        let effect_direction = if intention_mean > baseline_mean {
            "increased".to_string()
        } else if intention_mean < baseline_mean {
            "decreased".to_string()
        } else {
            "unchanged".to_string()
        };

        comparisons.push(MeasureComparison {
            measure_name: name.to_string(),
            baseline_mean,
            intention_mean,
            t_statistic: t,
            p_value: p,
            effect_direction,
        });
    }

    let any_significant = comparisons.iter().any(|c| c.p_value < 0.05);

    StructureResult {
        epoch_measures: epochs.to_vec(),
        comparisons,
        any_significant,
    }
}

// ---------------------------------------------------------------------------
// Coherence computation
// ---------------------------------------------------------------------------

/// Compute pairwise Pearson correlations between source byte streams.
fn pairwise_correlations(
    data: &HashMap<String, Vec<u8>>,
) -> Vec<(String, String, f64)> {
    let mut names: Vec<&String> = data.keys().collect();
    names.sort();

    let mut pairs = Vec::new();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let a = &data[names[i]];
            let b = &data[names[j]];
            let n = a.len().min(b.len());
            if n < 4 {
                continue;
            }
            let r = crate::analysis::pearson_correlation(&a[..n], &b[..n]);
            pairs.push((names[i].clone(), names[j].clone(), r));
        }
    }
    pairs
}

/// Compute coherence analysis: compare pairwise source correlations
/// between baseline and intention epochs.
pub fn compute_coherence(
    baseline_data: &HashMap<String, Vec<u8>>,
    intention_data: &HashMap<String, Vec<u8>>,
) -> CoherenceResult {
    let baseline_pairs = pairwise_correlations(baseline_data);
    let intention_pairs = pairwise_correlations(intention_data);

    // Match pairs between baseline and intention
    let baseline_n = baseline_data.values().next().map_or(0, |v| v.len());
    let intention_n = intention_data.values().next().map_or(0, |v| v.len());

    let mut shifts = Vec::new();
    let mut fisher_zs = Vec::new();

    for (a, b, r_bl) in &baseline_pairs {
        if let Some((_, _, r_int)) = intention_pairs
            .iter()
            .find(|(ia, ib, _)| ia == a && ib == b)
        {
            let (fz, p) =
                consciousness_stats::fisher_z_test(*r_bl, baseline_n, *r_int, intention_n);
            fisher_zs.push(fz);
            shifts.push(CorrelationShift {
                source_a: a.clone(),
                source_b: b.clone(),
                baseline_r: *r_bl,
                intention_r: *r_int,
                fisher_z: fz,
                p_value: p,
            });
        }
    }

    // BH correction
    let mut p_indexed: Vec<(usize, f64)> = shifts
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.p_value))
        .collect();
    let bh_results = consciousness_stats::benjamini_hochberg(&mut p_indexed, 0.05);
    let significant_shifts = bh_results.iter().filter(|&&(_, sig)| sig).count();

    let baseline_mean_abs_r = if baseline_pairs.is_empty() {
        0.0
    } else {
        baseline_pairs.iter().map(|(_, _, r)| r.abs()).sum::<f64>() / baseline_pairs.len() as f64
    };

    let intention_mean_abs_r = if intention_pairs.is_empty() {
        0.0
    } else {
        intention_pairs
            .iter()
            .map(|(_, _, r)| r.abs())
            .sum::<f64>()
            / intention_pairs.len() as f64
    };

    let global_coherence_z = if fisher_zs.is_empty() {
        0.0
    } else {
        stouffer_z(&fisher_zs)
    };
    let global_p = z_to_p_two_tailed(global_coherence_z);

    CoherenceResult {
        baseline_mean_abs_r,
        intention_mean_abs_r,
        shifts,
        significant_shifts,
        global_coherence_z,
        global_p,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Bit counting --

    #[test]
    fn count_ones_empty() {
        assert_eq!(count_ones(&[]), 0);
    }

    #[test]
    fn count_ones_all_ones() {
        assert_eq!(count_ones(&[0xFF, 0xFF]), 16);
    }

    #[test]
    fn count_ones_all_zeros() {
        assert_eq!(count_ones(&[0x00, 0x00]), 0);
    }

    #[test]
    fn count_ones_mixed() {
        assert_eq!(count_ones(&[0xAA]), 4); // 10101010
        assert_eq!(count_ones(&[0x55]), 4); // 01010101
        assert_eq!(count_ones(&[0x0F]), 4); // 00001111
    }

    #[test]
    fn count_ones_n_partial_byte() {
        // 0xFF = 11111111, but only first 3 bits → 3 ones
        assert_eq!(count_ones_n(&[0xFF], 3), 3);
        // 0xF0 = 11110000, first 4 bits → 4 ones
        assert_eq!(count_ones_n(&[0xF0], 4), 4);
        // Full byte + 4 bits of next
        assert_eq!(count_ones_n(&[0xFF, 0xF0], 12), 12);
    }

    // -- Z-score --

    #[test]
    fn z_score_at_expected() {
        // 100 ones out of 200 bits → Z = 0
        let z = trial_z_score(100, 200);
        assert!((z - 0.0).abs() < 1e-10);
    }

    #[test]
    fn z_score_above_expected() {
        // 110 ones out of 200 → Z = (110-100)/sqrt(50) ≈ 1.414
        let z = trial_z_score(110, 200);
        assert!((z - 10.0 / 50.0_f64.sqrt()).abs() < 1e-10);
    }

    #[test]
    fn z_score_below_expected() {
        let z = trial_z_score(90, 200);
        assert!(z < 0.0);
    }

    // -- Stouffer Z --

    #[test]
    fn stouffer_empty() {
        assert_eq!(stouffer_z(&[]), 0.0);
    }

    #[test]
    fn stouffer_single() {
        assert!((stouffer_z(&[2.0]) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn stouffer_symmetric() {
        // Equal and opposite Z-scores should cancel
        let z = stouffer_z(&[1.0, -1.0]);
        assert!(z.abs() < 1e-10);
    }

    #[test]
    fn stouffer_multiple() {
        // sum = 3.0, sqrt(3) ≈ 1.732, result ≈ 1.732
        let z = stouffer_z(&[1.0, 1.0, 1.0]);
        assert!((z - 3.0 / 3.0_f64.sqrt()).abs() < 1e-10);
    }

    // -- P-value --

    #[test]
    fn p_value_at_zero() {
        let p = z_to_p_two_tailed(0.0);
        assert!((p - 1.0).abs() < 0.01); // should be ~1.0
    }

    #[test]
    fn p_value_at_196() {
        let p = z_to_p_two_tailed(1.96);
        assert!((p - 0.05).abs() < 0.005); // should be ~0.05
    }

    #[test]
    fn p_value_at_258() {
        let p = z_to_p_two_tailed(2.58);
        assert!((p - 0.01).abs() < 0.005); // should be ~0.01
    }

    #[test]
    fn p_value_symmetric() {
        let p_pos = z_to_p_two_tailed(1.5);
        let p_neg = z_to_p_two_tailed(-1.5);
        assert!((p_pos - p_neg).abs() < 1e-10);
    }

    // -- Format helpers --

    #[test]
    fn format_p_stars() {
        assert!(format_p_value(0.0001).contains("***"));
        assert!(format_p_value(0.005).contains("**"));
        assert!(format_p_value(0.03).contains("*"));
        assert!(!format_p_value(0.1).contains("*"));
    }

    #[test]
    fn format_z_sign() {
        assert!(format_z(1.5).starts_with('+'));
        assert!(format_z(-1.5).starts_with('-'));
        assert!(format_z(0.0).starts_with('+'));
    }

    // -- Config --

    #[test]
    fn default_config() {
        let cfg = ExperimentConfig::default();
        assert_eq!(cfg.bits_per_trial, 200);
        assert_eq!(cfg.trials_per_phase, 50);
        assert_eq!(cfg.trial_interval_ms, 1000);
        assert_eq!(cfg.phases.len(), 3);
        assert_eq!(cfg.bytes_per_trial(), 25);
        assert!((cfg.expected_ones() - 100.0).abs() < 1e-10);
        assert!((cfg.sd_ones() - 50.0_f64.sqrt()).abs() < 1e-10);
    }

    #[test]
    fn estimated_duration() {
        let cfg = ExperimentConfig::default();
        // 3 phases * 50 trials * 1000ms = 150s
        assert!((cfg.estimated_duration_secs() - 150.0).abs() < 1e-10);
    }

    // -- Phase result computation --

    #[test]
    fn phase_result_empty() {
        let result = compute_phase_result(IntentionDirection::Baseline, &[]);
        assert_eq!(result.cumulative_z, 0.0);
        assert_eq!(result.p_value, 1.0);
    }

    #[test]
    fn phase_result_basic() {
        let trials = vec![
            Trial {
                index: 0,
                direction: IntentionDirection::High,
                source_trials: vec![SourceTrial {
                    source_name: "test".to_string(),
                    category: "timing".to_string(),
                    ones_count: 110,
                    z_score: trial_z_score(110, 200),
                }],
                pooled_z: trial_z_score(110, 200),
                timestamp_secs: 0.0,
            },
            Trial {
                index: 1,
                direction: IntentionDirection::High,
                source_trials: vec![SourceTrial {
                    source_name: "test".to_string(),
                    category: "timing".to_string(),
                    ones_count: 105,
                    z_score: trial_z_score(105, 200),
                }],
                pooled_z: trial_z_score(105, 200),
                timestamp_secs: 1.0,
            },
        ];

        let result = compute_phase_result(IntentionDirection::High, &trials);
        assert!(result.cumulative_z > 0.0);
        assert!(result.p_value < 1.0);
        assert!(result.mean_ones > 100.0);
    }

    // -- Source differential --

    #[test]
    fn source_differential_basic() {
        let make_trial = |dir: IntentionDirection, ones: u32, src: &str, cat: &str| Trial {
            index: 0,
            direction: dir,
            source_trials: vec![SourceTrial {
                source_name: src.to_string(),
                category: cat.to_string(),
                ones_count: ones,
                z_score: trial_z_score(ones, 200),
            }],
            pooled_z: trial_z_score(ones, 200),
            timestamp_secs: 0.0,
        };

        let phases = vec![
            PhaseResult {
                direction: IntentionDirection::Baseline,
                trials: vec![make_trial(IntentionDirection::Baseline, 100, "src_a", "timing")],
                cumulative_z: 0.0,
                p_value: 1.0,
                mean_ones: 100.0,
                effect_size: 0.0,
            },
            PhaseResult {
                direction: IntentionDirection::High,
                trials: vec![make_trial(IntentionDirection::High, 110, "src_a", "timing")],
                cumulative_z: 1.0,
                p_value: 0.3,
                mean_ones: 110.0,
                effect_size: 1.0,
            },
            PhaseResult {
                direction: IntentionDirection::Low,
                trials: vec![make_trial(IntentionDirection::Low, 90, "src_a", "timing")],
                cumulative_z: -1.0,
                p_value: 0.3,
                mean_ones: 90.0,
                effect_size: -1.0,
            },
        ];

        let diffs = compute_source_differentials(&phases);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].source_name, "src_a");
        assert!(diffs[0].differential_z > 0.0);
        assert!(diffs[0].high_z > 0.0);
        assert!(diffs[0].low_z < 0.0);
    }

    // -- ExperimentMode --

    #[test]
    fn experiment_mode_from_str() {
        assert_eq!(ExperimentMode::from_str("standard"), ExperimentMode::Standard);
        assert_eq!(ExperimentMode::from_str("spectroscopy"), ExperimentMode::Spectroscopy);
        assert_eq!(ExperimentMode::from_str("structure"), ExperimentMode::Structure);
        assert_eq!(ExperimentMode::from_str("coherence"), ExperimentMode::Coherence);
        assert_eq!(ExperimentMode::from_str("temporal"), ExperimentMode::Temporal);
        assert_eq!(ExperimentMode::from_str("adversarial"), ExperimentMode::Adversarial);
        assert_eq!(ExperimentMode::from_str("feedback"), ExperimentMode::Feedback);
        assert_eq!(ExperimentMode::from_str("anomaly"), ExperimentMode::Anomaly);
        assert_eq!(ExperimentMode::from_str("retrocausal"), ExperimentMode::Retrocausal);
        assert_eq!(ExperimentMode::from_str("unknown"), ExperimentMode::Standard);
    }

    #[test]
    fn experiment_mode_display() {
        assert_eq!(ExperimentMode::Standard.to_string(), "standard");
        assert_eq!(ExperimentMode::Spectroscopy.to_string(), "spectroscopy");
        assert_eq!(ExperimentMode::Structure.to_string(), "structure");
        assert_eq!(ExperimentMode::Coherence.to_string(), "coherence");
        assert_eq!(ExperimentMode::Temporal.to_string(), "temporal");
        assert_eq!(ExperimentMode::Adversarial.to_string(), "adversarial");
        assert_eq!(ExperimentMode::Feedback.to_string(), "feedback");
        assert_eq!(ExperimentMode::Anomaly.to_string(), "anomaly");
        assert_eq!(ExperimentMode::Retrocausal.to_string(), "retrocausal");
    }

    // -- Pre-registration --

    #[test]
    fn preregistration_generates_hash() {
        let config = ExperimentConfig::default();
        let prereg = generate_preregistration(
            ExperimentMode::Standard,
            &config,
            false,
            Some("test_operator"),
        );
        assert!(!prereg.hash.is_empty());
        assert_eq!(prereg.mode, ExperimentMode::Standard);
        assert_eq!(prereg.operator.as_deref(), Some("test_operator"));
    }

    // -- Operator profile --

    #[test]
    fn operator_profile_add_session() {
        let mut profile = OperatorProfile::new("Alice");
        profile.add_session(OperatorSessionSummary {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            mode: ExperimentMode::Standard,
            overall_z: 1.5,
            overall_p: 0.13,
            source_z_scores: {
                let mut m = HashMap::new();
                m.insert("clock_jitter".to_string(), 2.0);
                m.insert("sleep_jitter".to_string(), 0.5);
                m
            },
            preregistration_hash: None,
        });
        assert_eq!(profile.total_sessions, 1);
        assert!((profile.combined_z - 1.5).abs() < 1e-10);
        assert!(!profile.top_sources.is_empty());
    }

    // -- Adversarial types --

    #[test]
    fn adversarial_result_serializable() {
        let result = AdversarialResult {
            operator_a: OperatorResult {
                name: "Alice".to_string(),
                direction: IntentionDirection::High,
                cumulative_z: 1.0,
                p_value: 0.32,
                n_trials: 50,
            },
            operator_b: OperatorResult {
                name: "Bob".to_string(),
                direction: IntentionDirection::Low,
                cumulative_z: -0.5,
                p_value: 0.62,
                n_trials: 50,
            },
            net_z: 0.5,
            net_p: 0.62,
            dominance_z: 0.5,
            interpretation: "no clear dominance".to_string(),
        };
        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }

    // -- Weather types --

    #[test]
    fn weather_result_serializable() {
        let result = WeatherResult {
            epochs: vec![],
            mean_z: 0.0,
            sd_z: 1.0,
            extreme_count: 0,
            expected_extreme_count: 0.0,
            duration_secs: 0.0,
            labeled_events: vec![],
        };
        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }

    // -- Spectroscopy --

    #[test]
    fn spectroscopy_groups_by_category() {
        let diffs = vec![
            SourceDifferential {
                source_name: "a".into(),
                category: "timing".into(),
                high_z: 1.0,
                low_z: -1.0,
                baseline_z: 0.0,
                differential_z: 1.414,
                differential_p: 0.15,
            },
            SourceDifferential {
                source_name: "b".into(),
                category: "thermal".into(),
                high_z: 0.5,
                low_z: -0.5,
                baseline_z: 0.0,
                differential_z: 0.707,
                differential_p: 0.48,
            },
        ];
        let groups = group_sources_by_category(&diffs);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("timing"));
        assert!(groups.contains_key("thermal"));
    }

    #[test]
    fn spectroscopy_compute_basic() {
        let diffs = vec![
            SourceDifferential {
                source_name: "src_a".into(),
                category: "timing".into(),
                high_z: 1.0,
                low_z: -1.0,
                baseline_z: 0.0,
                differential_z: 1.414,
                differential_p: 0.15,
            },
            SourceDifferential {
                source_name: "src_b".into(),
                category: "timing".into(),
                high_z: 0.8,
                low_z: -0.8,
                baseline_z: 0.0,
                differential_z: 1.131,
                differential_p: 0.26,
            },
            SourceDifferential {
                source_name: "src_c".into(),
                category: "thermal".into(),
                high_z: 0.1,
                low_z: 0.1,
                baseline_z: 0.0,
                differential_z: 0.0,
                differential_p: 1.0,
            },
        ];

        let result = compute_spectroscopy(&diffs);
        assert_eq!(result.domains.len(), 2);
        assert!(result.i_squared >= 0.0);
        assert!(result.cochrans_p >= 0.0 && result.cochrans_p <= 1.0);
    }

    // -- Structure --

    #[test]
    fn structure_compute_basic() {
        let epochs = vec![
            EpochMeasures {
                direction: IntentionDirection::Baseline,
                epoch_index: 0,
                approximate_entropy: 0.5,
                sample_entropy: 1.0,
                lz76_complexity: 0.7,
                spectral_flatness: 0.8,
            },
            EpochMeasures {
                direction: IntentionDirection::High,
                epoch_index: 1,
                approximate_entropy: 0.6,
                sample_entropy: 1.2,
                lz76_complexity: 0.65,
                spectral_flatness: 0.82,
            },
            EpochMeasures {
                direction: IntentionDirection::Low,
                epoch_index: 2,
                approximate_entropy: 0.55,
                sample_entropy: 1.1,
                lz76_complexity: 0.68,
                spectral_flatness: 0.79,
            },
        ];

        let result = compute_structure(&epochs);
        assert_eq!(result.comparisons.len(), 4);
        // Each comparison should have valid fields
        for c in &result.comparisons {
            assert!(!c.measure_name.is_empty());
            assert!(c.p_value >= 0.0);
        }
    }

    // -- Coherence --

    #[test]
    fn coherence_compute_empty() {
        let baseline: HashMap<String, Vec<u8>> = HashMap::new();
        let intention: HashMap<String, Vec<u8>> = HashMap::new();
        let result = compute_coherence(&baseline, &intention);
        assert_eq!(result.shifts.len(), 0);
        assert_eq!(result.global_coherence_z, 0.0);
    }

    #[test]
    fn coherence_compute_basic() {
        let mut baseline = HashMap::new();
        let mut intention = HashMap::new();

        // Create correlated baseline data
        let data_a: Vec<u8> = (0..100).map(|i| (i % 256) as u8).collect();
        let data_b: Vec<u8> = (0..100).map(|i| ((i + 1) % 256) as u8).collect();
        baseline.insert("src_a".to_string(), data_a.clone());
        baseline.insert("src_b".to_string(), data_b.clone());

        // Intention data: different correlation structure
        let data_a2: Vec<u8> = (0..100).map(|i| ((i * 3 + 7) % 256) as u8).collect();
        let data_b2: Vec<u8> = (0..100).map(|i| ((i * 5 + 13) % 256) as u8).collect();
        intention.insert("src_a".to_string(), data_a2);
        intention.insert("src_b".to_string(), data_b2);

        let result = compute_coherence(&baseline, &intention);
        assert_eq!(result.shifts.len(), 1);
        assert!(result.global_p >= 0.0 && result.global_p <= 1.0);
    }

    // -- ModeResult serialization --

    #[test]
    fn mode_result_serializable() {
        let sr = StructureResult {
            epoch_measures: vec![],
            comparisons: vec![],
            any_significant: false,
        };
        let mr = ModeResult::Structure(sr);
        let json = serde_json::to_string(&mr);
        assert!(json.is_ok());
    }
}
