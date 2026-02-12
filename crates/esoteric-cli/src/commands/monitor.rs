pub fn run(refresh: f64, source_filter: Option<&str>) {
    let pool = super::make_pool(source_filter);
    let mut app = crate::tui::app::App::new(pool, refresh);
    if let Err(e) = app.run() {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}
