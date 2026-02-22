pub fn run(
    host: &str,
    port: u16,
    source_filter: Option<&str>,
    allow_raw: bool,
    include_telemetry: bool,
) {
    let pool = super::make_pool(source_filter);

    let base = format!("http://{host}:{port}");
    let n_sources = pool.source_count();

    println!("🔬 OpenEntropy Server v{}", openentropy_core::VERSION);
    println!("   {base}");
    println!("   {n_sources} entropy sources active");
    println!();
    println!("   Endpoints:");
    println!("     GET /                 API index (try: curl {base})");
    println!("     GET /api/v1/random    Random bytes from the mixed pool");
    println!("     GET /sources          List all sources with health metrics");
    println!("     GET /health           Pool health check");
    println!("     GET /pool/status      Detailed pool status");
    println!();
    println!("   Query params for /api/v1/random:");
    println!("     length=N              Bytes to return (1-65536, default: 1024)");
    println!("     type=hex16|uint8|uint16  Output format (default: hex16)");
    println!("     source=<name>         Request from a specific source");
    println!("     conditioning=sha256|vonneumann|raw");
    println!("   Query params for /sources and /pool/status:");
    println!("     telemetry=true        Include telemetry_v1 start/end report");
    if !allow_raw {
        println!("     (raw conditioning requires --allow-raw flag)");
    }
    println!();
    println!("   Examples:");
    println!("     curl {base}/api/v1/random?length=32&type=uint8");
    println!("     curl {base}/api/v1/random?source=clock_jitter&length=64");
    println!("     curl {base}/sources");
    println!("     curl {base}/sources?telemetry=true");
    println!("     curl {base}/pool/status?telemetry=true");
    println!();
    if super::telemetry::print_snapshot_if_enabled(include_telemetry, "server-startup").is_some() {
        println!();
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(openentropy_server::run_server(pool, host, port, allow_raw));
}
