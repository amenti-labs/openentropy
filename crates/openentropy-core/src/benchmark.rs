use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::conditioning::{ConditioningMode, quick_min_entropy, quick_shannon};
use crate::grade_min_entropy;
use crate::pool::{EntropyPool, SourceHealth};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchConfig {
    pub samples_per_round: usize,
    pub rounds: usize,
    pub warmup_rounds: usize,
    pub timeout_sec: f64,
    pub rank_by: RankBy,
    pub include_pool_quality: bool,
    pub pool_quality_bytes: usize,
    #[serde(with = "serde_conditioning_mode")]
    pub conditioning: ConditioningMode,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            samples_per_round: 2048,
            rounds: 3,
            warmup_rounds: 1,
            timeout_sec: 2.0,
            rank_by: RankBy::Balanced,
            include_pool_quality: true,
            pool_quality_bytes: 65_536,
            conditioning: ConditioningMode::Sha256,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchReport {
    pub generated_unix: u64,
    pub config: BenchConfig,
    pub sources: Vec<BenchSourceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolQualityReport>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchSourceReport {
    pub name: String,
    pub composite: bool,
    pub healthy: bool,
    pub success_rounds: usize,
    pub failures: u64,
    pub avg_shannon: f64,
    pub avg_min_entropy: f64,
    pub avg_throughput_bps: f64,
    pub avg_autocorrelation: f64,
    pub p99_latency_ms: f64,
    pub stability: f64,
    pub grade: char,
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PoolQualityReport {
    pub bytes: usize,
    pub shannon_entropy: f64,
    pub min_entropy: f64,
    pub healthy_sources: usize,
    pub total_sources: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RankBy {
    Balanced,
    MinEntropy,
    Throughput,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum BenchError {
    InvalidConfig(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid benchmark config: {msg}"),
        }
    }
}

impl std::error::Error for BenchError {}

#[derive(Default)]
struct SourceAccumulator {
    success_rounds: usize,
    failures: u64,
    shannon_sum: f64,
    min_entropy_sum: f64,
    throughput_sum: f64,
    autocorrelation_sum: f64,
    min_entropy_values: Vec<f64>,
    collection_times_ms: Vec<f64>,
}

#[derive(Clone)]
struct BenchRow {
    name: String,
    composite: bool,
    success_rounds: usize,
    failures: u64,
    avg_shannon: f64,
    avg_min_entropy: f64,
    avg_throughput_bps: f64,
    avg_autocorrelation: f64,
    p99_latency_ms: f64,
    stability: f64,
    score: f64,
}

pub fn benchmark_sources(
    pool: &EntropyPool,
    config: &BenchConfig,
) -> Result<BenchReport, BenchError> {
    validate_config(config)?;

    let infos = pool.source_infos();

    for _ in 0..config.warmup_rounds {
        let _ = pool.collect_all_parallel_n(config.timeout_sec, config.samples_per_round);
    }

    let mut prev = snapshot_counters(&pool.health_report().sources);
    let mut accum: HashMap<String, SourceAccumulator> = HashMap::new();

    for _ in 0..config.rounds {
        let _ = pool.collect_all_parallel_n(config.timeout_sec, config.samples_per_round);
        let health = pool.health_report();

        for src in &health.sources {
            let (prev_bytes, prev_failures) = prev
                .get(&src.name)
                .copied()
                .unwrap_or((src.bytes, src.failures));
            let bytes_delta = src.bytes.saturating_sub(prev_bytes);
            let failures_delta = src.failures.saturating_sub(prev_failures);

            let entry = accum.entry(src.name.clone()).or_default();
            entry.failures += failures_delta;

            if bytes_delta > 0 {
                entry.success_rounds += 1;
                entry.shannon_sum += src.entropy;
                entry.min_entropy_sum += src.min_entropy;
                entry.autocorrelation_sum += src.autocorrelation;
                entry.min_entropy_values.push(src.min_entropy);
                entry.collection_times_ms.push(src.time * 1000.0);
                if src.time > 0.0 {
                    entry.throughput_sum += bytes_delta as f64 / src.time;
                }
            }

            prev.insert(src.name.clone(), (src.bytes, src.failures));
        }
    }

    let mut rows: Vec<BenchRow> = infos
        .iter()
        .map(|info| {
            let (
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                avg_autocorrelation,
                p99_latency_ms,
                stability,
            ) = if let Some(src_acc) = accum.get(&info.name) {
                let success_rounds = src_acc.success_rounds;
                if success_rounds > 0 {
                    let n = success_rounds as f64;
                    (
                        success_rounds,
                        src_acc.failures,
                        src_acc.shannon_sum / n,
                        src_acc.min_entropy_sum / n,
                        src_acc.throughput_sum / n,
                        src_acc.autocorrelation_sum / n,
                        percentile(&src_acc.collection_times_ms, 99.0),
                        stability_index(&src_acc.min_entropy_values),
                    )
                } else {
                    (0, src_acc.failures, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            } else {
                (0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            };

            BenchRow {
                name: info.name.clone(),
                composite: info.composite,
                success_rounds,
                failures,
                avg_shannon,
                avg_min_entropy,
                avg_throughput_bps,
                avg_autocorrelation,
                p99_latency_ms,
                stability,
                score: 0.0,
            }
        })
        .collect();

    let max_throughput = rows
        .iter()
        .map(|r| r.avg_throughput_bps)
        .fold(0.0_f64, f64::max);

    for row in &mut rows {
        row.score = score_row(config.rank_by, row, max_throughput, config.rounds);
    }

    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let sources = rows
        .iter()
        .map(|row| BenchSourceReport {
            name: row.name.clone(),
            composite: row.composite,
            healthy: row.avg_min_entropy > 1.0 && row.failures == 0,
            success_rounds: row.success_rounds,
            failures: row.failures,
            avg_shannon: row.avg_shannon,
            avg_min_entropy: row.avg_min_entropy,
            avg_throughput_bps: row.avg_throughput_bps,
            avg_autocorrelation: row.avg_autocorrelation,
            p99_latency_ms: row.p99_latency_ms,
            stability: row.stability,
            grade: grade_min_entropy(row.avg_min_entropy.max(0.0)),
            score: row.score,
        })
        .collect();

    let pool = if config.include_pool_quality {
        let output = pool.get_bytes(config.pool_quality_bytes, config.conditioning);
        let health = pool.health_report();
        Some(PoolQualityReport {
            bytes: output.len(),
            shannon_entropy: quick_shannon(&output),
            min_entropy: quick_min_entropy(&output),
            healthy_sources: health.healthy,
            total_sources: health.total,
        })
    } else {
        None
    };

    Ok(BenchReport {
        generated_unix: unix_timestamp_now(),
        config: config.clone(),
        sources,
        pool,
    })
}

fn score_row(rank_by: RankBy, row: &BenchRow, max_throughput: f64, rounds: usize) -> f64 {
    let mut score = match rank_by {
        RankBy::MinEntropy => row.avg_min_entropy,
        RankBy::Throughput => row.avg_throughput_bps,
        RankBy::Balanced => {
            let min_h_term = (row.avg_min_entropy / 8.0).clamp(0.0, 1.0);
            let throughput_term = if max_throughput > 0.0 {
                (row.avg_throughput_bps / max_throughput).clamp(0.0, 1.0)
            } else {
                0.0
            };
            0.7 * min_h_term + 0.2 * throughput_term + 0.1 * row.stability
        }
    };

    let missed = rounds.saturating_sub(row.success_rounds) as f64;
    let total_issues = missed + row.failures as f64;
    let expected = rounds as f64;
    if total_issues > 0.0 && expected > 0.0 {
        let failure_rate = (total_issues / expected).clamp(0.0, 1.0);
        score *= 1.0 - 0.5 * failure_rate;
    }

    score
}

fn validate_config(config: &BenchConfig) -> Result<(), BenchError> {
    if config.samples_per_round == 0 {
        return Err(BenchError::InvalidConfig(
            "samples_per_round must be > 0".to_string(),
        ));
    }
    if config.rounds == 0 {
        return Err(BenchError::InvalidConfig("rounds must be > 0".to_string()));
    }
    if config.timeout_sec <= 0.0 {
        return Err(BenchError::InvalidConfig(
            "timeout_sec must be > 0".to_string(),
        ));
    }
    if config.include_pool_quality && config.pool_quality_bytes == 0 {
        return Err(BenchError::InvalidConfig(
            "pool_quality_bytes must be > 0 when include_pool_quality is true".to_string(),
        ));
    }
    Ok(())
}

fn snapshot_counters(sources: &[SourceHealth]) -> HashMap<String, (u64, u64)> {
    sources
        .iter()
        .map(|s| (s.name.clone(), (s.bytes, s.failures)))
        .collect()
}

fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p / 100.0 * sorted.len() as f64).ceil() as usize).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

fn stability_index(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return 1.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let stddev = var.sqrt();
    if mean.abs() < f64::EPSILON {
        return if stddev < f64::EPSILON { 1.0 } else { 0.0 };
    }
    let cv = (stddev / mean.abs()).min(1.0);
    (1.0 - cv).clamp(0.0, 1.0)
}

mod serde_conditioning_mode {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::conditioning::ConditioningMode;

    pub fn serialize<S>(mode: &ConditioningMode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match mode {
            ConditioningMode::Raw => "raw",
            ConditioningMode::VonNeumann => "von_neumann",
            ConditioningMode::Sha256 => "sha256",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ConditioningMode, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "raw" => Ok(ConditioningMode::Raw),
            "vonneumann" | "vn" | "von_neumann" => Ok(ConditioningMode::VonNeumann),
            "sha" | "sha256" => Ok(ConditioningMode::Sha256),
            _ => Err(serde::de::Error::custom(format!(
                "invalid conditioning mode '{s}', expected raw|vonneumann|vn|von_neumann|sha256"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, eps: f64) {
        assert!(
            (a - b).abs() <= eps,
            "values differ: left={a}, right={b}, eps={eps}"
        );
    }

    #[test]
    fn benchmark_default_config_matches_quick_profile() {
        let config = BenchConfig::default();
        assert_eq!(config.samples_per_round, 2048);
        assert_eq!(config.rounds, 3);
        assert_eq!(config.warmup_rounds, 1);
        approx_eq(config.timeout_sec, 2.0, f64::EPSILON);
        assert_eq!(config.rank_by, RankBy::Balanced);
        assert!(config.include_pool_quality);
        assert_eq!(config.pool_quality_bytes, 65_536);
        assert_eq!(config.conditioning, ConditioningMode::Sha256);
    }

    #[test]
    fn benchmark_percentile_uses_nearest_rank() {
        let values = [10.0, 50.0, 20.0, 40.0, 30.0];
        approx_eq(percentile(&values, 99.0), 50.0, f64::EPSILON);
        approx_eq(percentile(&values, 50.0), 30.0, f64::EPSILON);
        approx_eq(percentile(&[], 50.0), 0.0, f64::EPSILON);
    }

    #[test]
    fn benchmark_stability_index_expected_behavior() {
        approx_eq(stability_index(&[]), 0.0, f64::EPSILON);
        approx_eq(stability_index(&[4.2]), 1.0, f64::EPSILON);
        approx_eq(stability_index(&[2.0, 2.0, 2.0]), 1.0, 1e-12);
        let unstable = stability_index(&[0.5, 4.0, 7.5]);
        assert!(unstable < 0.7);
    }

    #[test]
    fn benchmark_scoring_math_balanced_with_reliability_penalty() {
        let row = BenchRow {
            name: "src".to_string(),
            composite: false,
            success_rounds: 8,
            failures: 1,
            avg_shannon: 0.0,
            avg_min_entropy: 4.0,
            avg_throughput_bps: 50.0,
            avg_autocorrelation: 0.0,
            p99_latency_ms: 0.0,
            stability: 0.8,
            score: 0.0,
        };
        let score = score_row(RankBy::Balanced, &row, 100.0, 10);
        approx_eq(score, 0.4505, 1e-12);
    }

    #[test]
    fn benchmark_scoring_math_min_entropy_and_throughput_modes() {
        let row = BenchRow {
            name: "src".to_string(),
            composite: false,
            success_rounds: 5,
            failures: 0,
            avg_shannon: 0.0,
            avg_min_entropy: 3.25,
            avg_throughput_bps: 1234.0,
            avg_autocorrelation: 0.0,
            p99_latency_ms: 0.0,
            stability: 0.0,
            score: 0.0,
        };
        approx_eq(score_row(RankBy::MinEntropy, &row, 5000.0, 5), 3.25, 1e-12);
        approx_eq(score_row(RankBy::Throughput, &row, 5000.0, 5), 1234.0, 1e-9);
    }

    #[test]
    fn benchmark_sources_runs_with_real_auto_pool() {
        let pool = EntropyPool::auto();
        let config = BenchConfig {
            samples_per_round: 64,
            rounds: 1,
            warmup_rounds: 0,
            timeout_sec: 0.5,
            rank_by: RankBy::Balanced,
            include_pool_quality: false,
            pool_quality_bytes: 64,
            conditioning: ConditioningMode::Sha256,
        };

        let report = benchmark_sources(&pool, &config).expect("benchmark should succeed");
        assert_eq!(report.sources.len(), pool.source_infos().len());
        for source in &report.sources {
            assert!(source.score.is_finite());
        }
    }
}
