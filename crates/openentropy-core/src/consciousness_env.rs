//! Environmental correlation analysis for consciousness experiments.
//!
//! Implements Global Consciousness Project (GCP) methodology for testing
//! whether entropy source behavior correlates with external events.
//! Records timestamps alongside Z-scores to enable post-hoc correlation
//! with world events, local environmental conditions, etc.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Event types and markers
// ---------------------------------------------------------------------------

/// An environmental event marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalEvent {
    /// Event name/description.
    pub name: String,
    /// Event category.
    pub category: EventCategory,
    /// Timestamp (seconds since experiment start).
    pub timestamp_secs: f64,
    /// ISO 8601 wall-clock time.
    pub wall_time: String,
    /// Optional intensity/magnitude (1-10 scale).
    pub intensity: Option<f64>,
    /// Optional user notes.
    pub notes: Option<String>,
}

/// Categories of environmental events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    /// Major world events (news, disasters, elections).
    WorldEvent,
    /// Local environmental changes (weather, noise, light).
    LocalEnvironment,
    /// Operator state changes (meditation, emotional shift).
    OperatorState,
    /// Technical events (source availability change, system load).
    Technical,
    /// Group coherence events (multiple people focused).
    GroupCoherence,
    /// Custom/other.
    Custom,
}

impl std::fmt::Display for EventCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorldEvent => write!(f, "world"),
            Self::LocalEnvironment => write!(f, "local"),
            Self::OperatorState => write!(f, "operator"),
            Self::Technical => write!(f, "technical"),
            Self::GroupCoherence => write!(f, "group"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ---------------------------------------------------------------------------
// Timestamped Z-score record
// ---------------------------------------------------------------------------

/// A single Z-score measurement with full timestamp information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedZ {
    /// Index within the experiment.
    pub index: usize,
    /// Seconds since experiment start.
    pub elapsed_secs: f64,
    /// ISO 8601 wall-clock time.
    pub wall_time: String,
    /// Unix timestamp (seconds since epoch).
    pub unix_timestamp: f64,
    /// Pooled Z-score.
    pub z_score: f64,
    /// Cumulative Z at this point.
    pub cumulative_z: f64,
    /// Per-source Z-scores.
    pub source_z_scores: HashMap<String, f64>,
}

// ---------------------------------------------------------------------------
// Correlation analysis
// ---------------------------------------------------------------------------

/// Result of correlating Z-scores with an event window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCorrelation {
    /// Event that was analyzed.
    pub event: EnvironmentalEvent,
    /// Window size in seconds around the event.
    pub window_secs: f64,
    /// Number of Z-score measurements in the window.
    pub n_measurements: usize,
    /// Mean Z in the event window.
    pub window_mean_z: f64,
    /// Mean Z outside the event window.
    pub outside_mean_z: f64,
    /// Welch's t-test comparing window vs outside.
    pub t_statistic: f64,
    pub p_value: f64,
    /// Effect size (Cohen's d).
    pub effect_size: f64,
}

/// Full environmental correlation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentalReport {
    /// All timestamped Z-scores.
    pub z_scores: Vec<TimestampedZ>,
    /// All recorded events.
    pub events: Vec<EnvironmentalEvent>,
    /// Per-event correlations.
    pub correlations: Vec<EventCorrelation>,
    /// Global event-vs-non-event comparison.
    pub global_event_z: f64,
    pub global_non_event_z: f64,
    pub global_t: f64,
    pub global_p: f64,
    /// Number of events with significant correlations (p < 0.05).
    pub significant_events: usize,
}

