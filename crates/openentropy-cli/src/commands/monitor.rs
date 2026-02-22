pub fn run(refresh: f64, source_filter: Option<&str>, include_telemetry: bool) {
    if super::telemetry::print_snapshot_if_enabled(include_telemetry, "monitor-startup").is_some() {
        println!();
    }
    // Monitor exposes the full source catalog so users can interactively
    // select any source (including slow sensor sources) at runtime.
    let pool = match source_filter {
        Some(filter) => super::make_pool(Some(filter)),
        None => super::make_pool(Some("all")),
    };
    let mut app = crate::tui::app::App::new(pool, refresh);
    if let Err(e) = app.run() {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}
