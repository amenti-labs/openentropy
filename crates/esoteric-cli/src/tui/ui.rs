//! TUI rendering.

use ratatui::{prelude::*, widgets::*};

use super::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),  // Main content
            Constraint::Length(5), // RNG output
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    draw_title(f, chunks[0]);

    match app.mode() {
        super::app::ViewMode::Dashboard => draw_dashboard(f, chunks[1], app),
        super::app::ViewMode::Stream => draw_stream(f, chunks[1], app),
    }

    draw_rng_output(f, chunks[2], app);
    draw_status_bar(f, chunks[3], app);
}

fn draw_title(f: &mut Frame, area: Rect) {
    let title = Block::default()
        .borders(Borders::ALL)
        .title(" 🔬 Esoteric Entropy Monitor ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(title, area);
}

fn draw_dashboard(f: &mut Frame, area: Rect, app: &App) {
    if app.show_info() {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        draw_source_table(f, chunks[0], app);
        draw_info_panel(f, chunks[1], app);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(area);
        draw_source_table(f, chunks[0], app);
        draw_entropy_chart(f, chunks[1], app);
    }
}

fn draw_source_table(f: &mut Frame, area: Rect, app: &App) {
    let health = app.health();
    let toggles = app.toggles();

    let header = Row::new(vec!["", "Source", "On", "Bytes", "H", "Time", "Fail"])
        .style(Style::default().bold())
        .bottom_margin(1);

    let rows: Vec<Row> = toggles
        .iter()
        .enumerate()
        .map(|(i, toggle)| {
            let is_selected = i == app.selected();

            // Find matching source in health report
            let health_info = health.as_ref().and_then(|h| {
                h.sources.iter().find(|s| s.name == toggle.name())
            });

            let enabled_marker = if toggle.enabled() { "●" } else { "○" };
            let pointer = if is_selected { "▶" } else { " " };

            let (bytes, entropy, time, failures, healthy) = match health_info {
                Some(s) => (
                    format!("{}", s.bytes),
                    format!("{:.2}", s.entropy),
                    format!("{:.3}s", s.time),
                    format!("{}", s.failures),
                    s.healthy,
                ),
                None => (
                    "—".into(),
                    "—".into(),
                    "—".into(),
                    "—".into(),
                    false,
                ),
            };

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if !toggle.enabled() {
                Style::default().fg(Color::DarkGray)
            } else if healthy {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Yellow)
            };

            Row::new(vec![
                pointer.to_string(),
                toggle.name().to_string(),
                enabled_marker.to_string(),
                bytes,
                entropy,
                time,
                failures,
            ])
            .style(style)
        })
        .collect();

    let enabled_count = toggles.iter().filter(|t| t.enabled()).count();
    let total = toggles.len();
    let collecting = if app.is_collecting() { " ⟳" } else { "" };
    let title = format!(" Sources ({enabled_count}/{total} enabled){collecting} ");

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(25),
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(table, area);
}

fn draw_info_panel(f: &mut Frame, area: Rect, app: &App) {
    let infos = app.source_infos();
    let selected = app.selected();

    let text = if selected < infos.len() {
        let info = &infos[selected];
        let toggle = &app.toggles()[selected];
        let status = if toggle.enabled() { "ENABLED ●" } else { "DISABLED ○" };
        vec![
            Line::from(Span::styled(
                &info.name,
                Style::default().bold().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                status,
                if toggle.enabled() {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            )),
            Line::from(""),
            Line::from(Span::styled("Category:", Style::default().bold())),
            Line::from(info.category.clone()),
            Line::from(""),
            Line::from(Span::styled("Description:", Style::default().bold())),
            Line::from(info.description.clone()),
            Line::from(""),
            Line::from(Span::styled("Physics:", Style::default().bold())),
            Line::from(info.physics.clone()),
            Line::from(""),
            Line::from(Span::styled(
                format!("Est. entropy rate: {:.0} bits/s", info.entropy_rate_estimate),
                Style::default().bold(),
            )),
        ]
    } else {
        vec![Line::from("No source selected")]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Info (i to toggle) ");
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).block(block);
    f.render_widget(paragraph, area);
}

fn draw_entropy_chart(f: &mut Frame, area: Rect, app: &App) {
    let history = app.entropy_history();
    if history.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Entropy History ");
        f.render_widget(block, area);
        return;
    }

    let data: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let datasets = vec![Dataset::default()
        .name("avg H")
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .data(&data)];

    let x_max = (history.len() as f64).max(10.0);
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Entropy History (bits/byte) "),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, x_max])
                .labels(vec![Line::from("0"), Line::from(format!("{}", history.len()))]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, 8.0])
                .labels(vec![Line::from("0"), Line::from("4"), Line::from("8")]),
        );

    f.render_widget(chart, area);
}

fn draw_stream(f: &mut Frame, area: Rect, app: &App) {
    let buf = app.stream_buffer();
    let visible_lines = area.height.saturating_sub(2) as usize;
    let start = buf.len().saturating_sub(visible_lines);
    let lines: Vec<Line> = buf[start..]
        .iter()
        .map(|s| Line::from(Span::styled(s.as_str(), Style::default().fg(Color::Yellow))))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Live Entropy Stream (s to toggle) ");
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_rng_output(f: &mut Frame, area: Rect, app: &App) {
    let output = app.rng_output();
    let total = app.total_bytes();
    let ms = app.last_collection_ms();

    let text = vec![
        Line::from(Span::styled(&output, Style::default().fg(Color::Yellow))),
        Line::from(Span::styled(
            format!("Total: {total} bytes | Last collection: {ms}ms"),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Latest Output ");
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let collecting = if app.is_collecting() { " ⟳ collecting..." } else { "" };
    let mode = match app.mode() {
        super::app::ViewMode::Dashboard => "dashboard",
        super::app::ViewMode::Stream => "stream",
    };

    let status = format!(
        " [{mode}]{collecting}  space:toggle  a:all  n:none  f:fast  s:stream  i:info  r:refresh  q:quit"
    );

    let bar = Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(bar, area);
}