/// Compute environmental correlations between Z-scores and events.
pub fn compute_environmental_correlations(
    z_scores: &[TimestampedZ],
    events: &[EnvironmentalEvent],
    window_secs: f64,
) -> EnvironmentalReport {
    if z_scores.is_empty() {
        return EnvironmentalReport {
            z_scores: Vec::new(),
            events: events.to_vec(),
            correlations: Vec::new(),
            global_event_z: 0.0,
            global_non_event_z: 0.0,
            global_t: 0.0,
            global_p: 1.0,
            significant_events: 0,
        };
    }

    let mut correlations = Vec::new();

    // For each event, find Z-scores within the window
    for event in events {
        let window_start = event.timestamp_secs - window_secs / 2.0;
        let window_end = event.timestamp_secs + window_secs / 2.0;

        let in_window: Vec<f64> = z_scores
            .iter()
            .filter(|z| z.elapsed_secs >= window_start && z.elapsed_secs <= window_end)
            .map(|z| z.z_score)
            .collect();

        let outside: Vec<f64> = z_scores
            .iter()
            .filter(|z| z.elapsed_secs < window_start || z.elapsed_secs > window_end)
            .map(|z| z.z_score)
            .collect();

        let window_mean_z = if in_window.is_empty() {
            0.0
        } else {
            in_window.iter().sum::<f64>() / in_window.len() as f64
        };

        let outside_mean_z = if outside.is_empty() {
            0.0
        } else {
            outside.iter().sum::<f64>() / outside.len() as f64
        };

        let (t, p) = if in_window.len() >= 2 && outside.len() >= 2 {
            crate::consciousness_stats::welch_t_test(&in_window, &outside)
        } else {
            (0.0, 1.0)
        };

        // Cohen's d
        let pooled_sd = pooled_standard_deviation(&in_window, &outside);
        let effect_size = if pooled_sd > 1e-10 {
            (window_mean_z - outside_mean_z) / pooled_sd
        } else {
            0.0
        };

        correlations.push(EventCorrelation {
            event: event.clone(),
            window_secs,
            n_measurements: in_window.len(),
            window_mean_z,
            outside_mean_z,
            t_statistic: t,
            p_value: p,
            effect_size,
        });
    }

    // Global comparison
    // Everything not in any event window
    let mut in_any_event = vec![false; z_scores.len()];
    for event in events {
        let window_start = event.timestamp_secs - window_secs / 2.0;
        let window_end = event.timestamp_secs + window_secs / 2.0;
        for (i, z) in z_scores.iter().enumerate() {
            if z.elapsed_secs >= window_start && z.elapsed_secs <= window_end {
                in_any_event[i] = true;
            }
        }
    }

    let global_event_zs: Vec<f64> = z_scores
        .iter()
        .enumerate()
        .filter(|(i, _)| in_any_event[*i])
        .map(|(_, z)| z.z_score)
        .collect();
    let global_non_event_zs: Vec<f64> = z_scores
        .iter()
        .enumerate()
        .filter(|(i, _)| !in_any_event[*i])
        .map(|(_, z)| z.z_score)
        .collect();

    let global_event_z = if global_event_zs.is_empty() {
        0.0
    } else {
        global_event_zs.iter().sum::<f64>() / global_event_zs.len() as f64
    };
    let global_non_event_z = if global_non_event_zs.is_empty() {
        0.0
    } else {
        global_non_event_zs.iter().sum::<f64>() / global_non_event_zs.len() as f64
    };

    let (global_t, global_p) = if global_event_zs.len() >= 2 && global_non_event_zs.len() >= 2 {
        crate::consciousness_stats::welch_t_test(&global_event_zs, &global_non_event_zs)
    } else {
        (0.0, 1.0)
    };

    let significant_events = correlations.iter().filter(|c| c.p_value < 0.05).count();

    EnvironmentalReport {
        z_scores: z_scores.to_vec(),
        events: events.to_vec(),
        correlations,
        global_event_z,
        global_non_event_z,
        global_t,
        global_p,
        significant_events,
    }
}

fn pooled_standard_deviation(a: &[f64], b: &[f64]) -> f64 {
    if a.len() + b.len() < 3 {
        return 1.0;
    }
    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
    let mean_b = b.iter().sum::<f64>() / b.len() as f64;
    let var_a: f64 = a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>();
    let var_b: f64 = b.iter().map(|x| (x - mean_b).powi(2)).sum::<f64>();
    let pooled_var = (var_a + var_b) / (a.len() + b.len() - 2) as f64;
    pooled_var.sqrt().max(1e-10)
}

/// Get current ISO 8601 wall-clock time.
pub fn current_wall_time() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC ISO-8601
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Approximate date from days since epoch (good enough for logging)
    let days = secs / 86400;
    let (year, month, day) = days_to_date(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    // Simplified Gregorian date from days since 1970-01-01
    let mut y = 1970;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let days_in_month = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1;
    for &dim in &days_in_month {
        if remaining < dim {
            break;
        }
        remaining -= dim;
        m += 1;
    }
    (y, m, remaining + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_category_display() {
        assert_eq!(format!("{}", EventCategory::WorldEvent), "world");
        assert_eq!(format!("{}", EventCategory::OperatorState), "operator");
    }

    #[test]
    fn test_days_to_date() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_current_wall_time_format() {
        let t = current_wall_time();
        assert!(t.contains("T"));
        assert!(t.ends_with("Z"));
    }

    #[test]
    fn test_empty_correlation() {
        let report = compute_environmental_correlations(&[], &[], 60.0);
        assert_eq!(report.global_p, 1.0);
        assert_eq!(report.significant_events, 0);
    }

    #[test]
    fn test_correlation_with_event() {
        let z_scores: Vec<TimestampedZ> = (0..100)
            .map(|i| TimestampedZ {
                index: i,
                elapsed_secs: i as f64,
                wall_time: String::new(),
                unix_timestamp: 0.0,
                z_score: if i >= 45 && i <= 55 { 2.0 } else { 0.0 },
                cumulative_z: 0.0,
                source_z_scores: HashMap::new(),
            })
            .collect();

        let events = vec![EnvironmentalEvent {
            name: "test event".to_string(),
            category: EventCategory::Custom,
            timestamp_secs: 50.0,
            wall_time: String::new(),
            intensity: None,
            notes: None,
        }];

        let report = compute_environmental_correlations(&z_scores, &events, 20.0);
        assert_eq!(report.correlations.len(), 1);
        assert!(report.correlations[0].window_mean_z > report.correlations[0].outside_mean_z);
    }

    #[test]
    fn test_pooled_sd() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let sd = pooled_standard_deviation(&a, &b);
        assert!(sd > 0.0);
    }
}
