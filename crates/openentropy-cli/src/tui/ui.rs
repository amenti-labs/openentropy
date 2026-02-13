//! TUI rendering — single-source focus design.
//!
//! ┌──────────────────────────────────────────────┐
//! │  🔬 OpenEntropy    cycle #42   32ms     │
//! ├─────────────────────┬────────────────────────┤
//! │  Sources            │  mach_timing           │
//! │  ▸ clock_jitter     │  Timing · Silicon      │
//! │    mach_timing  ●   │                        │
//! │    sleep_jitter     │  Mach absolute time    │
//! │    sysctl_deltas    │  LSB jitter from the   │
//! │    ...              │  performance counter   │
//! │                     ├────────────────────────┤
//! │                     │  ╭ entropy (bits/byte) │
//! │                     │  │  ~~~7.83~~~         │
//! │                     │  ╰──────────────────── │
//! ├─────────────────────┴────────────────────────┤
//! │  a3 f1 09 cc 7b 2e ...   256 bytes collected │
//! ├──────────────────────────────────────────────┤
//! │  ↑↓ navigate   space: select   r: refresh    │
//! └──────────────────────────────────────────────┘

use super::app::App;
use ratatui::{prelude::*, widgets::*};

pub fn draw(f: &mut Frame, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(10),   // main
            Constraint::Length(3), // output
            Constraint::Length(1), // keys
        ])
        .split(f.area());

    draw_title(f, rows[0], app);
    draw_main(f, rows[1], app);
    draw_output(f, rows[2], app);
    draw_keys(f, rows[3]);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let cycle = app.cycle_count();
    let ms = app.last_ms();
    let bytes = app.total_bytes();
    let spin = if app.is_collecting() { " ⟳" } else { "" };

    let active_label = app.active_name().unwrap_or("none");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(vec![
            Span::styled(" 🔬 OpenEntropy ", Style::default().bold().fg(Color::Cyan)),
            Span::raw("  watching: "),
            Span::styled(active_label, Style::default().bold().fg(Color::Yellow)),
            Span::styled(
                format!("  #{cycle}  {ms}ms  {bytes}B{spin} "),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

    f.render_widget(block, area);
}

fn draw_main(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_source_list(f, cols[0], app);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(cols[1]);

    draw_info(f, right[0], app);
    draw_chart(f, right[1], app);
}

fn draw_source_list(f: &mut Frame, area: Rect, app: &App) {
    let names = app.source_names();
    let cats = app.source_categories();

    let items: Vec<Row> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_cursor = i == app.cursor();
            let is_active = app.active() == Some(i);

            let pointer = if is_cursor { "▸" } else { " " };
            let marker = if is_active { "●" } else { " " };

            // Short category
            let cat = short_cat(&cats[i]);

            // Show last-known stats if we have them
            let stat = app.source_stat(name);
            let entropy_str = match &stat {
                Some(s) => format!("{:.1}", s.entropy),
                None => "—".into(),
            };
            let time_str = match &stat {
                Some(s) => format!("{:.2}s", s.time),
                None => "—".into(),
            };

            let style = if is_cursor {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if is_active {
                Style::default().fg(Color::Yellow).bold()
            } else {
                match &stat {
                    Some(s) if s.entropy >= 7.5 => Style::default().fg(Color::Green),
                    Some(s) if s.entropy >= 5.0 => Style::default().fg(Color::Yellow),
                    Some(_) => Style::default().fg(Color::Red),
                    None => Style::default().fg(Color::White),
                }
            };

            Row::new(vec![
                pointer.to_string(),
                marker.to_string(),
                name.clone(),
                cat.to_string(),
                entropy_str,
                time_str,
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        items,
        [
            Constraint::Length(2),  // pointer
            Constraint::Length(2),  // active marker
            Constraint::Length(22), // name
            Constraint::Length(4),  // category
            Constraint::Length(5),  // entropy
            Constraint::Length(7),  // time
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sources (space to select) "),
    );

    f.render_widget(table, area);
}

fn draw_info(f: &mut Frame, area: Rect, app: &App) {
    let infos = app.source_infos();
    let idx = app.active().unwrap_or(app.cursor());

    let text = if idx < infos.len() {
        let info = &infos[idx];
        let stat = app.source_stat(&info.name);

        let mut lines = vec![
            Line::from(Span::styled(
                &info.name,
                Style::default().bold().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                &info.category,
                Style::default().fg(Color::DarkGray),
            )),
        ];

        if let Some(s) = stat {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("H: ", Style::default().bold()),
                Span::styled(
                    format!("{:.3} bits/byte", s.entropy),
                    if s.entropy >= 7.5 {
                        Style::default().fg(Color::Green)
                    } else if s.entropy >= 5.0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
                Span::raw(format!("  {}B  {:.2}s", s.bytes, s.time)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(info.physics.clone()));

        lines
    } else {
        vec![Line::from("Select a source")]
    };

    let title = if let Some(idx) = app.active() {
        format!(" {} ", app.source_names()[idx])
    } else {
        " Info ".to_string()
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(text).wrap(Wrap { trim: true }).block(block);
    f.render_widget(p, area);
}

fn draw_chart(f: &mut Frame, area: Rect, app: &App) {
    let history = app.active_history();
    let name = app.active_name().unwrap_or("—");

    if history.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {name} — select a source "));
        let p = Paragraph::new("Press space on a source to start watching")
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
    let min_val = history.iter().copied().fold(f64::MAX, f64::min);
    let max_val = history.iter().copied().fold(f64::MIN, f64::max);

    let datasets = vec![
        Dataset::default()
            .name(format!("{latest:.2}"))
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ];

    let x_max = (history.len() as f64).max(10.0);
    let y_min = (min_val - 0.5).max(0.0);
    let y_max = (max_val + 0.5).min(8.0);

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {name}  H={latest:.2} bits/byte ")),
        )
        .x_axis(Axis::default().bounds([0.0, x_max]).labels(vec![
            Line::from("0"),
            Line::from(format!("{}", history.len())),
        ]))
        .y_axis(Axis::default().bounds([y_min, y_max]).labels(vec![
            Line::from(format!("{y_min:.1}")),
            Line::from(format!("{y_max:.1}")),
        ]));

    f.render_widget(chart, area);
}

fn draw_output(f: &mut Frame, area: Rect, app: &App) {
    let hex = app.rng_hex();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Live Output ");
    let p = Paragraph::new(hex)
        .style(Style::default().fg(Color::Yellow))
        .block(block);
    f.render_widget(p, area);
}

fn draw_keys(f: &mut Frame, area: Rect) {
    let bar = Paragraph::new(" ↑↓ navigate   space: select source   r: refresh   q: quit")
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(bar, area);
}

fn short_cat(cat: &str) -> &str {
    match cat {
        "Timing" => "TMG",
        "System" => "SYS",
        "Network" => "NET",
        "Hardware" => "HW",
        "Silicon" => "SI",
        "CrossDomain" => "XD",
        "Novel" => "NOV",
        _ => "?",
    }
}
