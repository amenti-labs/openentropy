//! `openentropy consciousness-batch` — automated session batching.
//!
//! Runs N consciousness experiment sessions back-to-back with automatic
//! meta-analysis combining all sessions. Designed for accumulating
//! statistical power over extended testing periods.

use std::time::Instant;

use openentropy_core::consciousness::*;
use openentropy_core::conditioning::ConditioningMode;

use super::make_pool;

pub struct BatchConfig<'a> {
    /// Number of sessions to run.
    pub sessions: usize,
    /// Comma-separated source name filter.
    pub source_filter: Option<&'a str>,
    /// Trials per phase per session.
    pub trials: usize,
    /// Bits per trial.
    pub bits: usize,
    /// Trial interval in milliseconds.
    pub interval_ms: u64,
    /// Quick mode (10 trials per phase).
    pub quick: bool,
    /// Rest period between sessions in seconds.
    pub rest_secs: u64,
    /// Output directory for per-session JSON files.
    pub output_dir: Option<&'a str>,
    /// Operator name for profile tracking.
    pub operator: Option<&'a str>,
}

pub fn run(cfg: BatchConfig<'_>) {
    let pool = make_pool(cfg.source_filter);
    let source_infos = pool.source_infos();

    if source_infos.is_empty() {
        eprintln!("Error: no entropy sources available");
        std::process::exit(1);
    }

    let active_sources: Vec<(String, String)> = source_infos
        .iter()
        .filter(|s| !s.composite)
        .map(|s| (s.name.clone(), s.category.clone()))
        .collect();

    let trials_per_phase = if cfg.quick { 10 } else { cfg.trials };

    println!();
    println!("  CONSCIOUSNESS-RNG BATCH EXPERIMENT");
    println!("  {}", "=".repeat(50));
    println!("  Sessions:       {}", cfg.sessions);
    println!("  Trials/session: {} per phase", trials_per_phase);
    println!("  Bits/trial:     {}", cfg.bits);
    println!("  Sources:        {} active", active_sources.len());
    println!("  Rest period:    {}s between sessions", cfg.rest_secs);
    if let Some(op) = cfg.operator {
        println!("  Operator:       {}", op);
    }
    println!();

    // Ensure output directory exists
    let output_dir = cfg.output_dir.unwrap_or("consciousness_batch");
    let _ = std::fs::create_dir_all(output_dir);

    let batch_start = Instant::now();
    let mut session_results: Vec<ExperimentResult> = Vec::new();
    let mut session_z_scores: Vec<f64> = Vec::new();

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    for session_idx in 0..cfg.sessions {
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            println!("\n  Batch interrupted after {} sessions.", session_idx);
            break;
        }

        println!(
            "  ━━━ Session {}/{} ━━━",
            session_idx + 1,
            cfg.sessions
        );
        println!();

        let experiment_config = ExperimentConfig {
            bits_per_trial: cfg.bits,
            trials_per_phase,
            trial_interval_ms: cfg.interval_ms,
            phases: vec![
                IntentionDirection::Baseline,
                IntentionDirection::High,
                IntentionDirection::Low,
            ],
        };

        let experiment_start = Instant::now();
        let bytes_per_trial = experiment_config.bytes_per_trial();
        let n_bits = experiment_config.bits_per_trial;
        let mut all_phase_results: Vec<PhaseResult> = Vec::new();

        for (phase_idx, &direction) in experiment_config.phases.iter().enumerate() {
            if !running.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            println!(
                "    Phase {}/{}: {}",
                phase_idx + 1,
                experiment_config.phases.len(),
                direction
            );

            let mut phase_trials: Vec<Trial> = Vec::new();
            let mut cumulative_zs: Vec<f64> = Vec::new();

            for trial_idx in 0..trials_per_phase {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                let trial_start = Instant::now();
                let timestamp_secs = experiment_start.elapsed().as_secs_f64();
                let mut source_trials: Vec<SourceTrial> = Vec::new();

                for (source_name, category) in &active_sources {
                    let conditioned = pool
                        .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                        .unwrap_or_default();

                    if conditioned.len() < bytes_per_trial {
                        continue;
                    }

                    let ones = count_ones_n(&conditioned, n_bits);
                    let z = trial_z_score(ones, n_bits);

                    source_trials.push(SourceTrial {
                        source_name: source_name.clone(),
                        category: category.clone(),
                        ones_count: ones,
                        z_score: z,
                    });
                }

                if source_trials.is_empty() {
                    continue;
                }

                let pooled_z = source_trials.iter().map(|st| st.z_score).sum::<f64>()
                    / source_trials.len() as f64;

                let trial = Trial {
                    index: trial_idx,
                    direction,
                    source_trials,
                    pooled_z,
                    timestamp_secs,
                };

                cumulative_zs.push(pooled_z);
                phase_trials.push(trial);

                // Progress
                let progress = trial_idx + 1;
                let bar_width = 20;
                let filled = (progress * bar_width) / trials_per_phase;
                let bar: String = (0..bar_width)
                    .map(|i| if i < filled { '#' } else { '-' })
                    .collect();
                let cum_z = stouffer_z(&cumulative_zs);
                print!(
                    "\r    [{bar}] {progress:>3}/{trials_per_phase}  Z: {:>7}",
                    format_z(cum_z)
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());

                // Wait for interval
                let elapsed = trial_start.elapsed();
                let interval = std::time::Duration::from_millis(cfg.interval_ms);
                if elapsed < interval {
                    std::thread::sleep(interval - elapsed);
                }
            }

            println!();
            let phase_result = compute_phase_result(direction, &phase_trials);
            println!(
                "    {} complete: Z = {}",
                direction,
                format_z(phase_result.cumulative_z)
            );
            all_phase_results.push(phase_result);
        }

        if all_phase_results.len() < 3 {
            println!("    Session aborted.");
            continue;
        }

        let source_differentials = compute_source_differentials(&all_phase_results);

        let high_phase = all_phase_results
            .iter()
            .find(|p| p.direction == IntentionDirection::High);
        let low_phase = all_phase_results
            .iter()
            .find(|p| p.direction == IntentionDirection::Low);

        let (overall_diff_z, overall_p) = match (high_phase, low_phase) {
            (Some(h), Some(l)) => {
                let diff_z = (h.cumulative_z - l.cumulative_z) / std::f64::consts::SQRT_2;
                (diff_z, z_to_p_two_tailed(diff_z))
            }
            _ => (0.0, 1.0),
        };

        let duration_secs = experiment_start.elapsed().as_secs_f64();
        let result = ExperimentResult {
            config: experiment_config,
            phases: all_phase_results,
            source_differentials,
            overall_differential_z: overall_diff_z,
            overall_p,
            duration_secs,
        };

        // Save per-session JSON
        let session_file = format!("{}/session_{:03}.json", output_dir, session_idx + 1);
        let mode_result = ModeResult::Standard(result.clone());
        if let Ok(json) = serde_json::to_string_pretty(&mode_result) {
            let _ = std::fs::write(&session_file, json);
        }

        println!(
            "\n    Session {} result: Z = {}, p = {}",
            session_idx + 1,
            format_z(overall_diff_z),
            format_p_value(overall_p)
        );

        session_z_scores.push(overall_diff_z);
        session_results.push(result);

        // Running meta-analysis
        let combined_z = stouffer_z(&session_z_scores);
        let combined_p = z_to_p_two_tailed(combined_z);
        println!(
            "    Running combined: Z = {}, p = {} ({} sessions)",
            format_z(combined_z),
            format_p_value(combined_p),
            session_z_scores.len()
        );

        // Update operator profile if specified
        if let Some(operator_name) = cfg.operator {
            let profiles_dir = "consciousness_profiles";
            let mut profile =
                super::consciousness_profile::load_or_create(operator_name, profiles_dir);
            super::consciousness_profile::update_with_result(
                &mut profile,
                session_results.last().unwrap(),
                ExperimentMode::Standard,
                None,
            );
            super::consciousness_profile::save_profile(&profile, profiles_dir);
        }

        // Rest period between sessions
        if session_idx + 1 < cfg.sessions {
            println!();
            println!(
                "    Rest period: {}s (Ctrl+C to stop batch)",
                cfg.rest_secs
            );
            for remaining in (1..=cfg.rest_secs).rev() {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                print!("\r    Resuming in {remaining}s...  ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            println!();
        }
    }

    // Final meta-analysis
    let total_elapsed = batch_start.elapsed().as_secs_f64();
    let combined_z = stouffer_z(&session_z_scores);
    let combined_p = z_to_p_two_tailed(combined_z);

    println!();
    println!("  BATCH META-ANALYSIS");
    println!("  {}", "=".repeat(50));
    println!("  Sessions completed: {}", session_results.len());
    println!("  Total duration:     {:.1}s ({:.1} min)", total_elapsed, total_elapsed / 60.0);
    println!("  Combined Z:         {}", format_z(combined_z));
    println!("  Combined p:         {}", format_p_value(combined_p));
    println!();

    // Forest plot
    println!("  Session Forest Plot:");
    println!("  {}", "-".repeat(50));
    for (i, z) in session_z_scores.iter().enumerate() {
        let bar_center = 20;
        let bar_pos = ((z / 3.0) * bar_center as f64) as i32 + bar_center as i32;
        let bar_pos = bar_pos.clamp(0, 39) as usize;
        let mut bar = vec![' '; 40];
        bar[bar_center] = '|';
        bar[bar_pos] = '#';
        let bar_str: String = bar.into_iter().collect();
        println!("  S{:>3} [{bar_str}] {}", i + 1, format_z(*z));
    }

    // Per-source aggregation
    let mut source_z_map: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for result in &session_results {
        for diff in &result.source_differentials {
            source_z_map
                .entry(diff.source_name.clone())
                .or_default()
                .push(diff.differential_z);
        }
    }

    if !source_z_map.is_empty() {
        println!();
        println!("  Cross-Session Source Analysis:");
        println!("  {}", "-".repeat(50));
        println!(
            "  {:<24} {:>10} {:>12}",
            "Source", "Combined Z", "p-value"
        );

        let mut source_combined: Vec<(String, f64, f64)> = source_z_map
            .iter()
            .map(|(name, zs)| {
                let z = stouffer_z(zs);
                let p = z_to_p_two_tailed(z);
                (name.clone(), z, p)
            })
            .collect();
        source_combined.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal));

        for (name, z, p) in &source_combined {
            let marker = if *name == "prng_control" { " [CTRL]" } else { "" };
            println!(
                "  {:<24} {:>10} {:>12}{}",
                name,
                format_z(*z),
                format_p_value(*p),
                marker
            );
        }
    }

    println!();
    if combined_p < 0.01 {
        println!("  Strong cumulative evidence after {} sessions.", session_results.len());
    } else if combined_p < 0.05 {
        println!("  Suggestive cumulative trend after {} sessions.", session_results.len());
    } else {
        println!("  No significant cumulative effect after {} sessions.", session_results.len());
        if session_results.len() < 20 {
            println!("  PEAR Lab typically needed 50+ sessions for significance.");
        }
    }

    // Save combined meta-analysis JSON
    let meta_file = format!("{}/meta_analysis.json", output_dir);
    let meta_data = serde_json::json!({
        "sessions": session_results.len(),
        "combined_z": combined_z,
        "combined_p": combined_p,
        "session_z_scores": session_z_scores,
        "total_duration_secs": total_elapsed,
    });
    if let Ok(json) = serde_json::to_string_pretty(&meta_data) {
        let _ = std::fs::write(&meta_file, json);
        println!("\n  Results saved to {}/", output_dir);
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_config_defaults() {
        let cfg = BatchConfig {
            sessions: 5,
            source_filter: None,
            trials: 50,
            bits: 200,
            interval_ms: 1000,
            quick: true,
            rest_secs: 30,
            output_dir: None,
            operator: None,
        };
        assert_eq!(cfg.sessions, 5);
        assert!(cfg.quick);
    }
}
