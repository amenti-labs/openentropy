//! TUI rendering — single-source focus design.
//!
//! All draw functions receive an `&App` (non-shared fields) and `&Snapshot`
//! (shared state captured in a single mutex lock per frame).

use super::app::{App, ChartMode, Sample, Snapshot, rolling_autocorr};
use openentropy_core::ConditioningMode;
use ratatui::{prelude::*, widgets::*};

// ---------------------------------------------------------------------------
// Category lookup (single source of truth for short_cat + display_cat)
// ---------------------------------------------------------------------------

const CATEGORIES: &[(&str, &str, &str)] = &[
    ("thermal", "THM", "Thermal"),
    ("timing", "TMG", "Timing"),
    ("scheduling", "SCH", "Scheduling"),
    ("io", "I/O", "I/O"),
    ("ipc", "IPC", "IPC"),
    ("microarch", "uAR", "Microarch"),
    ("gpu", "GPU", "GPU"),
    ("network", "NET", "Network"),
    ("system", "SYS", "System"),
    ("composite", "CMP", "Composite"),
    ("signal", "SIG", "Signal"),
    ("sensor", "SNS", "Sensor"),
];

fn short_cat(cat: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|(k, _, _)| *k == cat)
        .map(|(_, s, _)| *s)
        .unwrap_or("?")
}

fn display_cat(cat: &str) -> &str {
    CATEGORIES
        .iter()
        .find(|(k, _, _)| *k == cat)
        .map(|(_, _, d)| *d)
        .unwrap_or(cat)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn entropy_color(val: f64) -> Style {
    if val >= 7.5 {
        Style::default().fg(Color::Green)
    } else if val >= 5.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Red)
    }
}

fn format_time(secs: f64) -> String {
    if secs >= 1.0 {
        format!("{secs:.1}s")
    } else {
        format!("{:.1}ms", secs * 1000.0)
    }
}

/// Build spans for entropy values with coloring.
fn entropy_spans(label: &str, label_style: Style, h: f64, h_min: f64) -> Vec<Span<'static>> {
    vec![
        Span::styled(label.to_string(), label_style),
        Span::styled("Shannon ", Style::default().bold()),
        Span::styled(format!("{h:.3}"), entropy_color(h)),
        Span::styled("  NIST min ", Style::default().bold()),
        Span::styled(format!("{h_min:.3}"), entropy_color(h_min)),
    ]
}

/// Extract chart values from history, handling autocorrelation specially.
fn extract_chart_values(history: &[Sample], mode: ChartMode) -> Vec<f64> {
    if mode == ChartMode::Autocorrelation {
        let raw: Vec<f64> = history.iter().map(|s| s.output_value).collect();
        rolling_autocorr(&raw, 60)
    } else {
        history.iter().map(|s| mode.value_from(s)).collect()
    }
}

/// Render a placeholder block with a gray message (used for empty states).
fn draw_placeholder(f: &mut Frame, area: Rect, title: String, message: &str) {
    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(message)
        .style(Style::default().fg(Color::DarkGray))
        .block(block);
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Main draw entry point
// ---------------------------------------------------------------------------

pub fn draw(f: &mut Frame, app: &mut App) {
    let snap = app.snapshot();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(10),   // main
            Constraint::Length(4), // output
            Constraint::Length(1), // keys
        ])
        .split(f.area());

    draw_title(f, rows[0], app, &snap);
    draw_main(f, rows[1], app, &snap);
    draw_output(f, rows[2], app, &snap);
    draw_keys(f, rows[3]);
}

// ---------------------------------------------------------------------------
// Title bar
// ---------------------------------------------------------------------------

