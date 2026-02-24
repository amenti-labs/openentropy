//! `openentropy consciousness-profile` — operator profiling.
//!
//! Manages operator profiles that track session history, per-source
//! responsiveness, and cumulative effect sizes across sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use openentropy_core::consciousness::*;

const PROFILES_DIR: &str = "consciousness_profiles";

pub struct ProfileConfig<'a> {
    /// Operator name to view/manage.
    pub operator: &'a str,
    /// Directory for profile storage.
    pub dir: Option<&'a str>,
}

pub fn run(cfg: ProfileConfig<'_>) {
    let profiles_dir = cfg.dir.unwrap_or(PROFILES_DIR);
    let profile_path = profile_file_path(profiles_dir, cfg.operator);

    let profile = load_profile(&profile_path);

    match profile {
        Some(profile) => print_profile(&profile),
        None => {
            println!();
            println!("  No profile found for operator '{}'.", cfg.operator);
            println!();
            println!("  Profiles are automatically created when you run experiments");
            println!("  with the --operator flag:");
            println!();
            println!(
                "    openentropy consciousness --operator {} --quick",
                cfg.operator
            );
            println!();
            println!(
                "  Profile location: {}",
                profile_path.display()
            );
            println!();
        }
    }
}

fn profile_file_path(dir: &str, operator: &str) -> PathBuf {
    let safe_name: String = operator
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    Path::new(dir).join(format!("{safe_name}.json"))
}

fn load_profile(path: &Path) -> Option<OperatorProfile> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save a profile to disk. Called from the consciousness command after each session.
pub fn save_profile(profile: &OperatorProfile, dir: &str) {
    let path = profile_file_path(dir, &profile.name);

    // Create directory if needed
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match serde_json::to_string_pretty(profile) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("  Warning: could not save profile: {e}");
            }
        }
        Err(e) => eprintln!("  Warning: could not serialize profile: {e}"),
    }
}

/// Load an existing profile or create a new one.
pub fn load_or_create(operator: &str, dir: &str) -> OperatorProfile {
    let path = profile_file_path(dir, operator);
    load_profile(&path).unwrap_or_else(|| OperatorProfile::new(operator))
}

/// Update profile with a standard experiment result.
pub fn update_with_result(
    profile: &mut OperatorProfile,
    result: &ExperimentResult,
    mode: ExperimentMode,
    preregistration_hash: Option<String>,
) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default();

    let source_z_scores: HashMap<String, f64> = result
        .source_differentials
        .iter()
        .map(|d| (d.source_name.clone(), d.differential_z))
        .collect();

    profile.add_session(OperatorSessionSummary {
        timestamp,
        mode,
        overall_z: result.overall_differential_z,
        overall_p: result.overall_p,
        source_z_scores,
        preregistration_hash,
    });
}

fn print_profile(profile: &OperatorProfile) {
    println!();
    println!("  OPERATOR PROFILE: {}", profile.name);
    println!("  {}", "=".repeat(50));
    println!("  Sessions:     {}", profile.total_sessions);
    println!(
        "  Combined Z:   {}",
        format_z(profile.combined_z)
    );
    println!(
        "  Combined p:   {}",
        format_p_value(profile.combined_p)
    );
    println!();

    // Session history
    if !profile.sessions.is_empty() {
        println!(
            "  {:<6} {:<12} {:>8} {:>10}",
            "#", "Mode", "Z", "p-value"
        );
        println!("  {}", "-".repeat(40));

        for (i, session) in profile.sessions.iter().enumerate() {
            let prereg_marker = if session.preregistration_hash.is_some() {
                " [P]"
            } else {
                ""
            };
            println!(
                "  {:<6} {:<12} {:>8} {:>10}{}",
                i + 1,
                session.mode.to_string(),
                format_z(session.overall_z),
                format_p_value(session.overall_p),
                prereg_marker
            );
        }
    }

    // Top sources
    if !profile.top_sources.is_empty() {
        println!();
        println!("  Most Responsive Sources:");
        println!("  {}", "-".repeat(40));
        for (name, responsiveness) in &profile.top_sources {
            let bar_len = (responsiveness * 20.0).min(30.0) as usize;
            let bar: String = "#".repeat(bar_len);
            println!(
                "    {:<20} {:.3} {bar}",
                name, responsiveness
            );
        }
    }

    // Cumulative Z trend
    if profile.sessions.len() >= 3 {
        println!();
        println!("  Cumulative Z Trend:");
        let z_scores: Vec<f64> = profile.sessions.iter().map(|s| s.overall_z).collect();
        let mut cum_z = Vec::new();
        for i in 1..=z_scores.len() {
            cum_z.push(stouffer_z(&z_scores[..i]));
        }
        for (i, z) in cum_z.iter().enumerate() {
            let bar_center = 15;
            let bar_pos = ((z / 3.0) * bar_center as f64) as i32 + bar_center as i32;
            let bar_pos = bar_pos.clamp(0, 29) as usize;
            let mut bar = vec![' '; 30];
            bar[bar_center] = '|';
            bar[bar_pos] = '#';
            let bar_str: String = bar.into_iter().collect();
            println!("    S{:>2} [{bar_str}] {}", i + 1, format_z(*z));
        }
    }

    println!();

    // Interpretation
    if profile.combined_p < 0.01 {
        println!(
            "  Strong cumulative evidence after {} sessions.",
            profile.total_sessions
        );
    } else if profile.combined_p < 0.05 {
        println!(
            "  Suggestive cumulative trend after {} sessions.",
            profile.total_sessions
        );
    } else {
        println!(
            "  No significant cumulative effect after {} sessions.",
            profile.total_sessions
        );
        if profile.total_sessions < 20 {
            println!("  PEAR Lab typically needed 50+ sessions for significance.");
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_file_path_sanitizes() {
        let path = profile_file_path("profiles", "Alice Bob!");
        assert!(path.to_str().unwrap().contains("Alice_Bob_"));
    }

    #[test]
    fn load_or_create_new() {
        let profile = load_or_create("test_user_nonexistent", "/tmp/oe_test_profiles_xx");
        assert_eq!(profile.name, "test_user_nonexistent");
        assert_eq!(profile.total_sessions, 0);
    }
}
