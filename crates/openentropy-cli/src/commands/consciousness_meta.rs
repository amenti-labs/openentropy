//! `openentropy consciousness-meta` — cross-session meta-analysis.
//!
//! Reads multiple JSON result files from consciousness experiments and
//! computes combined statistics, forest-plot-style summaries, and
//! session-by-session breakdown.

use openentropy_core::consciousness::*;
use std::path::Path;

pub struct MetaAnalyzeConfig<'a> {
    pub files: &'a [String],
    pub output_path: Option<&'a str>,
}

pub fn run(cfg: MetaAnalyzeConfig<'_>) {
    if cfg.files.is_empty() {
        eprintln!("Error: no JSON files provided");
        eprintln!("Usage: openentropy consciousness-meta <file1.json> <file2.json> ...");
        std::process::exit(1);
    }

    println!();
    println!("  CONSCIOUSNESS META-ANALYSIS");
    println!("  {}", "=".repeat(50));
    println!("  Sessions: {}", cfg.files.len());
    println!();

    let mut session_z_scores: Vec<f64> = Vec::new();
    let mut session_summaries: Vec<SessionSummary> = Vec::new();
    let mut source_z_across_sessions: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();
    let mut failed_files = 0;

    for file in cfg.files {
        let path = Path::new(file);
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Warning: cannot read {file}: {e}");
                failed_files += 1;
                continue;
            }
        };

        // Try to deserialize as ModeResult first, then ExperimentResult
        let result: Option<ExperimentResult> =
            if let Ok(mode_result) = serde_json::from_str::<ModeResult>(&content) {
                match mode_result {
                    ModeResult::Standard(r) => Some(r),
                    _ => {
                        eprintln!(
                            "  Warning: {file} is not a standard mode result, skipping"
                        );
                        failed_files += 1;
                        continue;
                    }
                }
            } else if let Ok(r) = serde_json::from_str::<ExperimentResult>(&content) {
                Some(r)
            } else {
                eprintln!("  Warning: cannot parse {file} as consciousness result");
                failed_files += 1;
                continue;
            };

        if let Some(result) = result {
            let z = result.overall_differential_z;
            let p = result.overall_p;
            let n_trials: usize = result.phases.iter().map(|ph| ph.trials.len()).sum();

            session_z_scores.push(z);
            session_summaries.push(SessionSummary {
                file: file.clone(),
                z,
                p,
                n_trials,
                n_sources: result.source_differentials.len(),
            });

            // Accumulate per-source differentials
            for diff in &result.source_differentials {
                source_z_across_sessions
                    .entry(diff.source_name.clone())
                    .or_default()
                    .push(diff.differential_z);
            }
        }
    }

    if session_summaries.is_empty() {
        eprintln!("Error: no valid session files found");
        return;
    }

    // Combined Z via Stouffer
    let combined_z = stouffer_z(&session_z_scores);
    let combined_p = z_to_p_two_tailed(combined_z);

    // Session-by-session table
    println!(
        "  {:<30} {:>8} {:>8} {:>10} {:>8}",
        "File", "Z", "p-value", "Trials", "Sources"
    );
    println!("  {}", "-".repeat(68));

    for s in &session_summaries {
        let short_name = Path::new(&s.file)
            .file_name()
            .map_or(&s.file[..], |f| f.to_str().unwrap_or(&s.file));
        let short = if short_name.len() > 28 {
            &short_name[..28]
        } else {
            short_name
        };
        println!(
            "  {:<30} {:>8} {:>8} {:>10} {:>8}",
            short,
            format_z(s.z),
            format_p_value(s.p),
            s.n_trials,
            s.n_sources
        );
    }

    // Forest plot (ASCII)
    println!();
    println!("  Forest Plot (Z-scores):");
    println!("  {}", "-".repeat(60));
    let max_abs_z = session_z_scores
        .iter()
        .map(|z| z.abs())
        .fold(0.0f64, f64::max)
        .max(1.0);

    for (i, s) in session_summaries.iter().enumerate() {
        let bar_width = 30;
        let center = bar_width / 2;
        let pos = ((s.z / max_abs_z) * center as f64) as i32 + center as i32;
        let pos = pos.clamp(0, bar_width as i32 - 1) as usize;

        let mut bar = vec![' '; bar_width];
        bar[center] = '|';
        bar[pos] = if s.p < 0.05 { '*' } else { 'o' };

        let bar_str: String = bar.into_iter().collect();
        println!("  S{:>2} [{bar_str}] Z={:>6}", i + 1, format_z(s.z));
    }

    // Combined line
    let combined_pos =
        ((combined_z / max_abs_z) * 15.0) as i32 + 15;
    let combined_pos = combined_pos.clamp(0, 29) as usize;
    let mut combined_bar = vec![' '; 30];
    combined_bar[15] = '|';
    combined_bar[combined_pos] = '#';
    let combined_bar_str: String = combined_bar.into_iter().collect();
    println!("  {}", "-".repeat(60));
    println!(
        "  ALL [{combined_bar_str}] Z={:>6}",
        format_z(combined_z)
    );

    // Combined result
    println!();
    println!(
        "  Combined result: Z = {}, p = {}",
        format_z(combined_z),
        format_p_value(combined_p)
    );
    println!(
        "  Sessions: {} analyzed, {} failed",
        session_summaries.len(),
        failed_files
    );

    // Per-source meta-analysis
    if !source_z_across_sessions.is_empty() {
        println!();
        println!("  Per-Source Cross-Session Analysis (Stouffer combined):");
        println!("  {}", "-".repeat(58));
        println!(
            "  {:<24} {:>8} {:>8} {:>10}",
            "Source", "Combined Z", "p-value", "Sessions"
        );
        println!("  {}", "-".repeat(58));

        let mut source_results: Vec<(String, f64, f64, usize)> = source_z_across_sessions
            .iter()
            .map(|(name, zs)| {
                let combined = stouffer_z(zs);
                let p = z_to_p_two_tailed(combined);
                (name.clone(), combined, p, zs.len())
            })
            .collect();
        source_results.sort_by(|a, b| {
            b.1.abs()
                .partial_cmp(&a.1.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for (name, z, p, n) in &source_results {
            let marker = if *name == "prng_control" {
                " [CTRL]"
            } else {
                ""
            };
            println!(
                "  {:<24} {:>8} {:>8} {:>10}{}",
                name,
                format_z(*z),
                format_p_value(*p),
                n,
                marker
            );
        }
    }

    // Interpretation
    println!();
    println!("  {}", "-".repeat(50));
    if combined_p < 0.01 {
        println!("  Strong cumulative evidence of intention effect (p < 0.01).");
    } else if combined_p < 0.05 {
        println!("  Suggestive cumulative evidence (p < 0.05).");
        println!("  Additional sessions recommended.");
    } else {
        println!(
            "  No significant cumulative effect (p = {:.3}).",
            combined_p
        );
        println!("  PEAR Lab accumulated over thousands of sessions.");
    }
    println!();

    // Save combined JSON if requested
    if let Some(path) = cfg.output_path {
        let output = MetaAnalysisOutput {
            session_summaries: session_summaries.clone(),
            combined_z,
            combined_p,
            n_sessions: session_summaries.len(),
        };
        match serde_json::to_string_pretty(&output) {
            Ok(json) => match std::fs::write(path, &json) {
                Ok(()) => println!("  Meta-analysis saved to {path}"),
                Err(e) => eprintln!("  Error writing: {e}"),
            },
            Err(e) => eprintln!("  Error serializing: {e}"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct SessionSummary {
    file: String,
    z: f64,
    p: f64,
    n_trials: usize,
    n_sources: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MetaAnalysisOutput {
    session_summaries: Vec<SessionSummary>,
    combined_z: f64,
    combined_p: f64,
    n_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_config_basic() {
        let files = vec!["a.json".to_string()];
        let cfg = MetaAnalyzeConfig {
            files: &files,
            output_path: None,
        };
        assert_eq!(cfg.files.len(), 1);
    }
}