fn draw_title(f: &mut Frame, area: Rect, app: &App, snap: &Snapshot) {
    let rate = app.refresh_rate_secs();
    let spin = if app.is_paused() {
        " PAUSED"
    } else if snap.collecting {
        " ⟳"
    } else {
        ""
    };

    let active_label = app.active_name().unwrap_or("none");
    let rate_str = if rate >= 1.0 {
        format!("{rate:.0}s")
    } else {
        format!("{:.0}ms", rate * 1000.0)
    };

    let mut title_spans = vec![
        Span::styled(" 🔬 OpenEntropy ", Style::default().bold().fg(Color::Cyan)),
        Span::raw("  watching: "),
        Span::styled(active_label, Style::default().bold().fg(Color::Yellow)),
    ];

    if let Some(cmp_name) = app.compare_name() {
        title_spans.push(Span::styled(
            format!(" vs {cmp_name}"),
            Style::default().bold().fg(Color::Magenta),
        ));
    }

    title_spans.extend([
        Span::styled(
            format!(
                "  #{}  {}ms  {}B ",
                snap.cycle_count, snap.last_ms, snap.total_bytes
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(" @{rate_str}"),
            Style::default().bold().fg(Color::Magenta),
        ),
        Span::styled(format!("{spin} "), Style::default().fg(Color::DarkGray)),
    ]);

    if app.is_recording() {
        let rec_elapsed = app
            .recording_elapsed()
            .map(|d| format!("{:.0}s", d.as_secs_f64()))
            .unwrap_or_default();
        title_spans.push(Span::styled(
            format!(" REC {} {}smp ", rec_elapsed, app.recording_samples()),
            Style::default().bold().fg(Color::White).bg(Color::Red),
        ));
    }

    if let Some(path) = &snap.last_export {
        title_spans.push(Span::styled(
            format!(" saved: {} ", path.display()),
            Style::default().fg(Color::Green),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(title_spans));

    f.render_widget(block, area);
}

// ---------------------------------------------------------------------------
// Main area (sources + info + chart)
// ---------------------------------------------------------------------------

fn draw_main(f: &mut Frame, area: Rect, app: &mut App, snap: &Snapshot) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_source_list(f, cols[0], app, snap);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(cols[1]);

    draw_info(f, right[0], app, snap);
    draw_chart(f, right[1], app, snap);
}

// ---------------------------------------------------------------------------
// Source list
// ---------------------------------------------------------------------------

fn draw_source_list(f: &mut Frame, area: Rect, app: &mut App, snap: &Snapshot) {
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
            let cat = short_cat(&cats[i]);

            let stat = snap.source_stats.get(name.as_str());
            let entropy_str = match stat {
                Some(s) => format!("{:.1}", s.entropy),
                None => "—".into(),
            };
            let time_str = match stat {
                Some(s) => format_time(s.time),
                None => "—".into(),
            };

            let style = if is_cursor {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if is_active {
                Style::default().fg(Color::Yellow).bold()
            } else {
                match stat {
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

    let header = Row::new(vec!["", "", "Source", "Cat", "H", "Time"])
        .style(Style::default().bold().fg(Color::DarkGray))
        .bottom_margin(0);

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
    .header(header)
    .row_highlight_style(Style::default()) // cursor styling is manual (per-row)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sources (space to select) "),
    );

    f.render_stateful_widget(table, area, app.table_state_mut());
}

// ---------------------------------------------------------------------------
// Info panel
// ---------------------------------------------------------------------------

fn draw_info(f: &mut Frame, area: Rect, app: &App, snap: &Snapshot) {
    let infos = app.source_infos();
    let idx = app.active().unwrap_or(app.cursor());

    let text = if idx < infos.len() {
        let info = &infos[idx];
        let stat = snap.source_stats.get(info.name.as_str());

        let mut lines = vec![
            Line::from(Span::styled(
                &info.name,
                Style::default().bold().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                display_cat(&info.category),
                Style::default().fg(Color::DarkGray),
            )),
        ];

        if let Some(s) = stat {
            lines.push(Line::from(""));
            lines.push(Line::from(entropy_spans(
                "last ",
                Style::default().fg(Color::DarkGray),
                s.entropy,
                s.min_entropy,
            )));
        }

        if snap.active_history.len() >= 2 {
            let n = snap.active_history.len() as f64;
            let avg_sh: f64 = snap.active_history.iter().map(|s| s.shannon).sum::<f64>() / n;
            let avg_min: f64 = snap
                .active_history
                .iter()
                .map(|s| s.min_entropy)
                .sum::<f64>()
                / n;
            let mut spans = entropy_spans(
                "avg  ",
                Style::default().bold().fg(Color::Magenta),
                avg_sh,
                avg_min,
            );
            spans.push(Span::styled(
                format!("  n={}", snap.active_history.len()),
                Style::default().fg(Color::DarkGray),
            ));
            lines.push(Line::from(spans));
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

// ---------------------------------------------------------------------------
// Chart
// ---------------------------------------------------------------------------

fn draw_chart(f: &mut Frame, area: Rect, app: &App, snap: &Snapshot) {
    let mode = app.chart_mode();
    let name = app.active_name().unwrap_or("—");

    if mode == ChartMode::ByteDistribution {
        draw_byte_dist(f, area, snap, name);
        return;
    }

    if snap.active_history.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — select a source "),
            "Press space on a source to start watching",
        );
        return;
    }

    let values = extract_chart_values(&snap.active_history, mode);
    let compare_values = extract_chart_values(&snap.compare_history, mode);

    if values.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — collecting... "),
            "Waiting for data...",
        );
        return;
    }

    let to_points = |vals: &[f64]| -> Vec<(f64, f64)> {
        vals.iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v))
            .collect()
    };
    let data = to_points(&values);
    let compare_data = to_points(&compare_values);

    let cmp_name = app.compare_name().unwrap_or("?");
    let latest = *values.last().unwrap_or(&0.0);

    // Compute bounds across both traces
    let all_vals = values.iter().chain(compare_values.iter()).copied();
    let min_val = all_vals.clone().fold(f64::MAX, f64::min);
    let max_val = all_vals.fold(f64::MIN, f64::max);

    let mut datasets = vec![
        Dataset::default()
            .name(format!("{name} {latest:.2}"))
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ];

    if !compare_data.is_empty() {
        let cmp_latest = *compare_values.last().unwrap_or(&0.0);
        datasets.push(
            Dataset::default()
                .name(format!("{cmp_name} {cmp_latest:.2}"))
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Magenta))
                .data(&compare_data),
        );
    }

    let x_max = (data.len().max(compare_data.len()) as f64).max(10.0);
    let y_label = mode.y_label();
    let (y_min, y_max) = mode.y_bounds(min_val, max_val);

    let compare_hint = if app.compare_source().is_some() {
        format!(" vs {cmp_name}")
    } else {
        String::new()
    };
    let title = format!(
        " {name}{compare_hint}  [g] {}  {latest:.2} {y_label} ",
        mode.label()
    );

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(Line::from(format!(" {} ", mode.description())).dark_gray()),
        )
        .x_axis(
            Axis::default()
                .title("sample".dark_gray())
                .bounds([0.0, x_max])
                .labels(vec![Line::from("0"), Line::from(format!("{}", data.len()))]),
        )
        .y_axis(
            Axis::default()
                .title(y_label.dark_gray())
                .bounds([y_min, y_max])
                .labels(vec![
                    Line::from(format!("{y_min:.1}")),
                    Line::from(format!("{y_max:.1}")),
                ]),
        );

    f.render_widget(chart, area);
}

