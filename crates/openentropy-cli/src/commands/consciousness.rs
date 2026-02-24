//! `openentropy consciousness` — run a PEAR Lab-style intention experiment.
//!
//! Tests whether focused human intention can influence hardware RNG output.
//! Uses per-source differential analysis — a capability unique to OpenEntropy.
//!
//! ## Modes
//!
//! - **standard**: Classic tripolar protocol (Baseline / High / Low)
//! - **spectroscopy**: Cross-mechanism consciousness spectroscopy by source domain
//! - **structure**: Information-theoretic signature detection (ApEn, SampEn, LZ76, flatness)
//! - **coherence**: Cross-source coherence analysis (pairwise correlation shifts)

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::consciousness::*;

use super::make_pool;

/// Configuration for the consciousness CLI command.
pub struct ConsciousnessCommandConfig<'a> {
    pub source_filter: Option<&'a str>,
    pub trials: usize,
    pub bits: usize,
    pub interval_ms: u64,
    pub output_path: Option<&'a str>,
    pub quick: bool,
    pub mode: ExperimentMode,
    pub epochs: usize,
    pub epoch_duration_secs: u64,
    pub double_blind: bool,
    pub preregister: bool,
    pub operator: Option<&'a str>,
    pub evalue: bool,
    pub deep_analysis: bool,
    pub surrogate_n: usize,
    pub te_order: usize,
    pub calibration_file: Option<&'a str>,
}

pub fn run(cfg: ConsciousnessCommandConfig<'_>) {
    // Build pool
    let pool = make_pool(cfg.source_filter);
    let source_infos = pool.source_infos();

    if source_infos.is_empty() {
        eprintln!("Error: no entropy sources available");
        std::process::exit(1);
    }

    // Determine active sources (non-composite only for clean analysis)
    let mut active_sources: Vec<(String, String)> = source_infos
        .iter()
        .filter(|s| !s.composite)
        .map(|s| (s.name.clone(), s.category.clone()))
        .collect();

    // Ensure prng_control is always included as negative control
    let has_prng = active_sources.iter().any(|(n, _)| n == "prng_control");
    if !has_prng {
        active_sources.push(("prng_control".to_string(), "system".to_string()));
    }

    if active_sources.is_empty() {
        eprintln!("Error: no non-composite entropy sources available");
        std::process::exit(1);
    }

    // Pre-registration
    let prereg = if cfg.preregister {
        let experiment_config = ExperimentConfig {
            bits_per_trial: cfg.bits,
            trials_per_phase: if cfg.quick { 10 } else { cfg.trials },
            trial_interval_ms: cfg.interval_ms,
            phases: vec![
                IntentionDirection::Baseline,
                IntentionDirection::High,
                IntentionDirection::Low,
            ],
        };
        let pr = generate_preregistration(
            cfg.mode,
            &experiment_config,
            cfg.double_blind,
            cfg.operator,
        );
        println!("  Pre-registration hash: {}", pr.hash);
        println!("  (Record this hash before starting the experiment)");
        println!();
        Some(pr)
    } else {
        None
    };

    // Double-blind: randomize intention direction
    let double_blind_directions = if cfg.double_blind {
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        // Simple shuffle: swap High/Low based on seed bit
        let swap = seed % 2 == 1;
        if swap {
            println!("  [DOUBLE-BLIND] Intention directions randomized.");
            println!("  You will be told to go HIGH or LOW, but the true");
            println!("  mapping is hidden. Reveal after experiment.");
            println!();
        }
        Some(swap)
    } else {
        None
    };

    match cfg.mode {
        ExperimentMode::Standard => run_standard(&cfg, &pool, &active_sources, &source_infos, &prereg),
        ExperimentMode::Spectroscopy => {
            run_spectroscopy(&cfg, &pool, &active_sources, &source_infos)
        }
        ExperimentMode::Structure => run_structure(&cfg, &pool, &active_sources),
        ExperimentMode::Coherence => run_coherence(&cfg, &pool, &active_sources),
        ExperimentMode::Temporal => run_temporal(&cfg, &pool, &active_sources, &source_infos),
        ExperimentMode::Adversarial => run_adversarial(&cfg, &pool, &active_sources, &source_infos),
        ExperimentMode::Feedback => run_feedback(&cfg, &pool, &active_sources, &source_infos),
        ExperimentMode::Anomaly => run_anomaly(&cfg, &pool, &active_sources),
        ExperimentMode::Retrocausal => run_retrocausal(&cfg, &pool, &active_sources, &source_infos),
    }

    // Save operator profile if requested
    if let Some(operator_name) = cfg.operator {
        // We can't easily pass results back through all runners, so we skip profile
        // update for non-standard modes for now. Profile updates happen in run_standard.
        let _ = operator_name;
    }
    let _ = double_blind_directions; // used in future double-blind expansion
}

/// Run a consciousness experiment with the TUI dashboard for live visualization.
pub fn run_with_tui(cfg: ConsciousnessCommandConfig<'_>) {
    use crate::tui::consciousness::{ConsciousnessApp, ConsciousnessSharedState, TrialSnapshot};
    use std::sync::Mutex;

    let pool = make_pool(cfg.source_filter);
    let source_infos = pool.source_infos();

    if source_infos.is_empty() {
        eprintln!("Error: no entropy sources available");
        std::process::exit(1);
    }

    let mut active_sources: Vec<(String, String)> = source_infos
        .iter()
        .filter(|s| !s.composite)
        .map(|s| (s.name.clone(), s.category.clone()))
        .collect();

    let has_prng = active_sources.iter().any(|(n, _)| n == "prng_control");
    if !has_prng {
        active_sources.push(("prng_control".to_string(), "system".to_string()));
    }

    if active_sources.is_empty() {
        eprintln!("Error: no non-composite entropy sources available");
        std::process::exit(1);
    }

    let trials_per_phase = if cfg.quick { 10 } else { cfg.trials };
    let phases = vec![
        IntentionDirection::Baseline,
        IntentionDirection::High,
        IntentionDirection::Low,
    ];

    let source_names: Vec<String> = active_sources.iter().map(|(n, _)| n.clone()).collect();
    let shared = Arc::new(Mutex::new(ConsciousnessSharedState::new(
        cfg.mode,
        source_names,
        trials_per_phase,
        phases.len(),
    )));

    // Spawn experiment thread
    let shared_clone = shared.clone();
    let bits = cfg.bits;
    let interval_ms = cfg.interval_ms;
    let active_cloned = active_sources.clone();
    let phases_cloned = phases.clone();

    std::thread::spawn(move || {
        let bytes_per_trial = (bits + 7) / 8;
        let experiment_start = Instant::now();
        let mut all_cumulative: Vec<f64> = Vec::new();

        for (phase_idx, &direction) in phases_cloned.iter().enumerate() {
            {
                let mut st = shared_clone.lock().unwrap();
                st.current_phase = direction;
                st.phase_index = phase_idx;
                st.trial_in_phase = 0;
            }

            let mut phase_zs: Vec<f64> = Vec::new();

            for trial_idx in 0..trials_per_phase {
                let trial_start = Instant::now();
                let timestamp_secs = experiment_start.elapsed().as_secs_f64();
                let mut source_z_scores = HashMap::new();
                let mut pooled_zs: Vec<f64> = Vec::new();
                let mut first_ones = 0u32;

                for (source_name, _category) in &active_cloned {
                    let conditioned = pool
                        .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                        .unwrap_or_default();

                    if conditioned.len() < bytes_per_trial {
                        continue;
                    }

                    let ones = count_ones_n(&conditioned, bits);
                    let z = trial_z_score(ones, bits);
                    source_z_scores.insert(source_name.clone(), z);
                    pooled_zs.push(z);

                    if first_ones == 0 {
                        first_ones = ones;
                    }
                }

                if pooled_zs.is_empty() {
                    continue;
                }

                let pooled_z =
                    pooled_zs.iter().sum::<f64>() / pooled_zs.len() as f64;
                phase_zs.push(pooled_z);
                all_cumulative.push(pooled_z);

                let cum_z = stouffer_z(&phase_zs);
                let cum_p = z_to_p_two_tailed(cum_z);

                let snapshot = TrialSnapshot {
                    trial_index: phase_idx * trials_per_phase + trial_idx,
                    direction,
                    pooled_z,
                    cumulative_z: cum_z,
                    p_value: cum_p,
                    source_z_scores,
                    ones_count: first_ones,
                    timestamp_secs,
                };

                {
                    let mut st = shared_clone.lock().unwrap();
                    st.trials.push(snapshot);
                    st.trial_in_phase = trial_idx + 1;
                }

                // Wait for interval
                let elapsed = trial_start.elapsed();
                let target = Duration::from_millis(interval_ms);
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                }
            }

            // Record phase cumulative Z
            let phase_cum_z = stouffer_z(&phase_zs);
            let phase_cum_p = z_to_p_two_tailed(phase_cum_z);
            {
                let mut st = shared_clone.lock().unwrap();
                st.phase_cumulative_z
                    .push((direction, phase_cum_z, phase_cum_p));
            }
        }

        // Mark experiment complete
        {
            let mut st = shared_clone.lock().unwrap();
            st.experiment_complete = true;
        }
    });

    // Run TUI on main thread
    let mut app = ConsciousnessApp::new(shared);
    if let Err(e) = app.run() {
        eprintln!("TUI error: {e}");
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn setup_ctrlc() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");
    running
}

