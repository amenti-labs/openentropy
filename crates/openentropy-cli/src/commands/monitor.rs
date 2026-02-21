pub fn run(refresh: f64, source_filter: Option<&str>) {
    // Monitor should expose the full source catalog so users can interactively
    // select camera/muon/light-sensitive sources at runtime.
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