// ---------------------------------------------------------------------------
// Byte distribution (sparkline)
// ---------------------------------------------------------------------------

fn draw_byte_dist(f: &mut Frame, area: Rect, snap: &Snapshot, name: &str) {
    let freq = snap.byte_freq;
    let total: u64 = freq.iter().sum();

    if total == 0 {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Byte dist — collecting... "),
            "Accumulating byte frequencies...",
        );
        return;
    }

    // Bin 256 values into groups that fit the available width
    let inner_w = area.width.saturating_sub(2) as usize;
    let bin_size = (256 + inner_w - 1) / inner_w.max(1);
    let n_bins = 256_usize.div_ceil(bin_size);

    let mut bins: Vec<u64> = Vec::with_capacity(n_bins);
    for chunk in freq.chunks(bin_size) {
        bins.push(chunk.iter().sum());
    }

    let max_bin = *bins.iter().max().unwrap_or(&1);
    let expected = total as f64 / n_bins as f64;
    let chi_sq: f64 = bins
        .iter()
        .map(|&b| {
            let diff = b as f64 - expected;
            diff * diff / expected
        })
        .sum();

    let title = format!(" {name}  [g] Byte dist  n={total}  chi2={chi_sq:.1}  max={max_bin} ",);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(
            Line::from(format!(" {} ", ChartMode::ByteDistribution.description())).dark_gray(),
        );

    let sparkline = Sparkline::default()
        .block(block)
        .data(&bins)
        .max(max_bin)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(sparkline, area);
}

// ---------------------------------------------------------------------------
// Output panel
// ---------------------------------------------------------------------------

fn draw_output(f: &mut Frame, area: Rect, app: &App, snap: &Snapshot) {
    let mode = app.conditioning_mode();

    let (mode_label, mode_color) = match mode {
        ConditioningMode::Sha256 => ("SHA-256", Color::Green),
        ConditioningMode::VonNeumann => ("VonNeumann", Color::Yellow),
        ConditioningMode::Raw => ("Raw", Color::Red),
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("raw   ", Style::default().fg(Color::DarkGray)),
            Span::styled(&snap.raw_hex, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{mode_label:<6}"),
                Style::default().bold().fg(mode_color),
            ),
            Span::styled(&snap.rng_hex, Style::default().fg(Color::Yellow)),
        ]),
    ];

    let sz = app.sample_size();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Live Output  [c] {mode_label}  [n] {sz}B "));
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Key help bar
// ---------------------------------------------------------------------------

