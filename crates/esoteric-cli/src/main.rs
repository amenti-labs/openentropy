//! CLI for esoteric-entropy — your computer is a quantum noise observatory.

mod commands;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "esoteric-entropy")]
#[command(about = "🔬 esoteric-entropy — your computer is a quantum noise observatory")]
#[command(version = esoteric_core::VERSION)]
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
    Bench,

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
    },

    /// Create a named pipe (FIFO) that continuously provides entropy
    Device {
        /// Path to FIFO
        #[arg(default_value = "/tmp/esoteric-rng")]
        path: String,

        /// Write buffer size in bytes
        #[arg(long, default_value = "4096")]
        buffer_size: usize,

        /// Comma-separated source name filter
        #[arg(long)]
        sources: Option<String>,
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
    },

    /// Run the entropy pool and output quality metrics
    Pool,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => commands::scan::run(),
        Commands::Probe { source_name } => commands::probe::run(&source_name),
        Commands::Bench => commands::bench::run(),
        Commands::Stream {
            format,
            rate,
            sources,
            bytes,
        } => commands::stream::run(&format, rate, sources.as_deref(), bytes),
        Commands::Device {
            path,
            buffer_size,
            sources,
        } => commands::device::run(&path, buffer_size, sources.as_deref()),
        Commands::Server {
            port,
            host,
            sources,
        } => commands::server::run(&host, port, sources.as_deref()),
        Commands::Monitor { refresh, sources } => {
            commands::monitor::run(refresh, sources.as_deref())
        }
        Commands::Report {
            samples,
            source,
            output,
        } => commands::report::run(samples, source.as_deref(), output.as_deref()),
        Commands::Pool => commands::pool::run(),
    }
}
