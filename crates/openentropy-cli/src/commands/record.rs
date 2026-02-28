//! `openentropy record` — record a session of entropy collection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use openentropy_core::conditioning::condition;
use openentropy_core::session::{SessionConfig, SessionMeta, SessionWriter};

pub struct RecordArgs {
    pub positional: Vec<String>,
    pub all: bool,
    pub duration: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub output: Option<String>,
    pub interval: Option<String>,
    pub analyze: bool,
    pub conditioning: String,
    pub include_telemetry: bool,
}

/// Run the record command.
#[allow(clippy::too_many_lines)]
pub fn run(args: RecordArgs) {
    // Parse conditioning mode
    let mode = super::parse_conditioning(&args.conditioning);

    // Resolve sources: positional names, --all, or default fast sources
    let sources = super::resolve_sources(&args.positional, args.all);

    // Build pool from the resolved sources for per-source raw byte collection
    let mut pool = openentropy_core::EntropyPool::new(None);
    let available: Vec<String> = sources.iter().map(|s| s.name().to_string()).collect();
    for source in sources {
        pool.add_source(source);
    }

    // Parse duration
    let max_duration = args.duration.as_deref().map(parse_duration);

    // Parse interval
    let interval_dur = args.interval.as_deref().map(parse_duration);

    // Parse tags
    let mut tag_map = HashMap::new();
    for tag in &args.tags {
        if let Some((k, v)) = tag.split_once(':') {
            tag_map.insert(k.to_string(), v.to_string());
        } else {
            eprintln!("Warning: ignoring malformed tag '{tag}' (expected key:value)");
        }
    }

    // Build session config
    let output_dir = args
        .output
        .as_ref()
        .map_or_else(|| PathBuf::from("sessions"), PathBuf::from);

    let include_telemetry = args.include_telemetry;

    let config = SessionConfig {
        sources: available.clone(),
        conditioning: mode,
        interval: interval_dur,
        output_dir,
        tags: tag_map,
        note: args.note.clone(),
        duration: max_duration,
        sample_size: 1000,
        include_analysis: args.analyze,
        include_telemetry,
    };

    // Create session writer
    let mut writer = match SessionWriter::new(config) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Error creating session: {e}");
            std::process::exit(1);
        }
    };

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }) {
        eprintln!("Warning: could not set Ctrl+C handler: {e}");
    }

    // Print session start info
    let session_dir = writer.session_dir().to_path_buf();
    println!("Recording session");
    println!("  Sources:   {}", available.join(", "));
    println!("  Conditioning: {mode}");
    if let Some(d) = max_duration {
        println!("  Duration:  {}s", d.as_secs());
    } else {
        println!("  Duration:  until Ctrl+C");
    }
    if let Some(iv) = interval_dur {
        println!("  Interval:  {}ms", iv.as_millis());
    } else {
        println!("  Interval:  continuous");
    }
    println!(
        "  Analysis:  {}",
        if args.analyze { "enabled" } else { "disabled" }
    );
    println!(
        "  Telemetry: {}",
        if include_telemetry {
            "enabled (session start/end snapshot)"
        } else {
            "disabled"
        }
    );
    println!("  Output:    {}", session_dir.display());
    println!();

    // Recording loop
    let start = Instant::now();
    let mut had_write_error = false;

    'outer: while running.load(Ordering::SeqCst) {
        // Check duration limit
        if let Some(max) = max_duration
            && start.elapsed() >= max
        {
            break;
        }

        // Collect from each source individually, bypassing the shared pool buffer
        // so raw/conditioned streams are always tied to the same sample.
        for source_name in &available {
            if !running.load(Ordering::SeqCst) {
                break 'outer;
            }

            let raw = pool
                .get_source_raw_bytes(source_name, 1000)
                .unwrap_or_default();
            if raw.is_empty() {
                continue;
            }

            let conditioned = condition(&raw, raw.len(), mode);

            if let Err(e) = writer.write_sample(source_name, &raw, &conditioned) {
                eprintln!("\nError writing sample: {e}");
                had_write_error = true;
                break 'outer;
            }
        }

        // Print status
        let elapsed = start.elapsed();
        let total = writer.total_samples();
        print!(
            "\r  Samples: {total:<8} Elapsed: {:.1}s",
            elapsed.as_secs_f64()
        );
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // Wait for interval if configured
        if let Some(iv) = interval_dur {
            let deadline = Instant::now() + iv;
            while Instant::now() < deadline && running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    println!();
    println!();

    if had_write_error {
        eprintln!("Recording stopped due to write error.");
    }

    // Finalize session
    match writer.finish() {
        Ok(dir) => {
            println!("Session saved to {}", dir.display());
            println!("  session.json          — metadata");
            println!("  samples.csv           — per-sample raw/conditioned metrics");
            println!("  raw.bin               — raw entropy bytes");
            println!("  raw_index.csv         — byte offset index for raw.bin");
            println!("  conditioned.bin       — conditioned entropy bytes");
            println!("  conditioned_index.csv — byte offset index for conditioned.bin");
            if include_telemetry {
                let meta_path = dir.join("session.json");
                if let Ok(raw) = std::fs::read_to_string(&meta_path)
                    && let Ok(meta) = serde_json::from_str::<SessionMeta>(&raw)
                    && let Some(t) = meta.telemetry_v1
                {
                    println!(
                        "  telemetry_v1:         {} ({:.1}s, {} metrics)",
                        t.model_id,
                        t.elapsed_ms as f64 / 1000.0,
                        t.end.metrics.len()
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Error finalizing session: {e}");
            std::process::exit(1);
        }
    }
}

/// Parse a duration string like "5m", "30s", "1h", "100ms".
fn parse_duration(s: &str) -> Duration {
    let s = s.trim();

    let (numeric, multiplier) = if let Some(rest) = s.strip_suffix("ms") {
        (rest, 1u64)
    } else if let Some(rest) = s.strip_suffix('s') {
        (rest, 1000)
    } else if let Some(rest) = s.strip_suffix('m') {
        (rest, 60_000)
    } else if let Some(rest) = s.strip_suffix('h') {
        (rest, 3_600_000)
    } else {
        // Assume seconds
        (s, 1000)
    };

    let value: u64 = numeric.parse().unwrap_or_else(|_| {
        eprintln!("Invalid duration: {s}");
        std::process::exit(1);
    });

    let millis = value.checked_mul(multiplier).unwrap_or_else(|| {
        eprintln!("Duration too large: {s}");
        std::process::exit(1);
    });
    Duration::from_millis(millis)
}
