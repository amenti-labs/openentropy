use serde::{Deserialize, Serialize};

use crate::analysis::{self, CrossCorrMatrix, SourceAnalysis};
use crate::chaos::{self, ChaosAnalysis};
use crate::conditioning::{self, MinEntropyReport};
use crate::trials::{self, TrialAnalysis, TrialConfig};
use crate::verdict::{self, Verdict};

/// Controls which analysis modules the dispatcher should run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Run full_analysis (autocorrelation, spectral, bias, distribution, stationarity, runs).
    pub forensic: bool,
    /// Run min_entropy_estimate (detailed entropy breakdown).
    pub entropy: bool,
    /// Run chaos_analysis (Hurst, Lyapunov, correlation dimension, BiEntropy, epiplexity).
    pub chaos: bool,
    /// Run trial_analysis with given config. None = skip.
    pub trials: Option<TrialConfig>,
    /// Run cross_correlation_matrix when 2+ sources present.
    pub cross_correlation: bool,
}

/// Analysis profile presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnalysisProfile {
    Quick,
    Standard,
    Deep,
    Security,
}

impl AnalysisProfile {
    pub fn to_config(self) -> AnalysisConfig {
        match self {
            Self::Quick => AnalysisConfig {
                forensic: true,
                entropy: false,
                chaos: false,
                trials: None,
                cross_correlation: false,
            },
            Self::Standard => AnalysisConfig {
                forensic: true,
                entropy: false,
                chaos: false,
                trials: None,
                cross_correlation: false,
            },
            Self::Deep => AnalysisConfig {
                forensic: true,
                entropy: true,
                chaos: true,
                trials: Some(TrialConfig::default()),
                cross_correlation: true,
            },
            Self::Security => AnalysisConfig {
                forensic: true,
                entropy: true,
                chaos: false,
                trials: None,
                cross_correlation: false,
            },
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "quick" => Self::Quick,
            "deep" => Self::Deep,
            "security" => Self::Security,
            _ => Self::Standard,
        }
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisProfile::Standard.to_config()
    }
}

/// Collected verdicts from all analyses that were run.
#[derive(Debug, Clone, Serialize)]
pub struct VerdictSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocorrelation: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectral: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bias: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stationarity: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hurst: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyapunov: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_dimension: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bientropy: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<Verdict>,
}

/// Per-source analysis results.
#[derive(Debug, Clone, Serialize)]
pub struct SourceReport {
    pub label: String,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forensic: Option<SourceAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entropy: Option<MinEntropyReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chaos: Option<ChaosAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trials: Option<TrialAnalysis>,
    pub verdicts: VerdictSummary,
}

/// Complete analysis report across all sources.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisReport {
    pub config: AnalysisConfig,
    pub sources: Vec<SourceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_correlation: Option<CrossCorrMatrix>,
}

/// Run selected analyses on one or more data sources.
///
/// This is the unified entry point - CLI, Python, and HTTP all call this.
/// Individual analysis functions remain available for fine-grained control.
pub fn analyze(sources: &[(&str, &[u8])], config: &AnalysisConfig) -> AnalysisReport {
    let mut source_reports = Vec::with_capacity(sources.len());

    for &(label, data) in sources {
        let forensic = if config.forensic {
            Some(analysis::full_analysis(label, data))
        } else {
            None
        };

        let entropy = if config.entropy {
            Some(conditioning::min_entropy_estimate(data))
        } else {
            None
        };

        let chaos = if config.chaos {
            Some(chaos::chaos_analysis(data))
        } else {
            None
        };

        let trials = config
            .trials
            .as_ref()
            .map(|tc| trials::trial_analysis(data, tc));

        let verdicts = build_verdicts(forensic.as_ref(), chaos.as_ref());

        source_reports.push(SourceReport {
            label: label.to_string(),
            size: data.len(),
            forensic,
            entropy,
            chaos,
            trials,
            verdicts,
        });
    }

    let cross_correlation = if config.cross_correlation && sources.len() >= 2 {
        let sources_data: Vec<(String, Vec<u8>)> = sources
            .iter()
            .map(|&(label, data)| (label.to_string(), data.to_vec()))
            .collect();
        Some(analysis::cross_correlation_matrix(&sources_data))
    } else {
        None
    };

    AnalysisReport {
        config: config.clone(),
        sources: source_reports,
        cross_correlation,
    }
}

