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
    /// Use --source to probe a single source in detail.
    /// Includes a conditioned pool quality section by default.
    Bench {
        /// Probe a single source by name (partial match). Shows detailed quality stats.
        #[arg(long)]
        source: Option<String>,

        /// Comma-separated source name filter, or "all" for every source
        #[arg(long)]
        sources: Option<String>,

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
        #[arg(long, default_value = "balanced", value_parser = ["balanced", "min_entropy", "throughput", "quantum"])]
        rank_by: String,

        /// Include experimental quantum proxy diagnostics in table and JSON.
        /// Automatically enabled when using --rank-by quantum.
        #[arg(long)]
        quantum: bool,

        /// Include telemetry_v1 start/end environment snapshots in output.
        #[arg(long)]
        telemetry: bool,

        /// Run active stress sweeps (CPU/memory/scheduler load) for measured stress sensitivity.
        #[arg(long)]
        quantum_live_stress: bool,

        /// Path to prior calibration JSON for quantum proxy (defaults to seeded calibration).
        #[arg(long)]
        quantum_calibration: Option<String>,

        /// Write machine-readable benchmark report as JSON (`standard` + `experimental`)
        #[arg(long)]
        output: Option<String>,

        /// Skip conditioned pool output quality section
        #[arg(long)]
        no_pool: bool,
    },

    /// Statistical analysis: autocorrelation, spectral, bias, stationarity, runs.
    /// Min-entropy breakdown (MCV + diagnostics) is included by default.
    Analyze {
        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Number of samples to collect per source
        #[arg(long, default_value = "50000")]
        samples: usize,

        /// Write full results as JSON
        #[arg(long)]
        output: Option<String>,

        /// Compute cross-correlation matrix between all analyzed sources
        #[arg(long)]
        cross_correlation: bool,

        /// Skip min-entropy estimators per source
        #[arg(long)]
        no_entropy: bool,

        /// Conditioning mode for entropy analysis input: raw (default), vonneumann, sha256
        #[arg(long, default_value = "raw", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Output view: summary (default, verdict-driven) or detailed (full metrics)
        #[arg(long, default_value = "summary", value_parser = ["summary", "detailed"])]
        view: String,

        /// Estimate per-source and aggregate quantum:classical contribution proxy ratios
        #[arg(long)]
        quantum_ratio: bool,

        /// Include telemetry_v1 start/end environment snapshots in output.
        #[arg(long)]
        telemetry: bool,
    },

    /// Run NIST-inspired randomness test battery with pass/fail and p-values
    Report {
        /// Number of bytes to collect per source
        #[arg(long, default_value = "10000")]
        samples: usize,

        /// Test a single source
        #[arg(long)]
        source: Option<String>,

        /// Output path for report
        #[arg(long)]
        output: Option<String>,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,

        /// Include telemetry_v1 start/end environment snapshots in output.
        #[arg(long)]
        telemetry: bool,
    },

    /// Record entropy samples to disk for offline analysis
    Record {
        /// Comma-separated source names to record from
        #[arg(long)]
        sources: String,

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
    },

    /// Live interactive entropy dashboard (TUI)
    Monitor {
        /// Refresh rate in seconds
        #[arg(long, default_value = "1.0")]
        refresh: f64,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,

        /// Print a telemetry_v1 snapshot before launching the dashboard.
        #[arg(long)]
        telemetry: bool,
    },

    /// Stream raw entropy bytes to stdout (pipe-friendly)
    Stream {
        /// Output format
        #[arg(long, default_value = "raw", value_parser = ["raw", "hex", "base64"])]
        format: String,

        /// Bytes/sec rate limit (0 = unlimited)
        #[arg(long, default_value = "0")]
        rate: usize,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,

        /// Total bytes (0 = infinite)
        #[arg(long, default_value = "0")]
        bytes: usize,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
    },

    /// Create a FIFO (named pipe) that acts as an entropy device
    Device {
        /// Path to FIFO
        #[arg(default_value = "/tmp/openentropy-rng")]
        path: String,

        /// Write buffer size in bytes
        #[arg(long, default_value = "4096")]
        buffer_size: usize,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
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

        /// Estimate per-source and aggregate quantum:classical contribution proxy ratios
        #[arg(long)]
        quantum_ratio: bool,

        /// Include telemetry_v1 start/end environment snapshots in analysis output.
        #[arg(long)]
        telemetry: bool,

        /// Write analysis results as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// Start an HTTP entropy server (ANU QRNG API compatible)
    Server {
        /// Port to listen on
        #[arg(long, default_value = "8042")]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,

        /// Allow conditioning mode selection via ?conditioning=raw|vonneumann|sha256
        #[arg(long)]
        allow_raw: bool,

        /// Print a telemetry_v1 snapshot at server startup.
        #[arg(long)]
        telemetry: bool,
    },

    /// Capture telemetry_v1 as a standalone snapshot or timed window
    Telemetry {
        /// Window duration in seconds (0 = single snapshot).
        #[arg(long, default_value = "0")]
        window_sec: f64,

        /// Write telemetry JSON to path.
        #[arg(long)]
        output: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { telemetry } => commands::scan::run(telemetry),
        Commands::Bench {
            source,
            sources,
            conditioning,
            profile,
            samples_per_round,
            rounds,
            warmup_rounds,
            timeout_sec,
            rank_by,
            quantum,
            telemetry,
            quantum_live_stress,
            quantum_calibration,
            output,
            no_pool,
        } => commands::bench::run(commands::bench::BenchCommandConfig {
            source_filter: sources.as_deref(),
            conditioning: &conditioning,
            source: source.as_deref(),
            profile: &profile,
            samples_per_round,
            rounds,
            warmup_rounds,
            timeout_sec,
            rank_by: &rank_by,
            output_path: output.as_deref(),
            include_pool_quality: !no_pool,
            include_quantum: quantum,
            include_telemetry: telemetry,
            quantum_live_stress,
            quantum_calibration_path: quantum_calibration.as_deref(),
        }),
        Commands::Analyze {
            sources,
            samples,
            output,
            cross_correlation,
            no_entropy,
            conditioning,
            view,
            quantum_ratio,
            telemetry,
        } => commands::analyze::run(commands::analyze::AnalyzeCommandConfig {
            source_filter: sources.as_deref(),
            output_path: output.as_deref(),
            samples,
            cross_correlation,
            entropy: !no_entropy,
            conditioning: &conditioning,
            view: &view,
            quantum_ratio,
            include_telemetry: telemetry,
        }),
        Commands::Report {
            samples,
            source,
            output,
            conditioning,
            telemetry,
        } => commands::report::run(
            samples,
            source.as_deref(),
            output.as_deref(),
            &conditioning,
            telemetry,
        ),
        Commands::Record {
            sources,
            duration,
            tags,
            note,
            output,
            interval,
            analyze,
            conditioning,
            telemetry,
        } => commands::record::run(
            &sources,
            duration.as_deref(),
            &tags,
            note.as_deref(),
            output.as_deref(),
            interval.as_deref(),
            analyze,
            &conditioning,
            telemetry,
        ),
        Commands::Monitor {
            refresh,
            sources,
            telemetry,
        } => commands::monitor::run(refresh, sources.as_deref(), telemetry),
        Commands::Stream {
            format,
            rate,
            sources,
            bytes,
            conditioning,
        } => commands::stream::run(&format, rate, sources.as_deref(), bytes, &conditioning),
        Commands::Device {
            path,
            buffer_size,
            sources,
            conditioning,
        } => commands::device::run(&path, buffer_size, sources.as_deref(), &conditioning),
        Commands::Sessions {
            session,
            dir,
            analyze,
            entropy,
            quantum_ratio,
            telemetry,
            output,
        } => commands::sessions::run(
            session.as_deref(),
            &dir,
            analyze,
            entropy,
            output.as_deref(),
            quantum_ratio,
            telemetry,
        ),
        Commands::Server {
            port,
            host,
            sources,
            allow_raw,
            telemetry,
        } => commands::server::run(&host, port, sources.as_deref(), allow_raw, telemetry),
        Commands::Telemetry { window_sec, output } => {
            commands::telemetry::run(window_sec, output.as_deref())
        }
    }
}
