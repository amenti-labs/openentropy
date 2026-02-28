//! CLI for openentropy — your computer is a hardware noise observatory.

mod commands;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "openentropy")]
#[command(about = "openentropy — your computer is a hardware noise observatory")]
#[command(version = openentropy_core::VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available entropy sources on this machine
    Scan {
        /// Include a telemetry_v1 snapshot after source discovery.
        #[arg(long)]
        telemetry: bool,
    },

    /// Benchmark sources: Shannon entropy, min-entropy, grade, speed.
    /// Pass a single source name to probe it in detail.
    /// Includes a conditioned pool quality section by default.
    Bench {
        /// Source name(s) — positional, optional.
        /// One name: detailed single-source probe. Multiple: filter bench run.
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Comma-separated source name filter (hidden, use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Include all sources (including slow ones)
        #[arg(long)]
        all: bool,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Benchmark profile: quick (<10s), standard (default), deep (higher confidence)
        #[arg(long, default_value = "standard", value_parser = ["quick", "standard", "deep"])]
        profile: String,

        /// Override samples collected from each source per round
        #[arg(long)]
        samples_per_round: Option<usize>,

        /// Override number of measured rounds
        #[arg(long)]
        rounds: Option<usize>,

        /// Override number of warmup rounds (not scored)
        #[arg(long)]
        warmup_rounds: Option<usize>,

        /// Override per-round collection timeout in seconds
        #[arg(long)]
        timeout_sec: Option<f64>,

        /// Ranking strategy
        #[arg(long, default_value = "balanced", value_parser = ["balanced", "min_entropy", "throughput"])]
        rank_by: String,

        /// Include telemetry_v1 start/end environment snapshots in output.
        #[arg(long)]
        telemetry: bool,

        /// Write machine-readable benchmark report as JSON (includes optional telemetry_v1)
        #[arg(long)]
        output: Option<String>,

        /// Skip conditioned pool output quality section
        #[arg(long)]
        no_pool: bool,

        /// QCicada QRNG post-processing mode
        #[arg(long, value_parser = ["raw", "sha256", "samples"])]
        qcicada_mode: Option<String>,
    },

    /// Forensic source analysis: autocorrelation, spectral, bias, stationarity, runs.
    /// Deep statistical tests that bench doesn't cover.
    /// Use --report to run the NIST-inspired test battery with pass/fail and p-values.
    Analyze {
        /// Source name(s) — positional, optional
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Comma-separated source name filter (hidden, use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Include all sources (including slow ones)
        #[arg(long)]
        all: bool,

        /// Number of samples to collect per source
        #[arg(long, default_value = "50000")]
        samples: usize,

        /// Write full results as JSON (or Markdown when --report is used)
        #[arg(long)]
        output: Option<String>,

        /// Compute cross-correlation matrix between all analyzed sources
        #[arg(long)]
        cross_correlation: bool,

        /// Include min-entropy breakdown (MCV + diagnostic estimators) per source
        #[arg(long)]
        entropy: bool,

        /// Conditioning mode: raw (default), vonneumann, sha256
        #[arg(long, default_value = "raw", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Include telemetry_v1 start/end environment snapshots in output.
        #[arg(long)]
        telemetry: bool,

        /// Run NIST-inspired randomness test battery with pass/fail, p-values, and scores.
        /// When combined with --output, writes a Markdown report.
        #[arg(long)]
        report: bool,

        /// QCicada QRNG post-processing mode
        #[arg(long, value_parser = ["raw", "sha256", "samples"])]
        qcicada_mode: Option<String>,
    },

    /// Record entropy samples to disk for offline analysis
    Record {
        /// Source name(s) to record from
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Comma-separated source names (hidden, use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Include all sources (including slow ones)
        #[arg(long)]
        all: bool,

        /// Maximum recording duration (e.g. "5m", "30s", "1h")
        #[arg(long)]
        duration: Option<String>,

        /// Metadata tags as key:value pairs
        #[arg(long = "tag")]
        tags: Vec<String>,

        /// Session note
        #[arg(long)]
        note: Option<String>,

        /// Output directory (default: ./sessions/)
        #[arg(long)]
        output: Option<String>,

        /// Sample interval (e.g. "100ms", "1s"); default: continuous
        #[arg(long)]
        interval: Option<String>,

        /// Include end-of-session statistical analysis in session.json
        #[arg(long)]
        analyze: bool,

        /// Conditioning mode: raw (default for recording), vonneumann, sha256
        #[arg(long, default_value = "raw", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Store telemetry_v1 start/end snapshots in session.json.
        #[arg(long)]
        telemetry: bool,

        /// QCicada QRNG post-processing mode
        #[arg(long, value_parser = ["raw", "sha256", "samples"])]
        qcicada_mode: Option<String>,
    },

    /// Live interactive entropy dashboard (TUI)
    Monitor {
        /// Source name to pre-select in TUI
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Refresh rate in seconds
        #[arg(long, default_value = "1.0")]
        refresh: f64,

        /// Comma-separated source name filter (hidden, use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Print a telemetry_v1 snapshot before launching the dashboard.
        #[arg(long)]
        telemetry: bool,
    },

    /// Stream raw entropy bytes to stdout (pipe-friendly).
    /// Use --fifo to create a named pipe that acts as an entropy device.
    Stream {
        /// Source name(s) — positional, optional.
        /// One source: direct stream (no pool). Multiple: pooled.
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Output format (stdout mode only)
        #[arg(long, default_value = "raw", value_parser = ["raw", "hex", "base64"])]
        format: String,

        /// Bytes/sec rate limit (0 = unlimited); in FIFO mode, sets the write buffer size
        #[arg(long, default_value = "0")]
        rate: usize,

        /// Comma-separated source name filter (hidden, use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Total bytes (0 = infinite, stdout mode only)
        #[arg(long, default_value = "0")]
        bytes: usize,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Force pool mode even with a single source
        #[arg(long)]
        pool: bool,

        /// Include all sources (including slow ones)
        #[arg(long)]
        all: bool,

        /// Create a FIFO (named pipe) at this path and feed entropy to readers
        #[arg(long)]
        fifo: Option<String>,

        /// QCicada QRNG post-processing mode
        #[arg(long, value_parser = ["raw", "sha256", "samples"])]
        qcicada_mode: Option<String>,
    },

    /// List and analyze recorded entropy sessions
    Sessions {
        /// Path to a specific session directory to inspect or analyze
        session: Option<String>,

        /// Directory containing session recordings (default: ./sessions/)
        #[arg(long, default_value = "sessions")]
        dir: String,

        /// Run full statistical analysis on the session's raw data
        #[arg(long)]
        analyze: bool,

        /// Also run min-entropy estimators per source (MCV + diagnostics)
        #[arg(long)]
        entropy: bool,

        /// Include telemetry_v1 start/end environment snapshots in analysis output.
        #[arg(long)]
        telemetry: bool,

        /// Write analysis results as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// Start an HTTP entropy server (ANU QRNG API compatible)
    Server {
        /// Source name(s) to include in the pool
        #[arg(value_name = "SOURCE")]
        source: Vec<String>,

        /// Port to listen on
        #[arg(long, default_value = "8042")]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Comma-separated source name filter (hidden: use positional args instead)
        #[arg(long, hide = true)]
        sources: Option<String>,

        /// Allow conditioning mode selection via ?conditioning=raw|vonneumann|sha256
        #[arg(long)]
        allow_raw: bool,

        /// Print a telemetry_v1 snapshot at server startup.
        #[arg(long)]
        telemetry: bool,
    },
}

fn main() {
    // Ensure terminal is in cooked mode. A previous TUI crash or ctrl-c may
    // have left raw mode enabled, which breaks newline handling for all
    // subsequent CLI commands (println! outputs \n without \r).
    let _ = crossterm::terminal::disable_raw_mode();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { telemetry } => commands::scan::run(telemetry),
        Commands::Bench {
            source,
            sources,
            all,
            conditioning,
            profile,
            samples_per_round,
            rounds,
            warmup_rounds,
            timeout_sec,
            rank_by,
            telemetry,
            output,
            no_pool,
            qcicada_mode,
        } => {
            commands::apply_qcicada_mode(qcicada_mode.as_deref());
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            commands::bench::run(commands::bench::BenchArgs {
                positional,
                all,
                conditioning,
                profile,
                samples_per_round,
                rounds,
                warmup_rounds,
                timeout_sec,
                rank_by,
                output,
                no_pool,
                include_telemetry: telemetry,
            })
        }
        Commands::Analyze {
            source,
            sources,
            all,
            samples,
            output,
            cross_correlation,
            entropy,
            conditioning,
            telemetry,
            report,
            qcicada_mode,
        } => {
            commands::apply_qcicada_mode(qcicada_mode.as_deref());
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            commands::analyze::run(commands::analyze::AnalyzeArgs {
                positional,
                all,
                samples,
                output,
                cross_correlation,
                entropy,
                conditioning,
                include_telemetry: telemetry,
                report,
            })
        }
        Commands::Record {
            source,
            sources,
            all,
            duration,
            tags,
            note,
            output,
            interval,
            analyze,
            conditioning,
            telemetry,
            qcicada_mode,
        } => {
            commands::apply_qcicada_mode(qcicada_mode.as_deref());
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            if positional.is_empty() && !all {
                eprintln!("Error: at least one source name or --all is required for recording.");
                eprintln!("Run 'openentropy scan' to list available sources.");
                std::process::exit(1);
            }
            commands::record::run(commands::record::RecordArgs {
                positional,
                all,
                duration,
                tags,
                note,
                output,
                interval,
                analyze,
                conditioning,
                include_telemetry: telemetry,
            })
        }
        Commands::Monitor {
            source,
            refresh,
            sources,
            telemetry,
        } => {
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            let source_filter = if !positional.is_empty() {
                Some(positional.join(","))
            } else {
                None
            };
            commands::monitor::run(refresh, source_filter.as_deref(), telemetry)
        }
        Commands::Stream {
            source,
            format,
            rate,
            sources,
            bytes,
            conditioning,
            pool,
            all,
            fifo,
            qcicada_mode,
        } => {
            commands::apply_qcicada_mode(qcicada_mode.as_deref());
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            commands::stream::run(commands::stream::StreamArgs {
                positional,
                format,
                rate,
                bytes,
                conditioning,
                pool,
                all,
                fifo,
            })
        }
        Commands::Sessions {
            session,
            dir,
            analyze,
            entropy,
            telemetry,
            output,
        } => commands::sessions::run(
            session.as_deref(),
            &dir,
            analyze,
            entropy,
            output.as_deref(),
            telemetry,
        ),
        Commands::Server {
            source,
            port,
            host,
            sources,
            allow_raw,
            telemetry,
        } => {
            let positional = merge_positional_and_legacy(&source, sources.as_deref());
            let source_filter = if positional.is_empty() {
                None
            } else {
                Some(positional.join(","))
            };
            commands::server::run(&host, port, source_filter.as_deref(), allow_raw, telemetry)
        }
    }
}

/// Merge positional source args with legacy `--sources` flag.
/// Positional args take priority; the flag is a hidden backward-compat fallback.
fn merge_positional_and_legacy(positional: &[String], legacy: Option<&str>) -> Vec<String> {
    if !positional.is_empty() {
        return positional.to_vec();
    }
    if let Some(filter) = legacy {
        return filter
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    vec![]
}
