//! CLI for openentropy — your computer is a hardware noise observatory.

mod commands;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "openentropy")]
#[command(about = "🔬 openentropy — your computer is a hardware noise observatory")]
#[command(version = openentropy_core::VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover available entropy sources on this machine
    Scan,

    /// Test a specific source and show quality stats
    Probe {
        /// Source name (or partial match)
        source_name: String,
    },

    /// Benchmark all available sources with a ranked report
    Bench {
        /// Comma-separated source name filter, or "all" for every source
        #[arg(long)]
        sources: Option<String>,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
    },

    /// Stream entropy to stdout
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

    /// Create a named pipe (FIFO) that continuously provides entropy
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

    /// Record a session of entropy collection from one or more sources
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

        /// Conditioning mode: raw (default for recording), vonneumann, sha256
        #[arg(long, default_value = "raw", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
    },

    /// Live interactive entropy dashboard
    Monitor {
        /// Refresh rate in seconds
        #[arg(long, default_value = "1.0")]
        refresh: f64,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,
    },

    /// Full NIST-inspired randomness test battery with report
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

    /// Deep min-entropy analysis (NIST SP 800-90B estimators)
    Entropy {
        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
    },

    /// Run the entropy pool and output quality metrics
    Pool {
        /// Comma-separated source name filter, or "all"
        #[arg(long)]
        sources: Option<String>,

        /// Conditioning mode: raw (none), vonneumann (debias only), sha256 (full, default)
        #[arg(long, default_value = "sha256", value_parser = ["raw", "vonneumann", "sha256"])]
        conditioning: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => commands::scan::run(),
        Commands::Probe { source_name } => commands::probe::run(&source_name),
        Commands::Bench {
            sources,
            conditioning,
        } => commands::bench::run(sources.as_deref(), &conditioning),
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
        Commands::Server {
            port,
            host,
            sources,
            allow_raw,
        } => commands::server::run(&host, port, sources.as_deref(), allow_raw),
        Commands::Record {
            sources,
            duration,
            tags,
            note,
            output,
            interval,
            conditioning,
        } => commands::record::run(
            &sources,
            duration.as_deref(),
            &tags,
            note.as_deref(),
            output.as_deref(),
            interval.as_deref(),
            &conditioning,
        ),
        Commands::Monitor { refresh, sources } => {
            commands::monitor::run(refresh, sources.as_deref())
        }
        Commands::Report {
            samples,
            source,
            output,
            conditioning,
        } => commands::report::run(samples, source.as_deref(), output.as_deref(), &conditioning),
        Commands::Entropy {
            sources,
            conditioning,
        } => commands::entropy::run(sources.as_deref(), &conditioning),
        Commands::Pool {
            sources,
            conditioning,
        } => commands::pool::run(sources.as_deref(), &conditioning),
    }
}
