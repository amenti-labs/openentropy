pub fn run(host: &str, port: u16, source_filter: Option<&str>) {
    let pool = super::make_pool(source_filter);

    println!("🔬 Esoteric Entropy Server v{}", esoteric_core::VERSION);
    println!("   Listening on http://{host}:{port}");
    println!("   Sources: {}", pool.source_count());
    println!("   API: /api/v1/random?length=N&type=hex16|uint8|uint16");
    println!();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(esoteric_server::run_server(pool, host, port));
}