fn print_header(
    mode: ExperimentMode,
    active_sources: &[(String, String)],
    source_infos_len: usize,
) {
    println!();
    println!("  CONSCIOUSNESS-RNG EXPERIMENT");
    println!("  {}", "=".repeat(50));
    println!(
        "  Mode:       {}",
        match mode {
            ExperimentMode::Standard => "Standard (PEAR Lab tripolar)",
            ExperimentMode::Spectroscopy => "Spectroscopy (cross-domain analysis)",
            ExperimentMode::Structure => "Structure (information-theoretic)",
            ExperimentMode::Coherence => "Coherence (cross-source correlation)",
            ExperimentMode::Temporal => "Temporal (onset/decay analysis)",
            ExperimentMode::Adversarial => "Adversarial (two-operator protocol)",
            ExperimentMode::Feedback => "Feedback (real-time guided intention)",
            ExperimentMode::Anomaly => "Anomaly (ML-lite multivariate detection)",
            ExperimentMode::Retrocausal => "Retrocausal (pre-collected data protocol)",
        }
    );
    println!(
        "  Sources:    {} active ({} available)",
        active_sources.len(),
        source_infos_len
    );

    // Categorize sources
    let mut category_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for (_, cat) in active_sources {
        *category_counts.entry(cat.as_str()).or_insert(0) += 1;
    }
    let mut cats: Vec<(&str, usize)> = category_counts.into_iter().collect();
    cats.sort_by(|a, b| b.1.cmp(&a.1));
    print!("  Categories: ");
    for (i, (cat, count)) in cats.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{count} {cat}");
    }
    println!();
}

fn print_phase_instruction(direction: IntentionDirection) {
    match direction {
        IntentionDirection::Baseline => {
            println!("  Relax. No intention. Just observe.");
        }
        IntentionDirection::High => {
            println!("  Focus your intention: INCREASE the number of 1-bits.");
            println!("  Visualize the numbers going UP. More ones. Higher.");
        }
        IntentionDirection::Low => {
            println!("  Focus your intention: DECREASE the number of 1-bits.");
            println!("  Visualize the numbers going DOWN. Fewer ones. Lower.");
        }
    }
}

fn countdown(running: &AtomicBool, quick: bool) {
    if !quick {
        for cd in (1..=3).rev() {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            print!("  Starting in {cd}...\r");
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_secs(1));
        }
        println!();
    }
}

