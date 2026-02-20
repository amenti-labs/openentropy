pub fn run(refresh: f64, source_filter: Option<&str>, include_telemetry: bool) {
    if super::telemetry::print_snapshot_if_enabled(include_telemetry, "monitor-startup").is_some() {
        println!();
    }
    let pool = super::make_pool(source_filter);
    let mut app = crate::tui::app::App::new(pool, refresh);
    if let Err(e) = app.run() {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}
