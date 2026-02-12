//! TUI rendering.

use ratatui::{prelude::*, widgets::*};

use super::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // RNG output
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    draw_title(f, chunks[0]);
    draw_main(f, chunks[1], app);
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

fn draw_main(f: &mut Frame, area: Rect, app: &App) {
    if app.show_info() {
        // Split: source table left, info panel right
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        draw_source_table(f, chunks[0], app);
        draw_info_panel(f, chunks[1], app);
    } else {
        // Split: source table top, chart bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(area);
        draw_source_table(f, chunks[0], app);
        draw_entropy_chart(f, chunks[1], app);
    }
}

fn draw_source_table(f: &mut Frame, area: Rect, app: &App) {
    let health = match app.health() {
        Some(h) => h,
        None => {
            let block = Block::default().borders(Borders::ALL).title(" Sources ");
            let p = Paragraph::new("Collecting...");
            f.render_widget(p.block(block), area);
            return;
        }
    };

    let header = Row::new(vec!["Source", "OK", "Bytes", "H", "Time", "Fail"])
        .style(Style::default().bold())
        .bottom_margin(1);

    let rows: Vec<Row> = health
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if i == app.selected() {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if s.healthy {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };

            let ok = if s.healthy { "✓" } else { "✗" };
            Row::new(vec![
                s.name.clone(),
                ok.to_string(),
                format!("{}", s.bytes),
                format!("{:.2}", s.entropy),
                format!("{:.3}s", s.time),
                format!("{}", s.failures),
            ])
            .style(style)
        })
        .collect();

    let title = format!(" Sources ({}/{} healthy) ", health.healthy, health.total);

    let table = Table::new(
        rows,
        [
            Constraint::Length(25),
            Constraint::Length(4),
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
        vec![
            Line::from(Span::styled(
                &info.name,
                Style::default().bold().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled("Description:", Style::default().bold())),
            Line::from(info.description.clone()),
            Line::from(""),
            Line::from(Span::styled("Category:", Style::default().bold())),
            Line::from(info.category.clone()),
            Line::from(""),
            Line::from(Span::styled(
                format!("Entropy Rate: {:.0} bits/s", info.entropy_rate_estimate),
                Style::default().bold(),
            )),
            Line::from(""),
            Line::from(Span::styled("Physics:", Style::default().bold())),
            Line::from(info.physics.clone()),
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

    let datasets = vec![
        Dataset::default()
            .name("avg H")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ];

    let x_max = (history.len() as f64).max(10.0);
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Entropy History (bits/byte) "),
        )
        .x_axis(Axis::default().bounds([0.0, x_max]).labels(vec![
            Line::from("0"),
            Line::from(format!("{}", history.len())),
        ]))
        .y_axis(Axis::default().bounds([0.0, 8.0]).labels(vec![
            Line::from("0"),
            Line::from("4"),
            Line::from("8"),
        ]));

    f.render_widget(chart, area);
}

fn draw_rng_output(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Live RNG Output ");
    let text = Paragraph::new(app.rng_output().to_string())
        .style(Style::default().fg(Color::Yellow))
        .block(block);
    f.render_widget(text, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let health = app.health();
    let status = if let Some(h) = health {
        format!(
            " Buffer: {} bytes | Output: {} bytes | q:quit  ↑↓:navigate  i:info  r:refresh",
            h.buffer_size, h.output_bytes
        )
    } else {
        " Loading...".to_string()
    };

    let bar = Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(bar, area);
}
