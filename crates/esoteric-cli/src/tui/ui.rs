//! TUI rendering.
//!
//! Layout:
//! ┌─────────────────────────────────────────────┐
//! │              🔬 Esoteric Entropy             │
//! ├──────────────────────┬──────────────────────┤
//! │   Source Table       │  Selected Source Info │
//! │   (scrollable)       │  + Physics           │
//! │                      ├──────────────────────┤
//! │                      │  Entropy History      │
//! │                      │  (selected source)    │
//! ├──────────────────────┴──────────────────────┤
//! │  Live RNG Output                            │
//! ├─────────────────────────────────────────────┤
//! │  keybinds                                   │
//! └─────────────────────────────────────────────┘

use ratatui::{prelude::*, widgets::*};

use super::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(12),   // Main (table + info + chart)
            Constraint::Length(3), // RNG output
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    draw_title(f, chunks[0], app);
    draw_main(f, chunks[1], app);
    draw_rng_output(f, chunks[2], app);
    draw_status_bar(f, chunks[3]);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let enabled = app.toggles.iter().filter(|t| t.enabled()).count();
    let total = app.toggles.len();
    let ms = app.last_collection_ms();
    let bytes = app.total_bytes();
    let spinner = if app.is_collecting() { " ⟳" } else { "" };

    let title_text = format!(
        " 🔬 Esoteric Entropy   {enabled}/{total} sources   {bytes} bytes   {ms}ms/cycle{spinner} "
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_text)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);
}

fn draw_main(f: &mut Frame, area: Rect, app: &App) {
    // Left: source table, Right: info + chart stacked
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    draw_source_table(f, cols[0], app);

    // Right side: info panel on top, chart on bottom
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cols[1]);

    draw_info_panel(f, right[0], app);
    draw_entropy_chart(f, right[1], app);
}

fn draw_source_table(f: &mut Frame, area: Rect, app: &App) {
    let health = app.health();
    let toggles = &app.toggles;

    let header = Row::new(vec!["", "Source", "Cat", "⏱", "●", "H", "Bytes", "Time"])
        .style(Style::default().bold())
        .bottom_margin(1);

    let rows: Vec<Row> = toggles
        .iter()
        .enumerate()
        .map(|(i, toggle)| {
            let is_selected = i == app.selected();

            let health_info = health.as_ref().and_then(|h| {
                h.sources.iter().find(|s| s.name == toggle.name())
            });

            let pointer = if is_selected { "▸" } else { " " };
            let enabled = if toggle.enabled() { "●" } else { "○" };
            let speed = toggle.speed_tier().label();

            // Short category label
            let cat = match toggle.category() {
                "Timing" => "TMG",
                "System" => "SYS",
                "Network" => "NET",
                "Hardware" => "HW",
                "Silicon" => "SI",
                "CrossDomain" => "XD",
                "Novel" => "NOV",
                _ => "?",
            };

            let (entropy, bytes, time) = match health_info {
                Some(s) => (
                    format!("{:.1}", s.entropy),
                    format!("{}", s.bytes),
                    format!("{:.2}s", s.time),
                ),
                None if toggle.enabled() => ("...".into(), "...".into(), "...".into()),
                None => ("—".into(), "—".into(), "—".into()),
            };

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if !toggle.enabled() {
                Style::default().fg(Color::DarkGray)
            } else {
                match health_info {
                    Some(s) if s.entropy >= 7.5 => Style::default().fg(Color::Green),
                    Some(s) if s.entropy >= 5.0 => Style::default().fg(Color::Yellow),
                    Some(_) => Style::default().fg(Color::Red),
                    None => Style::default().fg(Color::White),
                }
            };

            Row::new(vec![
                pointer.to_string(),
                toggle.name().to_string(),
                cat.to_string(),
                speed.to_string(),
                enabled.to_string(),
                entropy,
                bytes,
                time,
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),  // pointer
            Constraint::Length(22), // name
            Constraint::Length(4),  // category
            Constraint::Length(3),  // speed
            Constraint::Length(2),  // enabled
            Constraint::Length(5),  // entropy
            Constraint::Length(7),  // bytes
            Constraint::Length(7),  // time
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sources "),
    );

    f.render_widget(table, area);
}

fn draw_info_panel(f: &mut Frame, area: Rect, app: &App) {
    let infos = app.source_infos();
    let selected = app.selected();

    let text = if selected < infos.len() {
        let info = &infos[selected];
        let toggle = &app.toggles[selected];
        let health = app.health();
        let health_info = health.as_ref().and_then(|h| {
            h.sources.iter().find(|s| s.name == toggle.name())
        });

        let status_line = if toggle.enabled() {
            match health_info {
                Some(s) => format!(
                    "ENABLED  H:{:.2}  {}B  {:.2}s",
                    s.entropy, s.bytes, s.time
                ),
                None => "ENABLED (collecting...)".into(),
            }
        } else {
            "DISABLED — press space to enable".into()
        };

        let status_style = if toggle.enabled() {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        vec![
            Line::from(vec![
                Span::styled(&info.name, Style::default().bold().fg(Color::Cyan)),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", toggle.category()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::raw(toggle.speed_tier().label()),
            ]),
            Line::from(Span::styled(status_line, status_style)),
            Line::from(""),
            Line::from(Span::styled(info.description.clone(), Style::default().italic())),
            Line::from(""),
            Line::from(Span::styled("Physics:", Style::default().bold())),
            Line::from(info.physics.clone()),
        ]
    } else {
        vec![Line::from("No source selected")]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " {} ",
            app.selected_name().unwrap_or("Info")
        ));
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).block(block);
    f.render_widget(paragraph, area);
}

fn draw_entropy_chart(f: &mut Frame, area: Rect, app: &App) {
    let history = app.selected_history();
    let source_name = app.selected_name().unwrap_or("none");

    if history.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {source_name} — no data yet "));
        let p = Paragraph::new("Enable source and wait for collection cycles")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(p, area);
        return;
    }

    let data: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let latest = history.last().copied().unwrap_or(0.0);
    let min = history.iter().copied().fold(f64::MAX, f64::min);
    let max = history.iter().copied().fold(f64::MIN, f64::max);

    let datasets = vec![Dataset::default()
        .name(format!("H={latest:.2}"))
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .data(&data)];

    let x_max = (history.len() as f64).max(10.0);
    // Auto-scale Y axis around the data range
    let y_min = (min - 0.5).max(0.0);
    let y_max = (max + 0.5).min(8.0);

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {source_name} entropy (bits/byte) ")),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, x_max])
                .labels(vec![
                    Line::from("0"),
                    Line::from(format!("{}", history.len())),
                ]),
        )
        .y_axis(
            Axis::default()
                .bounds([y_min, y_max])
                .labels(vec![
                    Line::from(format!("{y_min:.1}")),
                    Line::from(format!("{:.1}", (y_min + y_max) / 2.0)),
                    Line::from(format!("{y_max:.1}")),
                ]),
        );

    f.render_widget(chart, area);
}

fn draw_rng_output(f: &mut Frame, area: Rect, app: &App) {
    let output = app.rng_output();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Live Output ");
    let text = Paragraph::new(output)
        .style(Style::default().fg(Color::Yellow))
        .block(block);
    f.render_widget(text, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect) {
    let status =
        " space:toggle  1:fast  2:fast+med  3:all  n:none  r:refresh  q:quit";
    let bar = Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(bar, area);
}
