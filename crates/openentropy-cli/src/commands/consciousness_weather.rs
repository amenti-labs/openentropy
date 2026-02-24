//! `openentropy consciousness-weather` — long-running entropic weather station.
//!
//! Continuously monitors entropy source behavior over hours/days, recording
//! periodic epochs with Z-scores, info-theoretic measures, and optional
//! event labels. Searches for temporal patterns, extreme events, and
//! correlations with external events.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::consciousness::*;
use openentropy_core::consciousness_stats;

use super::make_pool;

pub struct WeatherConfig<'a> {
    pub source_filter: Option<&'a str>,
    pub duration_secs: u64,
    pub epoch_interval_secs: u64,
    pub output_path: Option<&'a str>,
    pub bits_per_epoch: usize,
}

pub fn run(cfg: WeatherConfig<'_>) {
    let pool = make_pool(cfg.source_filter);
    let source_infos = pool.source_infos();

    let active_sources: Vec<(String, String)> = source_infos
        .iter()
        .filter(|s| !s.composite)
        .map(|s| (s.name.clone(), s.category.clone()))
        .collect();

    if active_sources.is_empty() {
        eprintln!("Error: no entropy sources available");
        std::process::exit(1);
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    let n_epochs = if cfg.duration_secs > 0 {
        (cfg.duration_secs / cfg.epoch_interval_secs.max(1)) as usize
    } else {
        usize::MAX // infinite
    };

    println!();
    println!("  ENTROPIC WEATHER STATION");
    println!("  {}", "=".repeat(50));
    println!("  Sources:  {} active", active_sources.len());
    println!(
        "  Interval: {}s per epoch",
        cfg.epoch_interval_secs
    );
    if cfg.duration_secs > 0 {
        println!(
            "  Duration: {}s (~{} epochs)",
            cfg.duration_secs, n_epochs
        );
    } else {
        println!("  Duration: indefinite (Ctrl+C to stop)");
    }
    println!("  Press Ctrl+C to stop and save results.");
    println!();

    let bytes_per_trial = (cfg.bits_per_epoch + 7) / 8;
    let experiment_start = Instant::now();
    let mut epochs: Vec<WeatherEpoch> = Vec::new();

    for epoch_idx in 0..n_epochs {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let epoch_start = Instant::now();
        let timestamp_secs = experiment_start.elapsed().as_secs_f64();

        // Wall clock time
        let wall_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| {
                let secs = d.as_secs();
                let hours = (secs / 3600) % 24;
                let mins = (secs / 60) % 60;
                let secs = secs % 60;
                format!("{hours:02}:{mins:02}:{secs:02}")
            })
            .unwrap_or_else(|_| "??:??:??".to_string());

        // Collect from each source
        let mut source_z_scores: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        let mut all_bytes: Vec<u8> = Vec::new();

        for (source_name, _) in &active_sources {
            let conditioned = pool
                .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                .unwrap_or_default();

            if conditioned.len() >= bytes_per_trial {
                let ones = count_ones_n(&conditioned, cfg.bits_per_epoch);
                let z = trial_z_score(ones, cfg.bits_per_epoch);
                source_z_scores.insert(source_name.clone(), z);
                all_bytes.extend_from_slice(&conditioned);
            }
        }

        let pooled_z = if source_z_scores.is_empty() {
            0.0
        } else {
            source_z_scores.values().sum::<f64>() / source_z_scores.len() as f64
        };

        let flatness = consciousness_stats::spectral_flatness(&all_bytes);
        let lz76 = consciousness_stats::lz76_complexity(&all_bytes);

        let epoch = WeatherEpoch {
            index: epoch_idx,
            timestamp_secs,
            wall_time: wall_time.clone(),
            source_z_scores,
            pooled_z,
            event_label: None,
            spectral_flatness: flatness,
            lz76_complexity: lz76,
        };

        // Display
        let z_str = format_z(pooled_z);
        let bar_width = 20;
        let bar_center = bar_width / 2;
        let bar_pos = ((pooled_z / 3.0) * bar_center as f64) as i32 + bar_center as i32;
        let bar_pos = bar_pos.clamp(0, bar_width as i32 - 1) as usize;
        let mut bar = vec!['-'; bar_width];
        bar[bar_center] = '|';
        bar[bar_pos] = '#';
        let bar_str: String = bar.into_iter().collect();

        let extreme = if pooled_z.abs() > 2.0 { " !" } else { "" };
        print!(
            "\r  [{wall_time}] E{:>4} [{bar_str}] Z={z_str} f={flatness:.3} lz={lz76:.3}{extreme}",
            epoch_idx + 1
        );
        let _ = std::io::stdout().flush();

        epochs.push(epoch);

        // Auto-save every 100 epochs
        if epoch_idx > 0 && epoch_idx % 100 == 0 {
            if let Some(path) = cfg.output_path {
                let _ = save_weather_results(path, &epochs, experiment_start.elapsed().as_secs_f64());
            }
        }

        // Wait for next epoch
        let elapsed = epoch_start.elapsed();
        let interval = Duration::from_secs(cfg.epoch_interval_secs);
        if elapsed < interval {
            let deadline = Instant::now() + (interval - elapsed);
            while Instant::now() < deadline && running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    println!();
    println!();

    let duration_secs = experiment_start.elapsed().as_secs_f64();
    print_weather_results(&epochs, duration_secs);

    if let Some(path) = cfg.output_path {
        save_weather_results(path, &epochs, duration_secs);
        println!("  Results saved to {path}");
    }
}

fn save_weather_results(path: &str, epochs: &[WeatherEpoch], duration_secs: f64) {
    let result = build_weather_result(epochs, duration_secs);
    let mode_result = ModeResult::Weather(result);
    if let Ok(json) = serde_json::to_string_pretty(&mode_result) {
        let _ = std::fs::write(path, json);
    }
}

fn build_weather_result(epochs: &[WeatherEpoch], duration_secs: f64) -> WeatherResult {
    let z_scores: Vec<f64> = epochs.iter().map(|e| e.pooled_z).collect();
    let n = z_scores.len() as f64;

    let mean_z = if n > 0.0 {
        z_scores.iter().sum::<f64>() / n
    } else {
        0.0
    };
    let sd_z = if n > 1.0 {
        let var = z_scores.iter().map(|&z| (z - mean_z).powi(2)).sum::<f64>() / (n - 1.0);
        var.sqrt()
    } else {
        0.0
    };

    let extreme_count = z_scores.iter().filter(|&&z| z.abs() > 2.0).count();
    let expected_extreme = n * 2.0 * 0.0228; // two-tailed P(|Z| > 2)

    let labeled_events: Vec<(String, f64)> = epochs
        .iter()
        .filter_map(|e| {
            e.event_label
                .as_ref()
                .map(|label| (label.clone(), e.pooled_z))
        })
        .collect();

    WeatherResult {
        epochs: epochs.to_vec(),
        mean_z,
        sd_z,
        extreme_count,
        expected_extreme_count: expected_extreme,
        duration_secs,
        labeled_events,
    }
}

fn print_weather_results(epochs: &[WeatherEpoch], duration_secs: f64) {
    let result = build_weather_result(epochs, duration_secs);

    println!("  WEATHER STATION RESULTS");
    println!("  {}", "=".repeat(50));
    println!("  Total epochs:    {}", epochs.len());
    println!("  Duration:        {:.1}s ({:.1} min)", duration_secs, duration_secs / 60.0);
    println!("  Mean Z:          {}", format_z(result.mean_z));
    println!("  SD(Z):           {:.3}", result.sd_z);
    println!(
        "  Extreme epochs:  {} (expected: {:.1} under null)",
        result.extreme_count, result.expected_extreme_count
    );

    if result.extreme_count as f64 > result.expected_extreme_count * 2.0 && result.extreme_count > 3
    {
        println!();
        println!("  FINDING: More extreme epochs than expected under null hypothesis.");
    }

    // Time series summary
    if epochs.len() >= 10 {
        let n = epochs.len();
        let first_half: Vec<f64> = epochs[..n / 2].iter().map(|e| e.pooled_z).collect();
        let second_half: Vec<f64> = epochs[n / 2..].iter().map(|e| e.pooled_z).collect();
        let mean1 = first_half.iter().sum::<f64>() / first_half.len() as f64;
        let mean2 = second_half.iter().sum::<f64>() / second_half.len() as f64;
        println!();
        println!("  First half mean Z:  {}", format_z(mean1));
        println!("  Second half mean Z: {}", format_z(mean2));
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_config_defaults() {
        let cfg = WeatherConfig {
            source_filter: None,
            duration_secs: 3600,
            epoch_interval_secs: 60,
            output_path: None,
            bits_per_epoch: 200,
        };
        assert_eq!(cfg.duration_secs, 3600);
    }

    #[test]
    fn build_weather_result_empty() {
        let result = build_weather_result(&[], 0.0);
        assert_eq!(result.epochs.len(), 0);
        assert_eq!(result.extreme_count, 0);
    }
}