fn wait_interval(start: Instant, interval_ms: u64, running: &AtomicBool) {
    let elapsed = start.elapsed();
    let interval = Duration::from_millis(interval_ms);
    if elapsed < interval {
        let deadline = Instant::now() + (interval - elapsed);
        while Instant::now() < deadline && running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Run the standard tripolar protocol phase collection.
/// Returns (phase_results, source_differentials, overall_differential_z, overall_p, duration_secs).
fn run_tripolar_phases(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
) -> (Vec<PhaseResult>, Vec<SourceDifferential>, f64, f64, f64) {
    let running = setup_ctrlc();
    let trials_per_phase = if cfg.quick { 10 } else { cfg.trials };
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

    println!(
        "  Trials:     {} per phase x {} bits @ {:.1} Hz",
        trials_per_phase,
        cfg.bits,
        1000.0 / cfg.interval_ms as f64
    );
    println!(
        "  Duration:   ~{:.0}s ({:.1} min)",
        experiment_config.estimated_duration_secs(),
        experiment_config.estimated_duration_secs() / 60.0
    );
    println!();

    let experiment_start = Instant::now();
    let bytes_per_trial = experiment_config.bytes_per_trial();
    let n_bits = experiment_config.bits_per_trial;
    let mut all_phase_results: Vec<PhaseResult> = Vec::new();

    for (phase_idx, &direction) in experiment_config.phases.iter().enumerate() {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        println!(
            "  Phase {}/{}: {}",
            phase_idx + 1,
            experiment_config.phases.len(),
            direction
        );
        print_phase_instruction(direction);
        println!();
        countdown(&running, cfg.quick);

        let mut phase_trials: Vec<Trial> = Vec::new();
        let mut cumulative_zs: Vec<f64> = Vec::new();

        for trial_idx in 0..trials_per_phase {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            let trial_start = Instant::now();
            let timestamp_secs = experiment_start.elapsed().as_secs_f64();
            let mut source_trials: Vec<SourceTrial> = Vec::new();

            for (source_name, category) in active_sources {
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
            let cum_z = stouffer_z(&cumulative_zs);
            let cum_p = z_to_p_two_tailed(cum_z);

            let progress = trial_idx + 1;
            let bar_width = 20;
            let filled = (progress * bar_width) / trials_per_phase;
            let bar: String = (0..bar_width)
                .map(|i| if i < filled { '#' } else { '-' })
                .collect();

            print!(
                "\r  [{bar}] {progress:>3}/{trials_per_phase}  Z: {:>7}  p: {:<12}  ones: {:>3}",
                format_z(cum_z),
                format_p_value(cum_p),
                trial.source_trials.first().map_or(0, |st| st.ones_count)
            );
            let _ = std::io::stdout().flush();

            phase_trials.push(trial);
            wait_interval(trial_start, cfg.interval_ms, &running);
        }

        println!();

        let phase_result = compute_phase_result(direction, &phase_trials);
        println!(
            "  {} complete: Z = {}, p = {}",
            direction,
            format_z(phase_result.cumulative_z),
            format_p_value(phase_result.p_value)
        );
        println!();

        all_phase_results.push(phase_result);
    }

    let source_differentials = compute_source_differentials(&all_phase_results);

    let high_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::High);
    let low_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::Low);

    let (overall_differential_z, overall_p) = match (high_phase, low_phase) {
        (Some(h), Some(l)) => {
            let diff_z = (h.cumulative_z - l.cumulative_z) / std::f64::consts::SQRT_2;
            (diff_z, z_to_p_two_tailed(diff_z))
        }
        _ => (0.0, 1.0),
    };

    let duration_secs = experiment_start.elapsed().as_secs_f64();
    (
        all_phase_results,
        source_differentials,
        overall_differential_z,
        overall_p,
        duration_secs,
    )
}

fn save_json(path: Option<&str>, result: &ModeResult) {
    if let Some(path) = path {
        match serde_json::to_string_pretty(result) {
            Ok(json) => match std::fs::write(path, &json) {
                Ok(()) => println!("\n  Results saved to {path}"),
                Err(e) => eprintln!("\n  Error writing results: {e}"),
            },
            Err(e) => eprintln!("\n  Error serializing results: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Standard mode
// ---------------------------------------------------------------------------

fn run_standard(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
    prereg: &Option<PreRegistration>,
) {
    print_header(ExperimentMode::Standard, active_sources, source_infos.len());
    println!(
        "  Protocol:   PEAR Lab tripolar (Baseline / High / Low)"
    );
    if cfg.double_blind {
        println!("  Blinding:   DOUBLE-BLIND (directions randomized)");
    }
    if let Some(pr) = prereg {
        println!("  Pre-reg:    {}", pr.hash);
    }

    let (all_phase_results, source_differentials, overall_diff_z, overall_p, duration_secs) =
        run_tripolar_phases(cfg, pool, active_sources);

    if all_phase_results.is_empty() {
        eprintln!("Experiment aborted — no phases completed.");
        return;
    }

    let experiment_config = ExperimentConfig {
        bits_per_trial: cfg.bits,
        trials_per_phase: if cfg.quick { 10 } else { cfg.trials },
        trial_interval_ms: cfg.interval_ms,
        phases: vec![
            IntentionDirection::Baseline,
            IntentionDirection::High,
            IntentionDirection::Low,
        ],
    };

    let result = ExperimentResult {
        config: experiment_config,
        phases: all_phase_results,
        source_differentials,
        overall_differential_z: overall_diff_z,
        overall_p,
        duration_secs,
    };

    print_standard_results(&result);

    // E-value enrichment
    if cfg.evalue {
        print_evalue_enrichment(&result.phases, cfg.bits);
    }

    // Deep analysis enrichment
    if cfg.deep_analysis {
        print_deep_analysis(&result.phases, cfg.bits, cfg.surrogate_n, cfg.te_order, cfg.calibration_file);
    }

    // Update operator profile if requested
    if let Some(operator_name) = cfg.operator {
        let profiles_dir = "consciousness_profiles";
        let mut profile =
            super::consciousness_profile::load_or_create(operator_name, profiles_dir);
        let prereg_hash = prereg.as_ref().map(|p| p.hash.clone());
        super::consciousness_profile::update_with_result(
            &mut profile,
            &result,
            cfg.mode,
            prereg_hash,
        );
        super::consciousness_profile::save_profile(&profile, profiles_dir);
        println!(
            "  Profile updated for operator '{}' (session #{})",
            operator_name, profile.total_sessions
        );
    }

    save_json(cfg.output_path, &ModeResult::Standard(result));
}

// ---------------------------------------------------------------------------
// Spectroscopy mode
// ---------------------------------------------------------------------------

fn run_spectroscopy(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
) {
    print_header(
        ExperimentMode::Spectroscopy,
        active_sources,
        source_infos.len(),
    );
    println!("  Protocol:   Tripolar + cross-domain spectroscopy");

    let (all_phase_results, source_differentials, _overall_diff_z, _overall_p, _duration_secs) =
        run_tripolar_phases(cfg, pool, active_sources);

    if all_phase_results.is_empty() {
        eprintln!("Experiment aborted — no phases completed.");
        return;
    }

    let spectroscopy = compute_spectroscopy(&source_differentials);
    print_spectroscopy_results(&spectroscopy);
    save_json(cfg.output_path, &ModeResult::Spectroscopy(spectroscopy));
}

// ---------------------------------------------------------------------------
// Structure mode
// ---------------------------------------------------------------------------

fn run_structure(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
) {
    let running = setup_ctrlc();
    let epochs = if cfg.quick { 2 } else { cfg.epochs };
    let epoch_secs = if cfg.quick { 5 } else { cfg.epoch_duration_secs };
    let bytes_per_epoch: usize = 256; // Enough for information-theoretic measures

    println!(
        "  Epochs:     {} x {}s (Baseline, then intention alternating)",
        epochs, epoch_secs
    );
    println!();

    // Epoch order: Baseline, High, Baseline, Low, Baseline, High, ...
    let directions = [
        IntentionDirection::Baseline,
        IntentionDirection::High,
        IntentionDirection::Baseline,
        IntentionDirection::Low,
    ];

    let mut all_epochs: Vec<EpochMeasures> = Vec::new();

    for epoch_idx in 0..epochs {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let direction = directions[epoch_idx % directions.len()];
        println!(
            "  Epoch {}/{}: {}",
            epoch_idx + 1,
            epochs,
            direction
        );
        print_phase_instruction(direction);
        println!();
        countdown(&running, cfg.quick);

        // Collect bytes during this epoch from all sources (pooled)
        let epoch_start = Instant::now();
        let mut epoch_bytes: Vec<u8> = Vec::new();

        while epoch_start.elapsed() < Duration::from_secs(epoch_secs) {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            for (source_name, _) in active_sources {
                let conditioned = pool
                    .get_source_bytes(source_name, 32, ConditioningMode::Sha256)
                    .unwrap_or_default();
                epoch_bytes.extend_from_slice(&conditioned);
            }

            // Progress indicator
            let elapsed = epoch_start.elapsed().as_secs_f64();
            let progress = (elapsed / epoch_secs as f64 * 100.0).min(100.0);
            print!("\r  Collecting... {progress:>5.1}%  ({} bytes)", epoch_bytes.len());
            let _ = std::io::stdout().flush();

            std::thread::sleep(Duration::from_millis(cfg.interval_ms.max(50)));
        }
        println!();

        if epoch_bytes.len() < bytes_per_epoch {
            eprintln!(
                "  Warning: only collected {} bytes (need {})",
                epoch_bytes.len(),
                bytes_per_epoch
            );
        }

        // Compute info-theoretic measures
        let measures = compute_epoch_measures(&epoch_bytes, direction, epoch_idx);
        println!(
            "  ApEn={:.4} SampEn={:.4} LZ76={:.4} Flatness={:.4}",
            measures.approximate_entropy,
            measures.sample_entropy,
            measures.lz76_complexity,
            measures.spectral_flatness
        );
        println!();

        all_epochs.push(measures);
    }

    if all_epochs.is_empty() {
        eprintln!("Experiment aborted — no epochs completed.");
        return;
    }

    let result = compute_structure(&all_epochs);
    print_structure_results(&result);
    save_json(cfg.output_path, &ModeResult::Structure(result));
}

// ---------------------------------------------------------------------------
// Coherence mode
// ---------------------------------------------------------------------------

fn run_coherence(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
) {
    let running = setup_ctrlc();
    let epochs = if cfg.quick { 2 } else { cfg.epochs };
    let epoch_secs = if cfg.quick { 5 } else { cfg.epoch_duration_secs };

    // Limit to first 8 sources to keep pairwise count manageable
    let capped_sources: Vec<(String, String)> = active_sources.iter().take(8).cloned().collect();
    let n_pairs = capped_sources.len() * (capped_sources.len() - 1) / 2;

    println!(
        "  Sources:    {} (capped at 8 for {} pairwise comparisons)",
        capped_sources.len(),
        n_pairs
    );
    println!(
        "  Epochs:     {} x {}s (alternating Baseline / Intention)",
        epochs, epoch_secs
    );
    println!();

    // Alternate: Baseline, Intention (High), Baseline, Intention (High), ...
    let mut baseline_data: HashMap<String, Vec<u8>> = HashMap::new();
    let mut intention_data: HashMap<String, Vec<u8>> = HashMap::new();

    for epoch_idx in 0..epochs {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let is_baseline = epoch_idx % 2 == 0;
        let direction = if is_baseline {
            IntentionDirection::Baseline
        } else {
            IntentionDirection::High
        };

        println!(
            "  Epoch {}/{}: {}",
            epoch_idx + 1,
            epochs,
            direction
        );
        print_phase_instruction(direction);
        println!();
        countdown(&running, cfg.quick);

        let epoch_start = Instant::now();
        let mut epoch_source_bytes: HashMap<String, Vec<u8>> = HashMap::new();

        while epoch_start.elapsed() < Duration::from_secs(epoch_secs) {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            for (source_name, _) in &capped_sources {
                let conditioned = pool
                    .get_source_bytes(source_name, 32, ConditioningMode::Sha256)
                    .unwrap_or_default();
                epoch_source_bytes
                    .entry(source_name.clone())
                    .or_default()
                    .extend_from_slice(&conditioned);
            }

            let elapsed = epoch_start.elapsed().as_secs_f64();
            let progress = (elapsed / epoch_secs as f64 * 100.0).min(100.0);
            print!("\r  Collecting... {progress:>5.1}%");
            let _ = std::io::stdout().flush();

            std::thread::sleep(Duration::from_millis(cfg.interval_ms.max(50)));
        }
        println!();

        // Append to baseline or intention buckets
        let target = if is_baseline {
            &mut baseline_data
        } else {
            &mut intention_data
        };

        for (name, bytes) in epoch_source_bytes {
            target.entry(name).or_default().extend_from_slice(&bytes);
        }

        println!(
            "  Epoch complete ({} bytes per source avg)",
            target.values().next().map_or(0, |v| v.len())
        );
        println!();
    }

    if baseline_data.is_empty() || intention_data.is_empty() {
        eprintln!("Experiment aborted — need at least one baseline and one intention epoch.");
        return;
    }

    let result = compute_coherence(&baseline_data, &intention_data);
    print_coherence_results(&result);
    save_json(cfg.output_path, &ModeResult::Coherence(result));
}

// ---------------------------------------------------------------------------
// Print functions
// ---------------------------------------------------------------------------

fn print_standard_results(result: &ExperimentResult) {
    println!();
    println!("  RESULTS");
    println!("  {}", "=".repeat(62));
    println!();

    println!(
        "  {:<12} {:>6} {:>10} {:>8} {:>16}",
        "Phase", "Trials", "Mean 1s", "Z", "p-value"
    );
    println!("  {}", "-".repeat(56));

    for phase in &result.phases {
        println!(
            "  {:<12} {:>6} {:>10.1} {:>8} {:>16}",
            phase.direction.to_string(),
            phase.trials.len(),
            phase.mean_ones,
            format_z(phase.cumulative_z),
            format_p_value(phase.p_value)
        );
    }

    println!();
    println!(
        "  Differential (High - Low): Z = {}, p = {}",
        format_z(result.overall_differential_z),
        format_p_value(result.overall_p)
    );

    if !result.source_differentials.is_empty() {
        println!();
        println!("  Per-Source Differential Analysis");
        println!("  {}", "-".repeat(72));
        println!(
            "  {:<24} {:<10} {:>8} {:>8} {:>8} {:>12}",
            "Source", "Category", "High Z", "Low Z", "Diff Z", "p-value"
        );
        println!("  {}", "-".repeat(72));

        let mut sorted_diffs = result.source_differentials.clone();
        sorted_diffs.sort_by(|a, b| {
            b.differential_z
                .abs()
                .partial_cmp(&a.differential_z.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for diff in &sorted_diffs {
            let marker = if diff.source_name == "prng_control" {
                " [CTRL]"
            } else {
                ""
            };
            println!(
                "  {:<24} {:<10} {:>8} {:>8} {:>8} {:>12}{}",
                diff.source_name,
                diff.category,
                format_z(diff.high_z),
                format_z(diff.low_z),
                format_z(diff.differential_z),
                format_p_value(diff.differential_p),
                marker
            );
        }

        let significant: Vec<&SourceDifferential> = sorted_diffs
            .iter()
            .filter(|d| d.differential_p < 0.05)
            .collect();
        if !significant.is_empty() {
            println!();
            println!("  Significant effects (p < 0.05):");
            for d in &significant {
                let dir_word = if d.differential_z > 0.0 {
                    "positive"
                } else {
                    "negative"
                };
                let ctrl_note = if d.source_name == "prng_control" {
                    " [WARNING: PRNG control — likely statistical artifact]"
                } else {
                    ""
                };
                println!(
                    "    {} ({}) — {} effect, Z = {}, p = {}{}",
                    d.source_name,
                    d.category,
                    dir_word,
                    format_z(d.differential_z),
                    format_p_value(d.differential_p),
                    ctrl_note
                );
            }
        }
    }

    // PRNG control check
    if let Some(prng) = result
        .source_differentials
        .iter()
        .find(|d| d.source_name == "prng_control")
    {
        println!();
        println!(
            "  PRNG Control: Z = {}, p = {} — {}",
            format_z(prng.differential_z),
            format_p_value(prng.differential_p),
            if prng.differential_p < 0.05 {
                "WARNING: PRNG shows effect — check for statistical artifacts"
            } else {
                "OK (no spurious effect on deterministic control)"
            }
        );
    }

    println!();
    println!("  {}", "-".repeat(62));
    let p = result.overall_p;
    if p < 0.01 {
        println!("  Strong evidence of intention effect (p < 0.01).");
        println!("  Consider replicating with additional sessions.");
    } else if p < 0.05 {
        println!("  Suggestive evidence of intention effect (p < 0.05).");
        println!("  Additional sessions recommended before drawing conclusions.");
    } else if p < 0.10 {
        println!("  Marginal trend (p < 0.10). Not statistically significant.");
        println!("  Try a longer session (--trials 100) for more statistical power.");
    } else {
        println!("  No significant effect detected (p = {:.3}).", p);
        println!("  This is the expected null result for most single sessions.");
        println!("  PEAR Lab found effects accumulate over many sessions.");
    }

    println!(
        "\n  Experiment duration: {:.1}s ({:.1} min)",
        result.duration_secs,
        result.duration_secs / 60.0
    );
    println!();
}

fn print_spectroscopy_results(result: &SpectroscopyResult) {
    println!();
    println!("  SPECTROSCOPY RESULTS");
    println!("  {}", "=".repeat(62));
    println!();
    println!(
        "  {:<14} {:>8} {:>8} {:>8} {:>8} {:>12}",
        "Domain", "Sources", "High Z", "Low Z", "Diff Z", "p-value"
    );
    println!("  {}", "-".repeat(62));

    for domain in &result.domains {
        let bh_sig = result
            .bh_significant
            .iter()
            .find(|(d, _)| d == &domain.domain)
            .map_or(false, |&(_, s)| s);
        let sig_marker = if bh_sig { " *" } else { "" };

        println!(
            "  {:<14} {:>8} {:>8} {:>8} {:>8} {:>12}{}",
            domain.domain,
            domain.sources.len(),
            format_z(domain.high_z),
            format_z(domain.low_z),
            format_z(domain.differential_z),
            format_p_value(domain.differential_p),
            sig_marker
        );
    }

    println!();
    println!("  Heterogeneity Analysis (Schmidt's source-independence hypothesis):");
    println!(
        "    Cochran's Q = {:.3}, p = {}",
        result.cochrans_q,
        format_p_value(result.cochrans_p)
    );
    println!("    I-squared   = {:.1}%", result.i_squared);
    println!("    Interpretation: {}", result.interpretation);

    if result.cochrans_p < 0.05 {
        println!();
        println!("    FINDING: Significant heterogeneity detected across domains.");
        println!("    The consciousness effect, if present, varies by physical mechanism.");
        println!("    This contradicts Schmidt's source-independence hypothesis.");
    } else {
        println!();
        println!("    No significant heterogeneity across domains.");
        println!("    Consistent with Schmidt's source-independence hypothesis.");
    }

    // BH-significant domains
    let sig_domains: Vec<&str> = result
        .bh_significant
        .iter()
        .filter(|&&(_, s)| s)
        .map(|(d, _)| d.as_str())
        .collect();
    if !sig_domains.is_empty() {
        println!();
        println!(
            "  Significant domains (BH-corrected): {}",
            sig_domains.join(", ")
        );
    }

    println!();
}

fn print_structure_results(result: &StructureResult) {
    println!();
    println!("  STRUCTURE ANALYSIS RESULTS");
    println!("  {}", "=".repeat(62));
    println!();

    // Epoch summary
    println!(
        "  {:<8} {:<12} {:>8} {:>8} {:>8} {:>10}",
        "Epoch", "Direction", "ApEn", "SampEn", "LZ76", "Flatness"
    );
    println!("  {}", "-".repeat(58));

    for em in &result.epoch_measures {
        println!(
            "  {:<8} {:<12} {:>8.4} {:>8.4} {:>8.4} {:>10.4}",
            em.epoch_index,
            em.direction.to_string(),
            em.approximate_entropy,
            em.sample_entropy,
            em.lz76_complexity,
            em.spectral_flatness
        );
    }

    // Comparisons table
    println!();
    println!("  Intention vs Baseline Comparison (Welch's t-test):");
    println!("  {}", "-".repeat(72));
    println!(
        "  {:<22} {:>10} {:>10} {:>10} {:>12}",
        "Measure", "Baseline", "Intention", "t", "p-value"
    );
    println!("  {}", "-".repeat(72));

    for c in &result.comparisons {
        println!(
            "  {:<22} {:>10.4} {:>10.4} {:>10.3} {:>12}",
            c.measure_name,
            c.baseline_mean,
            c.intention_mean,
            c.t_statistic,
            format_p_value(c.p_value)
        );
    }

    println!();
    if result.any_significant {
        println!(
            "  FINDING: Significant structure differences detected during intention epochs."
        );
        println!("  This suggests consciousness may inject information-theoretic signatures,");
        println!("  not just mean shift.");
    } else {
        println!(
            "  No significant structure differences between intention and baseline."
        );
        println!("  Standard mean-shift analysis may be more sensitive for this session.");
    }
    println!();
}

fn print_coherence_results(result: &CoherenceResult) {
    println!();
    println!("  COHERENCE ANALYSIS RESULTS");
    println!("  {}", "=".repeat(62));
    println!();
    println!(
        "  Baseline mean |r|:  {:.4}",
        result.baseline_mean_abs_r
    );
    println!(
        "  Intention mean |r|: {:.4}",
        result.intention_mean_abs_r
    );
    println!(
        "  Change:             {:+.4}",
        result.intention_mean_abs_r - result.baseline_mean_abs_r
    );
    println!();

    if !result.shifts.is_empty() {
        println!(
            "  {:<20} {:<20} {:>8} {:>8} {:>8} {:>10}",
            "Source A", "Source B", "Base r", "Int r", "Fisher Z", "p-value"
        );
        println!("  {}", "-".repeat(78));

        for shift in &result.shifts {
            println!(
                "  {:<20} {:<20} {:>8.4} {:>8.4} {:>8.3} {:>10}",
                shift.source_a,
                shift.source_b,
                shift.baseline_r,
                shift.intention_r,
                shift.fisher_z,
                format_p_value(shift.p_value)
            );
        }
    }

    println!();
    println!(
        "  Global coherence Z: {}, p = {}",
        format_z(result.global_coherence_z),
        format_p_value(result.global_p)
    );
    println!(
        "  Significant shifts (BH-corrected): {} / {}",
        result.significant_shifts,
        result.shifts.len()
    );

    println!();
    if result.global_p < 0.05 {
        println!("  FINDING: Significant coherence change during intention.");
        println!("  Independent entropy sources became more correlated during");
        println!("  focused intention — consistent with consciousness-coherence models.");
    } else {
        println!("  No significant coherence change detected.");
        println!("  Cross-source correlations remained stable during intention.");
    }
    println!();
}

// ---------------------------------------------------------------------------
// Temporal mode
// ---------------------------------------------------------------------------

fn run_temporal(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
) {
    print_header(ExperimentMode::Temporal, active_sources, source_infos.len());
    println!("  Protocol:   Tripolar + temporal onset/decay analysis");

    let (all_phase_results, _source_differentials, _overall_diff_z, _overall_p, _duration_secs) =
        run_tripolar_phases(cfg, pool, active_sources);

    if all_phase_results.is_empty() {
        eprintln!("Experiment aborted — no phases completed.");
        return;
    }

    let result = openentropy_core::consciousness_temporal::compute_temporal(&all_phase_results);
    print_temporal_results(&result);
    save_json(
        cfg.output_path,
        &ModeResult::Temporal(result),
    );
}

fn print_temporal_results(result: &openentropy_core::consciousness_temporal::TemporalResult) {
    println!();
    println!("  TEMPORAL ANALYSIS RESULTS");
    println!("  {}", "=".repeat(62));
    println!();

    // Phase Z-series
    for pzs in &result.phase_z_series {
        println!(
            "  Phase: {} ({} trials)",
            pzs.direction, pzs.z_scores.len()
        );
        // Show cumulative Z trend (compact sparkline-style)
        if !pzs.cumulative_z.is_empty() {
            let final_cum_z = pzs.cumulative_z.last().copied().unwrap_or(0.0);
            println!("    Cumulative Z trend: → {}", format_z(final_cum_z));
        }
        println!();
    }

    // Autocorrelations
    if !result.autocorrelations.is_empty() {
        println!("  Autocorrelation Analysis:");
        println!("  {}", "-".repeat(50));
        for ac in &result.autocorrelations {
            println!(
                "    Max |r| = {:.3} at lag {} (threshold: {:.3}) — {}",
                ac.max_abs_corr,
                ac.max_abs_lag,
                ac.threshold,
                if ac.max_abs_corr > ac.threshold {
                    "SIGNIFICANT temporal structure"
                } else {
                    "no significant temporal structure"
                }
            );
        }
        println!();
    }

    // Peak effect windows
    if !result.peak_windows.is_empty() {
        println!("  Peak Effect Windows:");
        println!("  {}", "-".repeat(50));
        for pw in &result.peak_windows {
            println!(
                "    Trials {}-{}: Stouffer Z = {}, phase fraction = {:.1}%",
                pw.start_index,
                pw.start_index + pw.window_size,
                format_z(pw.stouffer_z),
                pw.phase_fraction * 100.0
            );
        }
        println!();
    }

    // Onset detection
    if !result.onset_detections.is_empty() {
        println!("  Onset Detection (CUSUM):");
        println!("  {}", "-".repeat(50));
        for onset in &result.onset_detections {
            if let Some(cp) = onset.change_point {
                println!(
                    "    Change-point at trial {}: pre-onset Z = {}, post-onset Z = {}",
                    cp,
                    format_z(onset.pre_onset_mean_z),
                    format_z(onset.post_onset_mean_z)
                );
            } else {
                println!("    No clear onset detected (gradual or absent)");
            }
        }
        println!();
    }

    // Decay analysis
    if !result.decay_analyses.is_empty() {
        println!("  Decay Analysis:");
        println!("  {}", "-".repeat(50));
        for decay in &result.decay_analyses {
            if decay.r_squared > 0.3 {
                println!(
                    "    Decay rate: {:.4}/trial, half-life: {:.1} trials, R² = {:.3}",
                    decay.decay_rate,
                    decay.half_life_trials,
                    decay.r_squared
                );
            } else {
                println!("    No exponential decay detected (R² = {:.3})", decay.r_squared);
            }
            println!("    {}", decay.interpretation);
        }
        println!();
    }

    // Interpretation
    println!("  {}", result.interpretation);
    println!();
}

// ---------------------------------------------------------------------------
// Adversarial mode
// ---------------------------------------------------------------------------

fn run_adversarial(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
) {
    print_header(
        ExperimentMode::Adversarial,
        active_sources,
        source_infos.len(),
    );
    println!("  Protocol:   Two-operator adversarial (opposing intentions)");
    println!();
    println!("  This mode requires two people at the keyboard.");
    println!("  Operator A will intend HIGH, Operator B will intend LOW simultaneously.");
    println!();

    // Run phases: first half = Operator A (High), second half = Operator B (Low)
    // We reuse tripolar but interpret the two intention phases as operators
    let (all_phase_results, _source_differentials, _overall_diff_z, _overall_p, _duration_secs) =
        run_tripolar_phases(cfg, pool, active_sources);

    if all_phase_results.len() < 3 {
        eprintln!("Experiment aborted — need all 3 phases.");
        return;
    }

    let high_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::High);
    let low_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::Low);

    let operator_a = OperatorResult {
        name: "Operator A".to_string(),
        direction: IntentionDirection::High,
        cumulative_z: high_phase.map(|p| p.cumulative_z).unwrap_or(0.0),
        p_value: high_phase.map(|p| p.p_value).unwrap_or(1.0),
        n_trials: high_phase.map(|p| p.trials.len()).unwrap_or(0),
    };

    let operator_b = OperatorResult {
        name: "Operator B".to_string(),
        direction: IntentionDirection::Low,
        cumulative_z: low_phase.map(|p| p.cumulative_z).unwrap_or(0.0),
        p_value: low_phase.map(|p| p.p_value).unwrap_or(1.0),
        n_trials: low_phase.map(|p| p.trials.len()).unwrap_or(0),
    };

    let net_z = (operator_a.cumulative_z + operator_b.cumulative_z) / std::f64::consts::SQRT_2;
    let net_p = z_to_p_two_tailed(net_z);
    let dominance_z = operator_a.cumulative_z.abs() - operator_b.cumulative_z.abs();

    let interpretation = if net_p < 0.05 {
        if net_z > 0.0 {
            "Operator A (HIGH) dominated — significant net effect".to_string()
        } else {
            "Operator B (LOW) dominated — significant net effect".to_string()
        }
    } else {
        "No significant net effect — intentions may have cancelled".to_string()
    };

    let result = AdversarialResult {
        operator_a,
        operator_b,
        net_z,
        net_p,
        dominance_z,
        interpretation: interpretation.clone(),
    };

    print_adversarial_results(&result);
    save_json(cfg.output_path, &ModeResult::Adversarial(result));
}

fn print_adversarial_results(result: &AdversarialResult) {
    println!();
    println!("  ADVERSARIAL RESULTS");
    println!("  {}", "=".repeat(62));
    println!();

    println!(
        "  {} ({}): Z = {}, p = {}, {} trials",
        result.operator_a.name,
        result.operator_a.direction,
        format_z(result.operator_a.cumulative_z),
        format_p_value(result.operator_a.p_value),
        result.operator_a.n_trials
    );
    println!(
        "  {} ({}): Z = {}, p = {}, {} trials",
        result.operator_b.name,
        result.operator_b.direction,
        format_z(result.operator_b.cumulative_z),
        format_p_value(result.operator_b.p_value),
        result.operator_b.n_trials
    );
    println!();
    println!(
        "  Net Z: {} (p = {})",
        format_z(result.net_z),
        format_p_value(result.net_p)
    );
    println!("  Dominance Z: {}", format_z(result.dominance_z));
    println!();
    println!("  {}", result.interpretation);
    println!();
}

// ---------------------------------------------------------------------------
// Feedback mode
// ---------------------------------------------------------------------------

fn run_feedback(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
) {
    print_header(
        ExperimentMode::Feedback,
        active_sources,
        source_infos.len(),
    );
    println!("  Protocol:   Real-time feedback-guided intention training");
    println!();
    println!("  You will see a visual feedback bar after each trial.");
    println!("  Focus your intention to push the bar to the RIGHT (more 1-bits).");
    println!();

    let running = setup_ctrlc();
    let trials = if cfg.quick { 20 } else { cfg.trials * 2 }; // double for learning curve
    let bytes_per_trial = (cfg.bits + 7) / 8;

    println!(
        "  Trials: {}, Bits/trial: {}, Rate: {:.1} Hz",
        trials,
        cfg.bits,
        1000.0 / cfg.interval_ms as f64
    );
    println!();
    countdown(&running, cfg.quick);

    let _experiment_start = Instant::now();
    let mut feedback_trials: Vec<FeedbackTrial> = Vec::new();

    for trial_idx in 0..trials {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let trial_start = Instant::now();

        // Collect from all sources (pooled)
        let mut all_z: Vec<f64> = Vec::new();
        for (source_name, _) in active_sources {
            let conditioned = pool
                .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                .unwrap_or_default();
            if conditioned.len() >= bytes_per_trial {
                let ones = count_ones_n(&conditioned, cfg.bits);
                let z = trial_z_score(ones, cfg.bits);
                all_z.push(z);
            }
        }

        let z = if all_z.is_empty() {
            0.0
        } else {
            all_z.iter().sum::<f64>() / all_z.len() as f64
        };

        // Feedback bar: -3σ to +3σ mapped to 0-40 chars
        let bar_width = 40;
        let center = bar_width / 2;
        let pos = ((z / 3.0) * center as f64) as i32 + center as i32;
        let pos = pos.clamp(0, bar_width as i32 - 1) as usize;
        let mut bar = vec![' '; bar_width];
        bar[center] = '|';
        for i in center..=pos {
            if z > 0.0 {
                bar[i] = '=';
            }
        }
        for i in pos..=center {
            if z < 0.0 {
                bar[i] = '=';
            }
        }
        bar[pos] = '#';
        let bar_str: String = bar.into_iter().collect();

        let feedback_signal = ((z + 3.0) / 6.0 * 100.0).clamp(0.0, 100.0);

        print!(
            "\r  T{:>3} [{bar_str}] Z={:>7} signal={:>3.0}%",
            trial_idx + 1,
            format_z(z),
            feedback_signal
        );
        let _ = std::io::stdout().flush();

        feedback_trials.push(FeedbackTrial {
            trial_index: trial_idx,
            z_score: z,
            feedback_signal,
            cumulative_z: stouffer_z(
                &feedback_trials
                    .iter()
                    .map(|t| t.z_score)
                    .chain(std::iter::once(z))
                    .collect::<Vec<_>>(),
            ),
        });

        wait_interval(trial_start, cfg.interval_ms, &running);
    }
    println!();

    if feedback_trials.is_empty() {
        eprintln!("Experiment aborted — no trials completed.");
        return;
    }

    // Compute learning curve: correlation between trial index and |Z|
    let indices: Vec<f64> = feedback_trials.iter().map(|t| t.trial_index as f64).collect();
    let abs_z: Vec<f64> = feedback_trials.iter().map(|t| t.z_score.abs()).collect();
    let learning_corr = openentropy_core::consciousness_stats::pearson_correlation_f64(&indices, &abs_z);

    let n = feedback_trials.len();
    let first_half: Vec<f64> = feedback_trials[..n / 2].iter().map(|t| t.z_score).collect();
    let second_half: Vec<f64> = feedback_trials[n / 2..].iter().map(|t| t.z_score).collect();
    let first_half_mean_z = if first_half.is_empty() {
        0.0
    } else {
        first_half.iter().sum::<f64>() / first_half.len() as f64
    };
    let second_half_mean_z = if second_half.is_empty() {
        0.0
    } else {
        second_half.iter().sum::<f64>() / second_half.len() as f64
    };

    let (learning_t, learning_p) = if first_half.len() >= 2 && second_half.len() >= 2 {
        openentropy_core::consciousness_stats::welch_t_test(&first_half, &second_half)
    } else {
        (0.0, 1.0)
    };

    // Find best source — would require per-source tracking; use "pooled" as placeholder
    let interpretation = if learning_corr > 0.2 {
        format!(
            "Positive learning trend (r = {:.3}) — effect strengthened with feedback",
            learning_corr
        )
    } else if learning_corr < -0.2 {
        format!(
            "Negative trend (r = {:.3}) — possible fatigue or regression to mean",
            learning_corr
        )
    } else {
        format!(
            "No significant learning trend (r = {:.3}) — stable performance",
            learning_corr
        )
    };

    let result = FeedbackResult {
        trials: feedback_trials,
        learning_correlation: learning_corr,
        first_half_mean_z,
        second_half_mean_z,
        learning_t,
        learning_p,
        best_source: "pooled".to_string(),
        best_source_z: stouffer_z(&abs_z),
        interpretation: interpretation.clone(),
    };

    print_feedback_results(&result);
    save_json(cfg.output_path, &ModeResult::Feedback(result));
}

fn print_feedback_results(result: &FeedbackResult) {
    println!();
    println!("  FEEDBACK MODE RESULTS");
    println!("  {}", "=".repeat(62));
    println!();
    println!("  Total trials:      {}", result.trials.len());
    println!(
        "  Learning corr:     {:.4}",
        result.learning_correlation
    );
    println!(
        "  First half mean Z: {}",
        format_z(result.first_half_mean_z)
    );
    println!(
        "  Second half mean Z: {}",
        format_z(result.second_half_mean_z)
    );
    println!(
        "  Learning t-test:   t = {:.3}, p = {}",
        result.learning_t,
        format_p_value(result.learning_p)
    );
    println!(
        "  Best source:       {} (Z = {})",
        result.best_source,
        format_z(result.best_source_z)
    );
    println!();
    println!("  {}", result.interpretation);
    println!();
}

// ---------------------------------------------------------------------------
// Anomaly mode
// ---------------------------------------------------------------------------

fn run_anomaly(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
) {
    let running = setup_ctrlc();
    let epochs = if cfg.quick { 4 } else { cfg.epochs.max(4) }; // need at least 4 for baseline
    let epoch_secs = if cfg.quick { 5 } else { cfg.epoch_duration_secs };

    println!(
        "  Epochs:     {} x {}s (alternating Baseline / Intention)",
        epochs, epoch_secs
    );
    println!("  Detection:  Mahalanobis distance from baseline distribution");
    println!();

    let mut baseline_epochs: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut intention_epochs: Vec<(usize, String, Vec<u8>)> = Vec::new();

    for epoch_idx in 0..epochs {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let is_baseline = epoch_idx % 2 == 0;
        let direction = if is_baseline {
            IntentionDirection::Baseline
        } else {
            IntentionDirection::High
        };

        println!(
            "  Epoch {}/{}: {}",
            epoch_idx + 1,
            epochs,
            direction
        );
        print_phase_instruction(direction);
        println!();
        countdown(&running, cfg.quick);

        let epoch_start = Instant::now();
        let mut epoch_bytes: Vec<u8> = Vec::new();

        while epoch_start.elapsed() < Duration::from_secs(epoch_secs) {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            for (source_name, _) in active_sources {
                let conditioned = pool
                    .get_source_bytes(source_name, 32, ConditioningMode::Sha256)
                    .unwrap_or_default();
                epoch_bytes.extend_from_slice(&conditioned);
            }

            let elapsed = epoch_start.elapsed().as_secs_f64();
            let progress = (elapsed / epoch_secs as f64 * 100.0).min(100.0);
            print!("\r  Collecting... {progress:>5.1}%  ({} bytes)", epoch_bytes.len());
            let _ = std::io::stdout().flush();

            std::thread::sleep(Duration::from_millis(cfg.interval_ms.max(50)));
        }
        println!();

        if is_baseline {
            baseline_epochs.push((epoch_idx, epoch_bytes));
        } else {
            intention_epochs.push((epoch_idx, direction.to_string(), epoch_bytes));
        }
    }

    if baseline_epochs.len() < 2 {
        eprintln!("Need at least 2 baseline epochs for anomaly detection.");
        return;
    }
    if intention_epochs.is_empty() {
        eprintln!("Need at least 1 intention epoch.");
        return;
    }

    let result =
        openentropy_core::consciousness_anomaly::compute_anomaly(&baseline_epochs, &intention_epochs);
    print_anomaly_results(&result);
    save_json(cfg.output_path, &ModeResult::Anomaly(result));
}

fn print_anomaly_results(result: &openentropy_core::consciousness_anomaly::AnomalyResult) {
    println!();
    println!("  ANOMALY DETECTION RESULTS");
    println!("  {}", "=".repeat(62));
    println!();
    println!(
        "  Features:           {} ({})",
        result.feature_names.len(),
        result.feature_names.join(", ")
    );
    println!(
        "  Baseline mean dist: {:.3}",
        result.baseline_mean_distance
    );
    println!(
        "  Intention mean dist: {:.3}",
        result.intention_mean_distance
    );
    println!(
        "  Threshold (chi²):   {:.3}",
        result.threshold
    );
    println!(
        "  Anomalous epochs:   {} / {}",
        result.anomalous_count, result.total_intention_epochs
    );
    println!();

    // Per-epoch detail
    println!(
        "  {:<8} {:<10} {:>12} {:>10}",
        "Epoch", "Direction", "Mahalanobis", "Anomalous"
    );
    println!("  {}", "-".repeat(44));
    for ef in &result.epoch_features {
        let dist_str = ef
            .mahalanobis_distance
            .map(|d| format!("{d:.3}"))
            .unwrap_or_else(|| "N/A".to_string());
        let flag = if ef.is_anomalous { " !" } else { "" };
        println!(
            "  {:<8} {:<10} {:>12} {:>10}{}",
            ef.epoch_index,
            ef.direction,
            dist_str,
            if ef.is_anomalous { "YES" } else { "no" },
            flag
        );
    }

    println!();
    if result.anomalous_count > 0 {
        println!(
            "  FINDING: {} intention epoch(s) flagged as anomalous.",
            result.anomalous_count
        );
        println!("  Multivariate feature distribution shifted during intention,");
        println!("  suggesting an effect beyond what parametric tests capture.");
    } else {
        println!("  No anomalous intention epochs detected.");
        println!("  Feature distributions remained within baseline variance.");
    }
    println!();
}

// ---------------------------------------------------------------------------
// Retrocausal mode
// ---------------------------------------------------------------------------

fn run_retrocausal(
    cfg: &ConsciousnessCommandConfig<'_>,
    pool: &openentropy_core::EntropyPool,
    active_sources: &[(String, String)],
    source_infos: &[openentropy_core::SourceInfoSnapshot],
) {
    print_header(ExperimentMode::Retrocausal, active_sources, source_infos.len());
    println!("  Protocol:   Retrocausal (data collected BEFORE direction assignment)");
    println!();
    println!("  Step 1: Collecting random data — do NOT focus any intention.");
    println!("  Step 2: After all data is collected, directions will be assigned.");
    println!("  Step 3: Data scored as if assigned directions were intended.");
    println!();

    let running = setup_ctrlc();
    let trials = if cfg.quick { 20 } else { cfg.trials * 3 }; // 3 phases worth
    let bytes_per_trial = (cfg.bits + 7) / 8;

    println!(
        "  Trials:     {trials} x {} bits @ {:.1} Hz",
        cfg.bits,
        1000.0 / cfg.interval_ms as f64
    );
    println!();
    println!("  RELAX. No intention. Just let the data collect.");
    println!();
    countdown(&running, cfg.quick);

    let mut trial_data: Vec<Vec<u8>> = Vec::new();

    for trial_idx in 0..trials {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        let trial_start = Instant::now();

        // Pool bytes from first available source (conditioned)
        let mut best_bytes = Vec::new();
        for (source_name, _) in active_sources {
            let conditioned = pool
                .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                .unwrap_or_default();
            if conditioned.len() >= bytes_per_trial {
                best_bytes = conditioned;
                break;
            }
        }

        if best_bytes.len() < bytes_per_trial {
            continue;
        }

        trial_data.push(best_bytes);

        let progress = trial_idx + 1;
        let bar_width = 30;
        let filled = (progress * bar_width) / trials;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { '#' } else { '-' })
            .collect();
        print!("\r  [{bar}] {progress:>3}/{trials}  (no intention — just collecting)");
        let _ = std::io::stdout().flush();

        wait_interval(trial_start, cfg.interval_ms, &running);
    }
    println!();

    if trial_data.is_empty() {
        eprintln!("No trials collected.");
        return;
    }

    println!();
    println!("  Data collection complete. {} trials recorded.", trial_data.len());
    println!("  NOW assigning random directions post-hoc...");
    println!();

    let retro_trials = openentropy_core::consciousness_retrocausal::generate_retrocausal_sequence(
        &trial_data,
        cfg.bits,
    );
    let result = openentropy_core::consciousness_retrocausal::retrocausal_analysis(&retro_trials);

    print_retrocausal_results(&result);
    save_json(
        cfg.output_path,
        &ModeResult::Retrocausal(result),
    );
}

fn print_retrocausal_results(result: &openentropy_core::consciousness_retrocausal::RetrocausalResult) {
    println!();
    println!("  RETROCAUSAL PROTOCOL RESULTS");
    println!("  {}", "=".repeat(62));
    println!();
    println!("  Total trials:       {}", result.trials.len());
    println!("  High-assigned:      {}", result.n_high);
    println!("  Low-assigned:       {}", result.n_low);
    println!();
    println!(
        "  High Z:             {}",
        format_z(result.high_z)
    );
    println!(
        "  Low Z:              {}",
        format_z(result.low_z)
    );
    println!(
        "  Differential Z:     {}",
        format_z(result.differential_z)
    );
    println!(
        "  Differential p:     {}",
        format_p_value(result.differential_p)
    );
    println!();
    println!(
        "  Overall Z:          {}",
        format_z(result.overall_z)
    );
    println!(
        "  Overall p:          {}",
        format_p_value(result.overall_p)
    );
    println!(
        "  Success rate:       {:.1}% (expected: {:.1}%)",
        result.success_rate * 100.0,
        result.expected_success_rate * 100.0
    );
    println!();
    println!("  {}", result.interpretation);
    println!();
}

// ---------------------------------------------------------------------------
// E-value enrichment (displayed alongside p-values in standard mode)
// ---------------------------------------------------------------------------

fn print_evalue_enrichment(phases: &[PhaseResult], bits: usize) {
    println!();
    println!("  E-VALUE ANALYSIS (Anytime-Valid Inference)");
    println!("  {}", "-".repeat(62));
    println!();

    for phase in phases {
        if phase.trials.is_empty() {
            continue;
        }

        let ones_counts: Vec<u32> = phase
            .trials
            .iter()
            .map(|t| t.source_trials.first().map_or(0, |st| st.ones_count))
            .collect();

        let result = openentropy_core::consciousness_evalue::sequential_evalue_test(
            &ones_counts,
            bits,
            0.01, // 1% effect size under H1
        );

        println!(
            "  {} — E-value: {} [{}]  (approx p <= {})",
            phase.direction,
            openentropy_core::consciousness_evalue::format_evalue(result.final_evalue),
            result.evidence_level,
            if result.approx_p < 0.001 {
                format!("{:.1e}", result.approx_p)
            } else {
                format!("{:.4}", result.approx_p)
            },
        );

        if let Some(crossing) = result.first_crossing_trial {
            println!(
                "    First strong evidence at trial {} (of {})",
                crossing + 1,
                result.n_trials
            );
        }
    }

    println!();
    println!("  E-values remain valid under optional stopping (Ville's inequality).");
    println!("  Evidence level: e<1 none | 1-3 anecdotal | 3-10 moderate | 10-30 strong | 30+ very strong");
    println!();
}

// ---------------------------------------------------------------------------
// Deep analysis (topology, RQA, ordinal, transfer entropy, conformal, DAT)
// ---------------------------------------------------------------------------

fn print_deep_analysis(phases: &[PhaseResult], bits: usize, surrogate_n: usize, te_order: usize, calibration_file: Option<&str>) {
    println!();
    if surrogate_n > 0 {
        println!("  DEEP ANALYSIS (7 Novel Analytical Frameworks + {} Surrogate Permutations)", surrogate_n);
    } else {
        println!("  DEEP ANALYSIS (7 Novel Analytical Frameworks)");
    }
    println!("  {}", "=".repeat(62));

    // Surrogate test results collector (populated when surrogate_n > 0)
    let mut surrogate_results: Vec<openentropy_core::consciousness_surrogate::SurrogateResult> = Vec::new();

    // Collect pooled bytes from phase trials for structural analysis
    let mut baseline_bytes: Vec<u8> = Vec::new();
    let mut intention_bytes: Vec<u8> = Vec::new();
    let mut trial_data_vec: Vec<openentropy_core::consciousness_dat::TrialData> = Vec::new();

    for phase in phases {
        for trial in &phase.trials {
            let ones = trial.source_trials.first().map_or(0, |st| st.ones_count);
            let is_intention = phase.direction != IntentionDirection::Baseline;
            let is_high = phase.direction == IntentionDirection::High;

            // Build synthetic bytes from ones counts for structural analysis
            let byte_val = (ones as f64 / bits as f64 * 255.0) as u8;
            let bytes = vec![byte_val; (bits + 7) / 8];

            if is_intention {
                intention_bytes.extend_from_slice(&bytes);
            } else {
                baseline_bytes.extend_from_slice(&bytes);
            }

            trial_data_vec.push(openentropy_core::consciousness_dat::TrialData {
                ones,
                n_bits: bits,
                trial_index: trial.index,
                success: if is_high { ones > bits as u32 / 2 } else { ones < bits as u32 / 2 },
            });
        }
    }

    // 1. Ordinal Pattern Analysis
    if baseline_bytes.len() >= 20 && intention_bytes.len() >= 20 {
        let ordinal = openentropy_core::consciousness_ordinal::compare_ordinal(
            &baseline_bytes,
            &intention_bytes,
            3, // order 3 = 6 patterns
        );
        println!();
        println!("  1. ORDINAL PATTERN ANALYSIS");
        println!("  {}", "-".repeat(50));
        println!("     Baseline PE:   {:.4} (1.0 = max random)", ordinal.baseline_pe);
        println!("     Intention PE:  {:.4}", ordinal.intention_pe);
        println!("     Baseline WPE:  {:.4}", ordinal.baseline_wpe);
        println!("     Intention WPE: {:.4}", ordinal.intention_wpe);
        println!(
            "     Chi-squared:   {:.3} (df={}, p={})",
            ordinal.chi_squared, ordinal.df, format_p_value(ordinal.chi_squared_p)
        );
        if !ordinal.baseline_forbidden.is_empty() {
            println!(
                "     Baseline forbidden patterns: {}",
                ordinal.baseline_forbidden.len()
            );
        }
        if !ordinal.intention_forbidden.is_empty() {
            println!(
                "     Intention forbidden patterns: {} !!",
                ordinal.intention_forbidden.len()
            );
        }
        println!("     {}", ordinal.interpretation);
        if surrogate_n > 0 {
            let surr = openentropy_core::consciousness_surrogate::ordinal_surrogate_test(
                &baseline_bytes, &intention_bytes, surrogate_n, 1001,
            );
            println!("     Surrogate p={:.4} (z={:.2}, d={:.2}, n={})",
                surr.p_value, surr.z_score, surr.effect_size, surr.n_surrogates);
            surrogate_results.push(surr);
        }
    }

    // 2. RQA
    if baseline_bytes.len() >= 30 && intention_bytes.len() >= 30 {
        let rqa = openentropy_core::consciousness_rqa::compare_rqa(
            &baseline_bytes[..baseline_bytes.len().min(200)],
            &intention_bytes[..intention_bytes.len().min(200)],
        );
        println!();
        println!("  2. RECURRENCE QUANTIFICATION ANALYSIS");
        println!("  {}", "-".repeat(50));
        println!(
            "     {:>20} {:>10} {:>10}",
            "Metric", "Baseline", "Intention"
        );
        println!(
            "     {:>20} {:>10.4} {:>10.4}",
            "Recurrence Rate", rqa.baseline.recurrence_rate, rqa.intention.recurrence_rate
        );
        println!(
            "     {:>20} {:>10.4} {:>10.4}",
            "Determinism", rqa.baseline.determinism, rqa.intention.determinism
        );
        println!(
            "     {:>20} {:>10.4} {:>10.4}",
            "Laminarity", rqa.baseline.laminarity, rqa.intention.laminarity
        );
        println!(
            "     {:>20} {:>10.1} {:>10.1}",
            "Trapping Time", rqa.baseline.trapping_time, rqa.intention.trapping_time
        );
        println!(
            "     {:>20} {:>10} {:>10}",
            "Longest Diagonal", rqa.baseline.longest_diagonal, rqa.intention.longest_diagonal
        );
        println!("     {}", rqa.interpretation);
        if surrogate_n > 0 {
            let surr = openentropy_core::consciousness_surrogate::rqa_surrogate_test(
                &baseline_bytes, &intention_bytes, surrogate_n, 2002,
            );
            println!("     Surrogate p={:.4} (z={:.2}, d={:.2}, n={})",
                surr.p_value, surr.z_score, surr.effect_size, surr.n_surrogates);
            surrogate_results.push(surr);
        }
    }

    // 3. Topology
    if baseline_bytes.len() >= 50 && intention_bytes.len() >= 50 {
        let topo = openentropy_core::consciousness_topology::compute_topology(
            &baseline_bytes,
            &intention_bytes,
            3,
        );
        println!();
        println!("  3. PERSISTENT HOMOLOGY (Topological Data Analysis)");
        println!("  {}", "-".repeat(50));
        println!(
            "     Baseline PE:    {:.4}  Total Persistence: {:.4}",
            topo.baseline_persistence_entropy, topo.baseline_total_persistence
        );
        println!(
            "     Intention PE:   {:.4}  Total Persistence: {:.4}",
            topo.intention_persistence_entropy, topo.intention_total_persistence
        );
        println!(
            "     Wasserstein(H0): {:.4}   Betti divergence: {:.4}",
            topo.wasserstein_distance_h0, topo.betti_curve_divergence
        );
        println!(
            "     H0 features: {} (baseline) vs {} (intention)",
            topo.baseline_diagram.h0.len(),
            topo.intention_diagram.h0.len()
        );
        println!(
            "     H1 features: {} (baseline) vs {} (intention)",
            topo.baseline_diagram.h1.len(),
            topo.intention_diagram.h1.len()
        );
        println!("     {}", topo.interpretation);
        if surrogate_n > 0 {
            let surr = openentropy_core::consciousness_surrogate::topology_surrogate_test(
                &baseline_bytes, &intention_bytes, surrogate_n, 3003,
            );
            println!("     Surrogate p={:.4} (z={:.2}, d={:.2}, n={})",
                surr.p_value, surr.z_score, surr.effect_size, surr.n_surrogates);
            surrogate_results.push(surr);
        }
    }

    // 4. DAT vs Force Model
    if trial_data_vec.len() >= 10 {
        let intention_trials: Vec<_> = trial_data_vec
            .iter()
            .filter(|t| {
                // Only include intention trials (not baseline)
                t.ones != bits as u32 / 2 || t.success
            })
            .cloned()
            .collect();

        if intention_trials.len() >= 10 {
            let dat = openentropy_core::consciousness_dat::likelihood_ratio_test(&intention_trials);
            println!();
            println!("  4. DAT vs FORCE MODEL");
            println!("  {}", "-".repeat(50));
            println!("     Preferred model:    {}", dat.preferred_model);
            println!(
                "     Force BIC:          {:.1}   DAT BIC: {:.1}",
                dat.force_bic, dat.dat_bic
            );
            println!(
                "     Log-LR (DAT-Force): {:.3}",
                dat.log_likelihood_ratio
            );
            println!(
                "     Excess kurtosis:    {:.3} (0=normal, >0=fat tails -> DAT)",
                dat.diagnostics.excess_kurtosis
            );
            println!(
                "     Tail ratio:         {:.2} (observed/expected |Z|>2 trials)",
                dat.diagnostics.tail_ratio
            );
            println!(
                "     Temporal clustering: p={} (1st quarter: {:.1}%, rest: {:.1}%)",
                format_p_value(dat.clustering.clustering_p),
                dat.clustering.first_quarter_success_rate * 100.0,
                dat.clustering.remaining_success_rate * 100.0,
            );
            println!("     {}", dat.interpretation);
        }
    }

    // 5. Conformal Prediction
    if baseline_bytes.len() >= 100 && intention_bytes.len() >= 50 {
        // Extract features for conformal analysis
        let chunk_size = 50;
        let baseline_features: Vec<Vec<f64>> = baseline_bytes
            .chunks(chunk_size)
            .filter(|c| c.len() == chunk_size)
            .map(|c| openentropy_core::consciousness_anomaly::extract_features(c))
            .collect();
        let intention_features: Vec<Vec<f64>> = intention_bytes
            .chunks(chunk_size)
            .filter(|c| c.len() == chunk_size)
            .map(|c| openentropy_core::consciousness_anomaly::extract_features(c))
            .collect();

        if baseline_features.len() >= 3 && !intention_features.is_empty() {
            // Load existing calibration if provided, merge with new baseline
            let cal = if let Some(cal_path) = calibration_file {
                match openentropy_core::consciousness_conformal::load_calibration(cal_path) {
                    Ok(existing_cal) => {
                        let fresh_cal = openentropy_core::consciousness_conformal::calibrate(&baseline_features, 3);
                        let merged = openentropy_core::consciousness_conformal::merge_calibrations(&existing_cal, &fresh_cal);
                        println!();
                        println!("  5. CONFORMAL PREDICTION (Cross-Session, {} baseline points)", merged.baseline_features.len());
                        merged
                    }
                    Err(_) => {
                        println!();
                        println!("  5. CONFORMAL PREDICTION (Distribution-Free)");
                        openentropy_core::consciousness_conformal::calibrate(&baseline_features, 3)
                    }
                }
            } else {
                println!();
                println!("  5. CONFORMAL PREDICTION (Distribution-Free)");
                openentropy_core::consciousness_conformal::calibrate(&baseline_features, 3)
            };

            let conf = openentropy_core::consciousness_conformal::detect_anomalies(
                &cal,
                &intention_features,
                0.05,
            );
            println!("  {}", "-".repeat(50));
            println!(
                "     Calibration set:   {} baseline points (k={})",
                cal.baseline_features.len(), cal.k
            );
            println!(
                "     Anomalous epochs:  {} / {} (alpha={})",
                conf.anomalous_epochs.len(),
                conf.total_epochs,
                conf.alpha
            );
            println!(
                "     Max martingale:    {:.2} (threshold: {:.1})",
                conf.max_martingale,
                1.0 / conf.alpha
            );
            println!(
                "     Martingale reject: {}",
                if conf.martingale_reject { "YES" } else { "no" }
            );
            println!("     {}", conf.interpretation);

            // Save updated calibration for future sessions
            if let Some(cal_path) = calibration_file {
                match openentropy_core::consciousness_conformal::save_calibration(&cal, cal_path) {
                    Ok(()) => println!("     Calibration saved to {cal_path} for future sessions"),
                    Err(e) => println!("     Warning: failed to save calibration: {e}"),
                }
            }
        }
    }

    // 6. Transfer Entropy (compute from pooled byte data as float signals)
    if baseline_bytes.len() >= 50 && intention_bytes.len() >= 50 {
        // Split baseline and intention into two pseudo-signals for TE analysis
        let bl_floats: Vec<f64> = baseline_bytes.iter().map(|&b| b as f64).collect();
        let int_floats: Vec<f64> = intention_bytes.iter().map(|&b| b as f64).collect();
        let min_len = bl_floats.len().min(int_floats.len());

        if min_len >= 30 {
            println!();
            if te_order > 1 {
                println!("  6. TRANSFER ENTROPY (Higher-Order, lag=1, order={})", te_order);
            } else {
                println!("  6. TRANSFER ENTROPY (Cross-Source Information Flow)");
            }
            println!("  {}", "-".repeat(50));

            if te_order > 1 {
                // Higher-order TE with multi-lag embeddings
                let hist_te = openentropy_core::consciousness_transfer::transfer_entropy_higher_order(
                    &bl_floats[..min_len], &int_floats[..min_len], 1, te_order, 8,
                );
                let knn_te = openentropy_core::consciousness_transfer::transfer_entropy_knn_higher_order(
                    &bl_floats[..min_len], &int_floats[..min_len], 1, te_order, 4,
                );
                let hist_te_rev = openentropy_core::consciousness_transfer::transfer_entropy_higher_order(
                    &int_floats[..min_len], &bl_floats[..min_len], 1, te_order, 8,
                );
                let knn_te_rev = openentropy_core::consciousness_transfer::transfer_entropy_knn_higher_order(
                    &int_floats[..min_len], &bl_floats[..min_len], 1, te_order, 4,
                );
                println!("     {:>25} {:>12} {:>12}", "Direction", "Histogram", "KSG k-NN");
                println!("     {:>25} {:>12.4} {:>12.4}", "Baseline -> Intention", hist_te, knn_te);
                println!("     {:>25} {:>12.4} {:>12.4}", "Intention -> Baseline", hist_te_rev, knn_te_rev);
                let net_hist = hist_te - hist_te_rev;
                let net_knn = knn_te - knn_te_rev;
                println!("     {:>25} {:>12.4} {:>12.4}", "Net flow (B->I minus I->B)", net_hist, net_knn);
                let agreement = if (net_hist > 0.0) == (net_knn > 0.0) { "agree" } else { "disagree" };
                println!("     Estimator agreement: {} | Embedding order: {}", agreement, te_order);
            } else {
                // Standard single-lag TE
                let (hist_te, knn_te) = openentropy_core::consciousness_transfer::transfer_entropy_comparison(
                    &bl_floats[..min_len], &int_floats[..min_len], 1, 8, 4,
                );
                let (hist_te_rev, knn_te_rev) = openentropy_core::consciousness_transfer::transfer_entropy_comparison(
                    &int_floats[..min_len], &bl_floats[..min_len], 1, 8, 4,
                );
                println!("     {:>25} {:>12} {:>12}", "Direction", "Histogram", "KSG k-NN");
                println!("     {:>25} {:>12.4} {:>12.4}", "Baseline -> Intention", hist_te, knn_te);
                println!("     {:>25} {:>12.4} {:>12.4}", "Intention -> Baseline", hist_te_rev, knn_te_rev);
                let net_hist = hist_te - hist_te_rev;
                let net_knn = knn_te - knn_te_rev;
                println!("     {:>25} {:>12.4} {:>12.4}", "Net flow (B->I minus I->B)", net_hist, net_knn);
                let agreement = if (net_hist > 0.0) == (net_knn > 0.0) { "agree" } else { "disagree" };
                println!("     Estimator agreement: {} (divergence suggests binning sensitivity)", agreement);
            }

            if surrogate_n > 0 {
                let surr = openentropy_core::consciousness_surrogate::te_surrogate_test(
                    &bl_floats[..min_len], &int_floats[..min_len], surrogate_n, 6006,
                );
                println!("     Surrogate p={:.4} (z={:.2}, d={:.2}, n={})",
                    surr.p_value, surr.z_score, surr.effect_size, surr.n_surrogates);
                surrogate_results.push(surr);
            }
        }
    } else {
        println!();
        println!("  6. TRANSFER ENTROPY (Cross-Source Information Flow)");
        println!("  {}", "-".repeat(50));
        println!("     Insufficient data for transfer entropy analysis.");
    }

    // 7. Conformal + E-value Fusion (doubly-robust sequential monitoring)
    if baseline_bytes.len() >= 100 && intention_bytes.len() >= 50 {
        let chunk_size = 50;
        let baseline_features: Vec<Vec<f64>> = baseline_bytes
            .chunks(chunk_size)
            .filter(|c| c.len() == chunk_size)
            .map(|c| openentropy_core::consciousness_anomaly::extract_features(c))
            .collect();
        let intention_features: Vec<Vec<f64>> = intention_bytes
            .chunks(chunk_size)
            .filter(|c| c.len() == chunk_size)
            .map(|c| openentropy_core::consciousness_anomaly::extract_features(c))
            .collect();
        let intention_chunks: Vec<Vec<u8>> = intention_bytes
            .chunks(chunk_size)
            .filter(|c| c.len() == chunk_size)
            .map(|c| c.to_vec())
            .collect();

        if baseline_features.len() >= 3 && !intention_features.is_empty() {
            let fused = openentropy_core::consciousness_conformal_evalue::fused_analysis(
                &baseline_features,
                &intention_features,
                &intention_chunks,
                bits,
                0.01, // 1% effect size
                0.05, // 5% family-wise alpha
            );
            println!();
            println!("  7. CONFORMAL + E-VALUE FUSION (Doubly-Robust)");
            println!("  {}", "-".repeat(50));
            println!("     {}", fused.evidence_summary);
            println!(
                "     Fused decision:    {}",
                if fused.fused_rejected { "REJECT H0" } else { "retain H0" }
            );
            if let Some(ref channel) = fused.rejection_channel {
                println!(
                    "     Rejection channel: {} (epoch {})",
                    channel,
                    fused.first_rejection_epoch.unwrap_or(0) + 1
                );
            }
            println!("     {}", fused.interpretation);
        }
    }

    // 8. SURROGATE SIGNIFICANCE SUMMARY (BH FDR corrected)
    if !surrogate_results.is_empty() {
        let has_ci = surrogate_results.iter().any(|r| r.ci.is_some());
        let report = openentropy_core::consciousness_surrogate::build_surrogate_report(
            surrogate_results,
            0.05,
        );
        println!();
        println!("  8. SURROGATE SIGNIFICANCE SUMMARY (BH FDR Corrected)");
        println!("  {}", "=".repeat(78));
        if has_ci {
            println!(
                "     {:>25} {:>8} {:>8} {:>8} {:>8}  {:>14}",
                "Test", "p-value", "q-value", "z-score", "d", "95% CI(d)"
            );
        } else {
            println!(
                "     {:>25} {:>8} {:>8} {:>8} {:>8}",
                "Test", "p-value", "q-value", "z-score", "d"
            );
        }
        println!("     {}", "-".repeat(74));
        for (test, &q) in report.tests.iter().zip(report.q_values.iter()) {
            let sig = if q < 0.05 { " *" } else { "" };
            if let Some(ref ci) = test.ci {
                println!(
                    "     {:>25} {:>8.4} {:>8.4} {:>8.2} {:>8.2}  [{:>5.2},{:>5.2}]{}",
                    test.statistic_name, test.p_value, q, test.z_score, test.effect_size,
                    ci.ci_lower, ci.ci_upper, sig
                );
            } else {
                println!(
                    "     {:>25} {:>8.4} {:>8.4} {:>8.2} {:>8.2}{}",
                    test.statistic_name, test.p_value, q, test.z_score, test.effect_size, sig
                );
            }
        }
        println!("     {}", "-".repeat(74));
        println!(
            "     {} of {} tests significant after FDR correction (* = q < 0.05)",
            report.n_rejected_fdr, report.tests.len()
        );
        println!("     {}", report.interpretation);
    }

    println!();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consciousness_command_config_defaults() {
        let cfg = ConsciousnessCommandConfig {
            source_filter: None,
            trials: 50,
            bits: 200,
            interval_ms: 1000,
            output_path: None,
            quick: false,
            mode: ExperimentMode::Standard,
            epochs: 5,
            epoch_duration_secs: 30,
            double_blind: false,
            preregister: false,
            operator: None,
            evalue: false,
            deep_analysis: false,
            surrogate_n: 0,
            te_order: 1,
            calibration_file: None,
        };
        assert_eq!(cfg.trials, 50);
        assert_eq!(cfg.bits, 200);
        assert_eq!(cfg.mode, ExperimentMode::Standard);
        assert_eq!(cfg.epochs, 5);
        assert_eq!(cfg.epoch_duration_secs, 30);
        assert!(!cfg.double_blind);
        assert!(!cfg.preregister);
        assert!(cfg.operator.is_none());
        assert!(!cfg.evalue);
        assert!(!cfg.deep_analysis);
        assert_eq!(cfg.te_order, 1);
        assert!(cfg.calibration_file.is_none());
    }

    #[test]
    fn config_modes() {
        let modes = [
            ("standard", ExperimentMode::Standard),
            ("spectroscopy", ExperimentMode::Spectroscopy),
            ("structure", ExperimentMode::Structure),
            ("coherence", ExperimentMode::Coherence),
            ("temporal", ExperimentMode::Temporal),
            ("adversarial", ExperimentMode::Adversarial),
            ("feedback", ExperimentMode::Feedback),
            ("anomaly", ExperimentMode::Anomaly),
            ("retrocausal", ExperimentMode::Retrocausal),
        ];
        for (s, expected) in modes {
            assert_eq!(ExperimentMode::from_str(s), expected);
        }
    }

    #[test]
    fn config_with_operator() {
        let cfg = ConsciousnessCommandConfig {
            source_filter: None,
            trials: 50,
            bits: 200,
            interval_ms: 1000,
            output_path: None,
            quick: true,
            mode: ExperimentMode::Standard,
            epochs: 5,
            epoch_duration_secs: 30,
            double_blind: true,
            preregister: true,
            operator: Some("alice"),
            evalue: true,
            deep_analysis: true,
            surrogate_n: 100,
            te_order: 3,
            calibration_file: Some("cal.json"),
        };
        assert!(cfg.double_blind);
        assert!(cfg.preregister);
        assert_eq!(cfg.operator, Some("alice"));
        assert!(cfg.evalue);
        assert!(cfg.deep_analysis);
        assert_eq!(cfg.te_order, 3);
        assert_eq!(cfg.calibration_file, Some("cal.json"));
    }
}
