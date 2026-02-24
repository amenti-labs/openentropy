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
    Scan,

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
        #[arg(long, default_value = "balanced", value_parser = ["balanced", "min_entropy", "throughput"])]
        rank_by: String,

        /// Write machine-readable benchmark report as JSON
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
    },

    /// Live interactive entropy dashboard (TUI)
    Monitor {
        /// Refresh rate in seconds
        #[arg(long, default_value = "1.0")]
        refresh: f64,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,
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

        /// Write analysis results as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// Run a consciousness-RNG experiment (PEAR Lab tripolar protocol).
    /// Tests whether focused human intention can influence hardware RNG output.
    /// Uses per-source differential analysis unique to OpenEntropy.
    ///
    /// Modes: standard (tripolar), spectroscopy (cross-domain), structure (info-theoretic),
    /// coherence (cross-source correlation).
    Consciousness {
        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Number of trials per phase (default: 50)
        #[arg(long, default_value = "50")]
        trials: usize,

        /// Bits per trial (default: 200, PEAR Lab standard)
        #[arg(long, default_value = "200")]
        bits: usize,

        /// Milliseconds between trials (default: 1000 = 1 Hz)
        #[arg(long, default_value = "1000")]
        interval: u64,

        /// Write experiment results as JSON
        #[arg(long)]
        output: Option<String>,

        /// Quick mode: 10 trials per phase for testing
        #[arg(long)]
        quick: bool,

        /// Experiment mode
        #[arg(long, default_value = "standard", value_parser = ["standard", "spectroscopy", "structure", "coherence", "temporal", "adversarial", "feedback", "anomaly", "retrocausal"])]
        mode: String,

        /// Number of epochs for structure/coherence/anomaly modes (default: 5)
        #[arg(long, default_value = "5")]
        epochs: usize,

        /// Duration of each epoch in seconds (default: 30)
        #[arg(long, default_value = "30")]
        epoch_duration: u64,

        /// Double-blind: randomize and hide intention direction from operator
        #[arg(long)]
        double_blind: bool,

        /// Pre-register experiment parameters (generates hash before experiment)
        #[arg(long)]
        preregister: bool,

        /// Operator name for profile tracking across sessions
        #[arg(long)]
        operator: Option<String>,

        /// Launch TUI dashboard with live Z-score visualization
        #[arg(long)]
        tui: bool,

        /// Enable e-value reporting (anytime-valid inference alongside p-values)
        #[arg(long)]
        evalue: bool,

        /// Run deep analysis: topology, RQA, ordinal patterns, DAT model, conformal prediction
        #[arg(long)]
        deep_analysis: bool,

        /// Run surrogate permutation tests for deep analysis (N = number of surrogates, e.g. 100)
        #[arg(long)]
        surrogate: Option<usize>,

        /// Transfer entropy embedding order for multi-lag coupling detection (default: 1)
        #[arg(long)]
        te_order: Option<usize>,

        /// Load/save conformal calibration file for cross-session baseline accumulation
        #[arg(long)]
        calibration_file: Option<String>,
    },

    /// Run N consciousness experiment sessions back-to-back with automatic meta-analysis.
    /// Accumulates statistical power over extended testing periods.
    ConsciousnessBatch {
        /// Number of sessions to run (default: 10)
        #[arg(long, default_value = "10")]
        sessions: usize,

        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Trials per phase per session (default: 50)
        #[arg(long, default_value = "50")]
        trials: usize,

        /// Bits per trial (default: 200)
        #[arg(long, default_value = "200")]
        bits: usize,

        /// Milliseconds between trials (default: 1000)
        #[arg(long, default_value = "1000")]
        interval: u64,

        /// Quick mode: 10 trials per phase
        #[arg(long)]
        quick: bool,

        /// Rest period between sessions in seconds (default: 10)
        #[arg(long, default_value = "10")]
        rest: u64,

        /// Output directory for per-session JSON and meta-analysis
        #[arg(long)]
        output: Option<String>,

        /// Operator name for profile tracking
        #[arg(long)]
        operator: Option<String>,
    },

    /// Networked multi-operator adversarial consciousness experiment.
    /// One machine hosts the experiment, remote operators connect via TCP.
    ConsciousnessNetwork {
        /// Run as host (server) collecting entropy and coordinating experiment
        #[arg(long)]
        host: bool,

        /// Connect to a remote host (e.g. "192.168.1.10:9042")
        #[arg(long)]
        connect: Option<String>,

        /// Port for hosting (default: 9042)
        #[arg(long, default_value = "9042")]
        port: u16,

        /// Operator name
        #[arg(long, default_value = "anonymous")]
        name: String,

        /// Comma-separated source name filter (host only)
        #[arg(long)]
        sources: Option<String>,

        /// Trials per phase (default: 50)
        #[arg(long, default_value = "50")]
        trials: usize,

        /// Bits per trial (default: 200)
        #[arg(long, default_value = "200")]
        bits: usize,

        /// Milliseconds between trials (default: 1000)
        #[arg(long, default_value = "1000")]
        interval: u64,

        /// Quick mode: 10 trials per phase
        #[arg(long)]
        quick: bool,

        /// Max operators to wait for before starting (default: 2)
        #[arg(long, default_value = "2")]
        max_operators: usize,

        /// Write experiment results as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// Meta-analyze multiple consciousness experiment JSON files.
    /// Computes combined Stouffer Z, forest plot, and per-source cross-session analysis.
    ConsciousnessMeta {
        /// JSON result files to analyze
        files: Vec<String>,

        /// Write combined meta-analysis as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// Long-running entropic weather station.
    /// Continuously monitors entropy source behavior over hours/days.
    ConsciousnessWeather {
        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Total duration in seconds (0 = indefinite, Ctrl+C to stop)
        #[arg(long, default_value = "0")]
        duration: u64,

        /// Seconds between epochs (default: 60)
        #[arg(long, default_value = "60")]
        interval: u64,

        /// Bits per epoch (default: 200)
        #[arg(long, default_value = "200")]
        bits: usize,

        /// Write results as JSON
        #[arg(long)]
        output: Option<String>,
    },

    /// View operator consciousness experiment profile.
    /// Shows session history, per-source responsiveness, and cumulative effect sizes.
    ConsciousnessProfile {
        /// Operator name to view
        operator: String,

        /// Directory for profile storage (default: consciousness_profiles)
        #[arg(long)]
        dir: Option<String>,
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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => commands::scan::run(),
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
        }),
        Commands::Analyze {
            sources,
            samples,
            output,
            cross_correlation,
            no_entropy,
            conditioning,
            view,
        } => commands::analyze::run(
            sources.as_deref(),
            output.as_deref(),
            samples,
            cross_correlation,
            !no_entropy,
            &conditioning,
            &view,
        ),
        Commands::Report {
            samples,
            source,
            output,
            conditioning,
        } => commands::report::run(samples, source.as_deref(), output.as_deref(), &conditioning),
        Commands::Record {
            sources,
            duration,
            tags,
            note,
            output,
            interval,
            analyze,
            conditioning,
        } => commands::record::run(
            &sources,
            duration.as_deref(),
            &tags,
            note.as_deref(),
            output.as_deref(),
            interval.as_deref(),
            analyze,
            &conditioning,
        ),
        Commands::Monitor { refresh, sources } => {
            commands::monitor::run(refresh, sources.as_deref())
        }
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
            output,
        } => commands::sessions::run(
            session.as_deref(),
            &dir,
            analyze,
            entropy,
            output.as_deref(),
        ),
        Commands::Consciousness {
            sources,
            trials,
            bits,
            interval,
            output,
            quick,
            mode,
            epochs,
            epoch_duration,
            double_blind,
            preregister,
            operator,
            tui,
            evalue,
            deep_analysis,
            surrogate,
            te_order,
            calibration_file,
        } => {
            let config = commands::consciousness::ConsciousnessCommandConfig {
                source_filter: sources.as_deref(),
                trials,
                bits,
                interval_ms: interval,
                output_path: output.as_deref(),
                quick,
                mode: openentropy_core::consciousness::ExperimentMode::from_str(&mode),
                epochs,
                epoch_duration_secs: epoch_duration,
                double_blind,
                preregister,
                operator: operator.as_deref(),
                evalue,
                deep_analysis,
                surrogate_n: surrogate.unwrap_or(0),
                te_order: te_order.unwrap_or(1),
                calibration_file: calibration_file.as_deref(),
            };
            if tui {
                commands::consciousness::run_with_tui(config);
            } else {
                commands::consciousness::run(config);
            }
        }
        Commands::ConsciousnessBatch {
            sessions,
            sources,
            trials,
            bits,
            interval,
            quick,
            rest,
            output,
            operator,
        } => commands::consciousness_batch::run(commands::consciousness_batch::BatchConfig {
            sessions,
            source_filter: sources.as_deref(),
            trials,
            bits,
            interval_ms: interval,
            quick,
            rest_secs: rest,
            output_dir: output.as_deref(),
            operator: operator.as_deref(),
        }),
        Commands::ConsciousnessNetwork {
            host,
            connect,
            port,
            name,
            sources,
            trials,
            bits,
            interval,
            quick,
            max_operators,
            output,
        } => commands::consciousness_network::run(commands::consciousness_network::NetworkConfig {
            host,
            connect: connect.as_deref(),
            port,
            name: &name,
            source_filter: sources.as_deref(),
            trials,
            bits,
            interval_ms: interval,
            quick,
            max_operators,
            output_path: output.as_deref(),
        }),
        Commands::ConsciousnessMeta { files, output } => {
            commands::consciousness_meta::run(commands::consciousness_meta::MetaAnalyzeConfig {
                files: &files,
                output_path: output.as_deref(),
            })
        }
        Commands::ConsciousnessWeather {
            sources,
            duration,
            interval,
            bits,
            output,
        } => commands::consciousness_weather::run(
            commands::consciousness_weather::WeatherConfig {
                source_filter: sources.as_deref(),
                duration_secs: duration,
                epoch_interval_secs: interval,
                output_path: output.as_deref(),
                bits_per_epoch: bits,
            },
        ),
        Commands::ConsciousnessProfile { operator, dir } => {
            commands::consciousness_profile::run(
                commands::consciousness_profile::ProfileConfig {
                    operator: &operator,
                    dir: dir.as_deref(),
                },
            )
        }
        Commands::Server {
            port,
            host,
            sources,
            allow_raw,
        } => commands::server::run(&host, port, sources.as_deref(), allow_raw),
    }
}