fn draw_keys(f: &mut Frame, area: Rect) {
    let bar = Paragraph::new(" ↑↓ nav  space: select  r: record  g: graph  c: cond  n: size  Tab: compare  p: pause  s: export  +/-: speed  q: quit")
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(bar, area);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_cat_maps_all_known_categories() {
        assert_eq!(short_cat("thermal"), "THM");
        assert_eq!(short_cat("timing"), "TMG");
        assert_eq!(short_cat("scheduling"), "SCH");
        assert_eq!(short_cat("io"), "I/O");
        assert_eq!(short_cat("ipc"), "IPC");
        assert_eq!(short_cat("microarch"), "uAR");
        assert_eq!(short_cat("gpu"), "GPU");
        assert_eq!(short_cat("network"), "NET");
        assert_eq!(short_cat("system"), "SYS");
        assert_eq!(short_cat("composite"), "CMP");
        assert_eq!(short_cat("signal"), "SIG");
        assert_eq!(short_cat("sensor"), "SNS");
    }

    #[test]
    fn short_cat_unknown_returns_question_mark() {
        assert_eq!(short_cat(""), "?");
        assert_eq!(short_cat("Timing"), "?");
        assert_eq!(short_cat("something_else"), "?");
    }

    #[test]
    fn display_cat_maps_all_known_categories() {
        assert_eq!(display_cat("thermal"), "Thermal");
        assert_eq!(display_cat("timing"), "Timing");
        assert_eq!(display_cat("scheduling"), "Scheduling");
        assert_eq!(display_cat("io"), "I/O");
        assert_eq!(display_cat("ipc"), "IPC");
        assert_eq!(display_cat("microarch"), "Microarch");
        assert_eq!(display_cat("gpu"), "GPU");
        assert_eq!(display_cat("network"), "Network");
        assert_eq!(display_cat("system"), "System");
        assert_eq!(display_cat("composite"), "Composite");
        assert_eq!(display_cat("signal"), "Signal");
        assert_eq!(display_cat("sensor"), "Sensor");
    }

    #[test]
    fn display_cat_unknown_passes_through() {
        assert_eq!(display_cat("something"), "something");
    }

    #[test]
    fn categories_table_consistent() {
        for (key, short, display) in CATEGORIES {
            assert_eq!(short_cat(key), *short);
            assert_eq!(display_cat(key), *display);
        }
    }

    #[test]
    fn format_time_sub_millisecond() {
        assert_eq!(format_time(0.0001), "0.1ms");
        assert_eq!(format_time(0.0005), "0.5ms");
    }

    #[test]
    fn format_time_milliseconds() {
        assert_eq!(format_time(0.015), "15.0ms");
        assert_eq!(format_time(0.1), "100.0ms");
        assert_eq!(format_time(0.999), "999.0ms");
    }

    #[test]
    fn format_time_seconds() {
        assert_eq!(format_time(1.0), "1.0s");
        assert_eq!(format_time(2.5), "2.5s");
        assert_eq!(format_time(10.0), "10.0s");
    }

    #[test]
    fn entropy_color_thresholds() {
        assert_eq!(entropy_color(7.5).fg, Some(Color::Green));
        assert_eq!(entropy_color(8.0).fg, Some(Color::Green));
        assert_eq!(entropy_color(5.0).fg, Some(Color::Yellow));
        assert_eq!(entropy_color(6.0).fg, Some(Color::Yellow));
        assert_eq!(entropy_color(4.9).fg, Some(Color::Red));
        assert_eq!(entropy_color(0.0).fg, Some(Color::Red));
    }

    #[test]
    fn extract_chart_values_shannon() {
        let history = vec![
            Sample {
                shannon: 7.0,
                min_entropy: 6.0,
                collect_time_ms: 1.0,
                output_value: 0.5,
            },
            Sample {
                shannon: 7.5,
                min_entropy: 6.5,
                collect_time_ms: 2.0,
                output_value: 0.6,
            },
        ];
        let vals = extract_chart_values(&history, ChartMode::Shannon);
        assert_eq!(vals, vec![7.0, 7.5]);
    }

    #[test]
    fn extract_chart_values_autocorrelation_too_short() {
        let history = vec![
            Sample {
                shannon: 7.0,
                min_entropy: 6.0,
                collect_time_ms: 1.0,
                output_value: 0.5,
            },
            Sample {
                shannon: 7.5,
                min_entropy: 6.5,
                collect_time_ms: 2.0,
                output_value: 0.6,
            },
        ];
        let vals = extract_chart_values(&history, ChartMode::Autocorrelation);
        assert!(vals.is_empty());
    }

    #[test]
    fn extract_chart_values_autocorrelation_with_data() {
        let history: Vec<Sample> = (0..10)
            .map(|i| Sample {
                shannon: 7.0,
                min_entropy: 6.0,
                collect_time_ms: 1.0,
                output_value: (i as f64) / 10.0,
            })
            .collect();
        let vals = extract_chart_values(&history, ChartMode::Autocorrelation);
        assert_eq!(vals.len(), 9);
    }
}