fn build_verdicts(
    forensic: Option<&SourceAnalysis>,
    chaos_result: Option<&ChaosAnalysis>,
) -> VerdictSummary {
    let (autocorrelation, spectral, bias, distribution, stationarity, runs) =
        if let Some(f) = forensic {
            (
                Some(verdict::verdict_autocorr(
                    f.autocorrelation.max_abs_correlation,
                )),
                Some(verdict::verdict_spectral(f.spectral.flatness)),
                Some(verdict::verdict_bias(
                    f.bit_bias.overall_bias,
                    f.bit_bias.has_significant_bias,
                )),
                Some(verdict::verdict_distribution(f.distribution.ks_p_value)),
                Some(verdict::verdict_stationarity(
                    f.stationarity.f_statistic,
                    f.stationarity.is_stationary,
                )),
                Some(verdict::verdict_runs(&f.runs, f.sample_size)),
            )
        } else {
            (None, None, None, None, None, None)
        };

    let (hurst, lyapunov, correlation_dimension, bientropy, compression) =
        if let Some(c) = chaos_result {
            (
                Some(verdict::verdict_hurst(c.hurst.hurst_exponent)),
                Some(verdict::verdict_lyapunov(c.lyapunov.lyapunov_exponent)),
                Some(verdict::verdict_corrdim(c.correlation_dimension.dimension)),
                Some(verdict::verdict_bientropy(c.bientropy.bien)),
                Some(verdict::verdict_compression(c.epiplexity.compression_ratio)),
            )
        } else {
            (None, None, None, None, None)
        };

    VerdictSummary {
        autocorrelation,
        spectral,
        bias,
        distribution,
        stationarity,
        runs,
        hurst,
        lyapunov,
        correlation_dimension,
        bientropy,
        compression,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_data() -> Vec<u8> {
        (0..1000).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn profile_quick_config() {
        let config = AnalysisProfile::Quick.to_config();
        assert!(config.forensic);
        assert!(!config.entropy);
        assert!(!config.chaos);
        assert!(config.trials.is_none());
        assert!(!config.cross_correlation);
    }

    #[test]
    fn profile_deep_config() {
        let config = AnalysisProfile::Deep.to_config();
        assert!(config.forensic);
        assert!(config.entropy);
        assert!(config.chaos);
        assert!(config.trials.is_some());
        assert!(config.cross_correlation);
    }

    #[test]
    fn profile_security_config() {
        let config = AnalysisProfile::Security.to_config();
        assert!(config.forensic);
        assert!(config.entropy);
        assert!(!config.chaos);
        assert!(config.trials.is_none());
        assert!(!config.cross_correlation);
    }

    #[test]
    fn profile_parse() {
        assert_eq!(AnalysisProfile::parse("quick"), AnalysisProfile::Quick);
        assert_eq!(AnalysisProfile::parse("DEEP"), AnalysisProfile::Deep);
        assert_eq!(
            AnalysisProfile::parse("security"),
            AnalysisProfile::Security
        );
        assert_eq!(AnalysisProfile::parse("unknown"), AnalysisProfile::Standard);
    }

    #[test]
    fn analyze_forensic_only() {
        let data = test_data();
        let config = AnalysisConfig {
            forensic: true,
            entropy: false,
            chaos: false,
            trials: None,
            cross_correlation: false,
        };
        let report = analyze(&[("test", &data)], &config);
        assert_eq!(report.sources.len(), 1);
        assert!(report.sources[0].forensic.is_some());
        assert!(report.sources[0].entropy.is_none());
        assert!(report.sources[0].chaos.is_none());
        assert!(report.sources[0].trials.is_none());
        assert!(report.cross_correlation.is_none());
        assert!(report.sources[0].verdicts.autocorrelation.is_some());
        assert!(report.sources[0].verdicts.hurst.is_none());
    }

    #[test]
    fn analyze_deep_profile() {
        let data = test_data();
        let config = AnalysisProfile::Deep.to_config();
        let report = analyze(&[("src_a", &data), ("src_b", &data)], &config);
        assert_eq!(report.sources.len(), 2);
        assert!(report.sources[0].forensic.is_some());
        assert!(report.sources[0].entropy.is_some());
        assert!(report.sources[0].chaos.is_some());
        assert!(report.sources[0].trials.is_some());
        assert!(report.cross_correlation.is_some());
        assert!(report.sources[0].verdicts.autocorrelation.is_some());
        assert!(report.sources[0].verdicts.hurst.is_some());
    }

    #[test]
    fn analyze_no_modules() {
        let data = test_data();
        let config = AnalysisConfig {
            forensic: false,
            entropy: false,
            chaos: false,
            trials: None,
            cross_correlation: false,
        };
        let report = analyze(&[("test", &data)], &config);
        assert!(report.sources[0].forensic.is_none());
        assert!(report.sources[0].entropy.is_none());
        assert!(report.sources[0].chaos.is_none());
        assert!(report.sources[0].trials.is_none());
        assert!(report.sources[0].verdicts.autocorrelation.is_none());
    }

    #[test]
    fn analyze_cross_correlation_needs_two_sources() {
        let data = test_data();
        let config = AnalysisConfig {
            forensic: false,
            entropy: false,
            chaos: false,
            trials: None,
            cross_correlation: true,
        };
        let report = analyze(&[("test", &data)], &config);
        assert!(report.cross_correlation.is_none());
        let report = analyze(&[("a", &data), ("b", &data)], &config);
        assert!(report.cross_correlation.is_some());
    }

    #[test]
    fn default_config_is_standard() {
        let config = AnalysisConfig::default();
        let standard = AnalysisProfile::Standard.to_config();
        assert_eq!(config.forensic, standard.forensic);
        assert_eq!(config.entropy, standard.entropy);
        assert_eq!(config.chaos, standard.chaos);
        assert_eq!(config.cross_correlation, standard.cross_correlation);
    }

    #[test]
    fn analysis_report_serializes() {
        let data = test_data();
        let config = AnalysisProfile::Quick.to_config();
        let report = analyze(&[("test", &data)], &config);
        let json = serde_json::to_string(&report).expect("should serialize");
        assert!(json.contains("\"forensic\""));
        // Quick profile: chaos analysis result should be absent from sources
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let source = &v["sources"][0];
        assert!(source.get("chaos").is_none());
        assert!(source.get("entropy").is_none());
        assert!(source.get("trials").is_none());
    }
}
