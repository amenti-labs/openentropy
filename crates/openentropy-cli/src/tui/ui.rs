//! TUI rendering — single-source focus design.
//!
//! All draw functions receive an `&App` (non-shared fields) and `&Snapshot`
//! (shared state captured in a single mutex lock per frame).

use super::app::{App, ChartMode, QuantumFlowState, Sample, Snapshot, rolling_autocorr};
use openentropy_core::ConditioningMode;
use openentropy_core::sources::quantum::quantum_fraction;
use ratatui::{prelude::*, widgets::*};
use std::collections::VecDeque;

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

const LAVENDER: Color = Color::Rgb(200, 170, 255);

#[derive(Debug, Clone, Copy)]
struct QuantumSummary {
    level: &'static str,
    trust: &'static str,
}

fn quantum_summary(source_name: &str) -> Option<QuantumSummary> {
    let fraction = quantum_fraction(source_name);
    if fraction <= 0.0 {
        return None;
    }

    let (level, trust) = if source_name == "multi_source_quantum" {
        ("high-combined", "trusted if source independence holds")
    } else if fraction >= 0.95 {
        ("very high", "strong physics basis; good trust")
    } else if fraction >= 0.80 {
        ("high", "good trust with conditioning")
    } else if fraction >= 0.65 {
        ("moderate", "mixed quantum/classical; monitor quality")
    } else {
        ("low", "treat as mixed-noise input")
    };

    Some(QuantumSummary { level, trust })
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
    // Keep a fixed-width activity marker so the title doesn't shift.
    // While recording, keep it static to avoid visual jitter near the REC badge.
    let activity = if app.is_paused() {
        "⏸"
    } else if app.is_recording() {
        "●"
    } else if snap.collecting {
        "⟳"
    } else {
        "·"
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
        Span::styled(
            format!(" {activity} "),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    if app.is_recording() {
        let rec_elapsed = app
            .recording_elapsed()
            .map(|d| format!("{:.0}s", d.as_secs_f64()))
            .unwrap_or_default();
        title_spans.push(Span::styled(
            format!(" REC {} {}smp ", rec_elapsed, snap.recording_samples),
            Style::default().bold().fg(Color::White).bg(Color::Red),
        ));
        if let Some(path) = app.recording_path() {
            title_spans.push(Span::styled(
                format!(" {} ", path.display()),
                Style::default().fg(Color::Red),
            ));
        }
    }
    if let Some(err) = app.recording_error() {
        title_spans.push(Span::styled(
            format!(" REC ERROR: {} ", truncate_message(err, 72)),
            Style::default().fg(Color::White).bg(Color::Red),
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

fn truncate_message(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = s
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
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
            let q_summary = quantum_summary(name);
            let q_badge = if q_summary.is_some() { "Q" } else { " " };
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
                Cell::from(pointer.to_string()),
                Cell::from(marker.to_string()),
                Cell::from(q_badge.to_string()).style(if q_summary.is_some() {
                    let glow = match (snap.cycle_count % 3) as u8 {
                        0 => Color::Rgb(180, 140, 255),
                        1 => Color::Rgb(210, 180, 255),
                        _ => Color::Rgb(240, 220, 255),
                    };
                    let mut style = Style::default().bold().fg(glow);
                    if is_active {
                        style = style.add_modifier(Modifier::SLOW_BLINK);
                    }
                    style
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
                Cell::from(name.clone()),
                Cell::from(cat.to_string()),
                Cell::from(entropy_str),
                Cell::from(time_str),
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(vec!["", "", "Q", "Source", "Cat", "H", "Time"])
        .style(Style::default().bold().fg(Color::DarkGray))
        .bottom_margin(0);

    let table = Table::new(
        items,
        [
            Constraint::Length(2),  // pointer
            Constraint::Length(2),  // active marker
            Constraint::Length(2),  // quantum badge
            Constraint::Length(20), // name
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

        if let Some(q) = quantum_summary(&info.name) {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Q-origin ", Style::default().bold().fg(LAVENDER)),
                Span::styled(
                    q.level.to_string(),
                    Style::default().fg(LAVENDER),
                ),
                Span::styled("  trust: ", Style::default().fg(Color::DarkGray)),
                Span::styled(q.trust, Style::default().fg(LAVENDER)),
            ]));
            lines.push(Line::from(Span::styled(
                "Measurable via entropy/timing tests, but quantum certification needs Bell-test hardware.",
                Style::default().fg(LAVENDER),
            )));
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

    // Split area: chart on top, description below
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(4)])
        .split(area);
    let chart_area = parts[0];
    let desc_area = parts[1];

    if mode == ChartMode::ByteDistribution {
        draw_byte_dist(f, chart_area, snap, name);
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::RandomWalk {
        draw_random_walk(f, chart_area, snap, name);
        draw_description(f, desc_area, mode);
        return;
    }

    // Quantum visualizations - only applicable for specific sources
    if mode == ChartMode::SsdTunneling {
        if name == "ssd_tunneling" {
            draw_ssd_tunneling(f, chart_area, snap, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] SSD Tunneling "),
                "Select 'ssd_tunneling' source to view quantum visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::CosmicMuon {
        if name == "cosmic_muon" {
            draw_cosmic_muon(f, chart_area, snap, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Cosmic Muon "),
                "Select 'cosmic_muon' source to view quantum visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::CameraShotNoise {
        if name == "camera_noise" {
            draw_camera_shot_noise(f, chart_area, &snap.camera_noise, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Camera Shot Noise "),
                "Select 'camera_noise' source to view this visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::RadioactiveDecay {
        if name == "radioactive_decay" {
            draw_radioactive_decay(f, chart_area, &snap.radioactive_decay, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Radioactive Decay "),
                "Select 'radioactive_decay' source to view this visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::AvalancheNoise {
        if name == "avalanche_noise" {
            draw_avalanche_noise(f, chart_area, &snap.avalanche_noise, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Avalanche Noise "),
                "Select 'avalanche_noise' source to view this visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::VacuumFluctuations {
        if name == "vacuum_fluctuations" {
            draw_vacuum_fluctuations(f, chart_area, &snap.vacuum_fluctuations, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Vacuum Fluctuations "),
                "Select 'vacuum_fluctuations' source to view this visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if mode == ChartMode::MultiSourceQuantum {
        if name == "multi_source_quantum" {
            draw_multi_source_quantum(f, chart_area, &snap.multi_source_quantum, name);
        } else {
            draw_placeholder(
                f,
                chart_area,
                format!(" {name} — [g] Multi-Source XOR "),
                "Select 'multi_source_quantum' source to view this visualization",
            );
        }
        draw_description(f, desc_area, mode);
        return;
    }

    if snap.active_history.is_empty() {
        draw_placeholder(
            f,
            chart_area,
            format!(" {name} — select a source "),
            "Press space on a source to start watching",
        );
        draw_description(f, desc_area, mode);
        return;
    }

    let values = extract_chart_values(&snap.active_history, mode);
    let compare_values = extract_chart_values(&snap.compare_history, mode);

    if values.is_empty() {
        draw_placeholder(
            f,
            chart_area,
            format!(" {name} — collecting... "),
            "Waiting for data...",
        );
        draw_description(f, desc_area, mode);
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
                .title_bottom(Line::from(format!(" {} ", mode.summary())).dark_gray()),
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

    f.render_widget(chart, chart_area);
    draw_description(f, desc_area, mode);
}

// ---------------------------------------------------------------------------
// Chart description panel
// ---------------------------------------------------------------------------

fn draw_description(f: &mut Frame, area: Rect, mode: ChartMode) {
    let desc = mode.description();
    let lines: Vec<Line> = desc
        .iter()
        .map(|&s| Line::from(Span::styled(s, Style::default().fg(Color::DarkGray))))
        .collect();
    let p = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(p, area);
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
            Line::from(format!(" {} ", ChartMode::ByteDistribution.summary())).dark_gray(),
        );

    let sparkline = Sparkline::default()
        .block(block)
        .data(&bins)
        .max(max_bin)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(sparkline, area);
}

/// Draw a random walk (cumulative sum) that grows over time.
///
/// Each collection cycle appends new steps. The walk accumulates across
/// refreshes so you can watch it evolve like a live seismograph.
///
/// What the shape tells you:
/// - **Random data** → Brownian motion (wandering, no trend)
/// - **Biased data** → steady drift up or down
/// - **Correlated data** → smooth, sweeping curves
/// - **Stuck/broken** → flat line or extreme runaway
fn draw_random_walk(f: &mut Frame, area: Rect, snap: &Snapshot, name: &str) {
    if snap.walk.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Random walk — collecting... "),
            "Waiting for data...",
        );
        return;
    }

    let walk = &snap.walk;
    let n = walk.len();

    // Convert to chart points
    let data: Vec<(f64, f64)> = walk
        .iter()
        .enumerate()
        .map(|(i, &y)| (i as f64, y))
        .collect();

    // Stats
    let current = *walk.last().unwrap_or(&0.0);
    let min_y = walk.iter().copied().fold(f64::MAX, f64::min);
    let max_y = walk.iter().copied().fold(f64::MIN, f64::max);
    let title = format!(
        " {name}  [g] Random walk  {n} steps  now={current:+.0}  range=[{min_y:.0}, {max_y:.0}] "
    );

    // Y bounds: symmetric around 0, or track the actual range if it's drifted
    let y_center = (min_y + max_y) / 2.0;
    let y_range = (max_y - min_y).max(100.0) * 1.2;
    let y_lo = y_center - y_range / 2.0;
    let y_hi = y_center + y_range / 2.0;

    let dataset = Dataset::default()
        .name(format!("{current:+.0}"))
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .data(&data);

    // Zero line reference points (draw a flat line at y=0)
    let zero_line: Vec<(f64, f64)> = vec![(0.0, 0.0), (n as f64, 0.0)];
    let zero_dataset = Dataset::default()
        .name("0")
        .marker(symbols::Marker::Dot)
        .style(Style::default().fg(Color::DarkGray))
        .data(&zero_line);

    let chart = Chart::new(vec![dataset, zero_dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(
                    Line::from(format!(" {} ", ChartMode::RandomWalk.summary())).dark_gray(),
                ),
        )
        .x_axis(
            Axis::default()
                .title("steps".dark_gray())
                .bounds([0.0, (n as f64).max(10.0)])
                .labels(vec![Line::from("0"), Line::from(format!("{n}"))]),
        )
        .y_axis(Axis::default().bounds([y_lo, y_hi]).labels(vec![
            Line::from(format!("{y_lo:.0}")),
            Line::from(format!("{:.0}", (y_lo + y_hi) / 2.0)),
            Line::from(format!("{y_hi:.0}")),
        ]));

    f.render_widget(chart, area);
}

fn format_tail_bytes(bytes: &VecDeque<u8>, take: usize) -> String {
    let len = bytes.len();
    let start = len.saturating_sub(take);
    bytes
        .iter()
        .skip(start)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_tail_bits(bits: &VecDeque<u8>, take: usize) -> String {
    let len = bits.len();
    let start = len.saturating_sub(take);
    let raw: String = bits
        .iter()
        .skip(start)
        .map(|b| if *b == 0 { '0' } else { '1' })
        .collect();
    raw.chars()
        .enumerate()
        .flat_map(|(i, ch)| if i > 0 && i % 8 == 0 { vec![' ', ch] } else { vec![ch] })
        .collect()
}

fn spinner(frame: u64) -> char {
    match frame % 4 {
        0 => '|',
        1 => '/',
        2 => '-',
        _ => '\\',
    }
}

fn pulse_color(frame: u64) -> Color {
    match frame % 4 {
        0 => Color::Cyan,
        1 => Color::LightCyan,
        2 => Color::LightBlue,
        _ => Color::Blue,
    }
}

fn animated_cursor(line: &str, frame: u64, glyph: char) -> String {
    let mut chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let idx = (frame as usize) % chars.len();
    chars[idx] = glyph;
    chars.into_iter().collect()
}

fn compact_wave(samples: &[i32], width: usize) -> String {
    if samples.is_empty() || width == 0 {
        return String::new();
    }
    let min = *samples.iter().min().unwrap_or(&0);
    let max = *samples.iter().max().unwrap_or(&0);
    let span = (max - min).max(1) as f64;
    let step = samples.len().div_ceil(width).max(1);
    let glyphs = ['.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let mut out = String::new();
    for i in (0..samples.len()).step_by(step) {
        let v = samples[i];
        let norm = (v - min) as f64 / span;
        let gi = (norm * (glyphs.len() as f64 - 1.0)).round() as usize;
        out.push(glyphs[gi.min(glyphs.len() - 1)]);
    }
    out
}

fn draw_ssd_tunneling(f: &mut Frame, area: Rect, snap: &Snapshot, name: &str) {
    let state = &snap.ssd_tunneling;
    let latest_cycle = state.events.back().map(|e| e.cycle).unwrap_or(0);
    let mut lane_activity = [0u32; 8];
    let mut lane_recent_cycle: [Option<u64>; 8] = [None; 8];
    let mut recent_events = 0usize;
    let mut last_signal = 0u64;
    for e in state.events.iter().rev() {
        if latest_cycle.saturating_sub(e.cycle) <= 8 {
            recent_events += 1;
            lane_activity[e.col.min(7)] = lane_activity[e.col.min(7)].saturating_add(2);
            if lane_recent_cycle[e.col.min(7)].is_none() {
                lane_recent_cycle[e.col.min(7)] = Some(e.cycle);
            }
            if last_signal == 0 {
                last_signal = e.timing_delta;
            }
        } else {
            break;
        }
    }

    for &b in state.recent_bytes.iter().rev().take(24) {
        for bit in 0..8 {
            if ((b >> bit) & 1) == 1 {
                lane_activity[7 - bit as usize] += 1;
            }
        }
    }
    let max_lane = lane_activity.iter().copied().max().unwrap_or(1).max(1);

    let mut bits_now = ['0'; 8];
    for (idx, b) in state.recent_bits.iter().rev().take(8).rev().enumerate() {
        bits_now[idx] = if *b == 1 { '1' } else { '0' };
    }

    let mut field_row = ['L'; 8];
    let mut stress_row = ['1'; 8];
    let mut tunnel_row = ['.'; 8];
    let mut charge_row = ['.'; 8];
    let mut flow_pipe_row = [' '; 8];

    for lane in 0..8 {
        let activity = lane_activity[lane];
        let level = if activity.saturating_mul(3) >= max_lane.saturating_mul(2) {
            'H'
        } else if activity.saturating_mul(3) >= max_lane {
            'M'
        } else {
            'L'
        };
        field_row[lane] = level;
        stress_row[lane] = match level {
            'H' => '3',
            'M' => '2',
            _ => '1',
        };

        flow_pipe_row[lane] = if bits_now[lane] == '1' { '|' } else { ' ' };
        charge_row[lane] = match state.cell_states[lane] {
            0..=31 => '.',
            32..=63 => ':',
            64..=127 => '*',
            _ => '#',
        };

        if let Some(event_cycle) = lane_recent_cycle[lane] {
            let age = latest_cycle.saturating_sub(event_cycle);
            tunnel_row[lane] = if age == 0 {
                match (state.frame + lane as u64) % 4 {
                    0 => 'v',
                    1 => 'V',
                    2 => '|',
                    _ => ':',
                }
            } else if age <= 2 {
                ':'
            } else {
                '.'
            };
        }
    }

    let lane_to_string = |arr: &[char; 8]| -> String {
        arr.iter()
            .map(|c| format!("{c} "))
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    let text = vec![
        Line::from(vec![
            Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "NAND Fowler-Nordheim tunneling lanes".to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled("events", Style::default().fg(Color::DarkGray)),
            Span::raw(format!(" total={} recent={}", state.total_events, recent_events)),
        ]),
        Line::from(vec![
            Span::styled("Data map ", Style::default().fg(Color::DarkGray)),
            Span::raw("direct raw-bit -> cell event"),
            Span::raw("  "),
            Span::styled("last signal ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("0x{last_signal:04x}")),
        ]),
        Line::from(vec![
            Span::styled("stream ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.repeat_streak > 0 { "REPEATING" } else { "LIVE" },
                Style::default().fg(if state.repeat_streak > 0 { Color::Red } else { Color::Green }).bold(),
            ),
            Span::raw("  "),
            Span::styled("changed bits ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", state.changed_bits_last)),
            Span::raw("  "),
            Span::styled("repeat ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", state.repeat_streak)),
            Span::raw("  "),
            Span::styled("fp ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:08x}", (state.stream_fingerprint & 0xffff_ffff) as u32)),
        ]),
        Line::from(vec![
            Span::styled("bytes tail ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_tail_bytes(&state.recent_bytes, 12), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("bits tail  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                animated_cursor(&format_tail_bits(&state.recent_bits, 64), state.frame, '^'),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("PHYSICAL FLOW (top -> bottom): ", Style::default().fg(Color::DarkGray)),
            Span::raw("each column is one NAND cell lane"),
        ]),
        Line::from(format!("lane index              0 1 2 3 4 5 6 7")),
        Line::from(format!("1) raw bit (this cycle) {}", lane_to_string(&bits_now))),
        Line::from(format!("                        {}", lane_to_string(&flow_pipe_row))),
        Line::from(format!("2) electric field (E)   {}", lane_to_string(&field_row))),
        Line::from(format!("3) oxide stress         {}", lane_to_string(&stress_row))),
        Line::from(format!("4) tunnel crossing      {}", lane_to_string(&tunnel_row))),
        Line::from(format!("5) trapped charge       {}", lane_to_string(&charge_row))),
        Line::from(""),
        Line::from(Span::styled("HOW TO READ THIS", Style::default().fg(Color::DarkGray))),
        Line::from("H/M/L field: High / Medium / Low electric field in that lane."),
        Line::from("Stress 3/2/1: Effective oxide stress (3 highest tunneling chance)."),
        Line::from("Tunnel v/V/|: Electron crossing now (animated); : recent; . none."),
        Line::from("Charge #/*/:/. : Floating-gate stored charge from high -> low."),
        Line::from("Data mapping: bit=1 raises lane drive; repeated activity builds charge and events."),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] SSD Tunneling "));
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_cosmic_muon(f: &mut Frame, area: Rect, snap: &Snapshot, name: &str) {
    let state = &snap.cosmic_muon;
    let latest_cycle = state.hits.back().map(|h| h.cycle).unwrap_or(0);
    let recent_hits = state
        .hits
        .iter()
        .filter(|h| latest_cycle.saturating_sub(h.cycle) <= 6)
        .count();
    let mut overlay = state.grid;
    for hit in state
        .hits
        .iter()
        .filter(|h| latest_cycle.saturating_sub(h.cycle) <= 6)
    {
        let x = hit.x.min(31);
        let y = hit.y.min(23);
        overlay[y][x] = overlay[y][x].max(hit.intensity);
    }

    let inner_w = area.width.saturating_sub(4).max(8) as usize;
    let inner_h = area.height.saturating_sub(7).max(4) as usize;
    let step_x = 32_usize.div_ceil(inner_w).max(1);
    let step_y = 24_usize.div_ceil(inner_h).max(1);
    let scan_y = (state.frame as usize) % 24;

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Sensor: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("camera dark-frame ionizing hits {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        Line::from(vec![
            Span::styled("hits ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("total={} recent={}", state.total_hits, recent_hits)),
            Span::raw("  "),
            Span::styled("map ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("bit-index -> pixel  scan_y={scan_y}")),
        ]),
        Line::from(vec![
            Span::styled("stream ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.repeat_streak > 0 { "REPEATING" } else { "LIVE" },
                Style::default().fg(if state.repeat_streak > 0 { Color::Red } else { Color::Green }).bold(),
            ),
            Span::raw("  "),
            Span::styled("changed bits ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", state.changed_bits_last)),
            Span::raw("  "),
            Span::styled("repeat ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", state.repeat_streak)),
            Span::raw("  "),
            Span::styled("fp ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:08x}", (state.stream_fingerprint & 0xffff_ffff) as u32)),
        ]),
        Line::from(vec![
            Span::styled("bytes tail ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_tail_bytes(&state.recent_bytes, 12), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("bits tail  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                animated_cursor(&format_tail_bits(&state.recent_bits, 64), state.frame, '*'),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(Span::styled(
            "Legend: . : * # @ intensity from set bits in current stream",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    for y in (0..24).step_by(step_y) {
        let mut row = String::new();
        for x in (0..32).step_by(step_x) {
            let intensity = overlay[y][x];
            let ch = match intensity {
                0..=20 => ' ',
                21..=50 => '.',
                51..=90 => ':',
                91..=140 => '*',
                141..=200 => '#',
                _ => '@',
            };
            if scan_y >= y && scan_y < y + step_y && ch == ' ' {
                row.push('.');
            } else {
                row.push(ch);
            }
        }
        lines.push(Line::from(row));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Cosmic Muon "));
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn flow_status_line(state: &QuantumFlowState) -> Line<'static> {
    let live = state.repeat_streak == 0;
    Line::from(vec![
        Span::styled("stream ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if live { "LIVE" } else { "REPEATING" },
            Style::default()
                .fg(if live { Color::Green } else { Color::Red })
                .bold(),
        ),
        Span::raw("  "),
        Span::styled("changed ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}", state.changed_bits_last)),
        Span::raw("  "),
        Span::styled("repeat ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}", state.repeat_streak)),
        Span::raw("  "),
        Span::styled("fp ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{:08x}", (state.stream_fingerprint & 0xffff_ffff) as u32)),
    ])
}

fn draw_camera_shot_noise(f: &mut Frame, area: Rect, state: &QuantumFlowState, name: &str) {
    if state.recent_bytes.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Camera Shot Noise "),
            "Waiting for stream data...",
        );
        return;
    }

    let mut nibbles = Vec::with_capacity(state.recent_bytes.len() * 2);
    for &b in &state.recent_bytes {
        nibbles.push((b >> 4) & 0x0f);
        nibbles.push(b & 0x0f);
    }

    let cols = area.width.saturating_sub(6).clamp(20, 64) as usize;
    let rows = area.height.saturating_sub(12).clamp(6, 18) as usize;

    // Use a rolling window over recent nibble stream so the panel is always full
    // and movement reflects actual incoming data phase.
    let window_len = nibbles.len().max(1);
    let phase = (state.frame as usize) % window_len;
    let start = nibbles.len().saturating_sub(window_len);

    let nibble_to_glyph = |n: u8| -> char {
        match n {
            0 => ' ',
            1 => '.',
            2..=3 => ':',
            4..=5 => '-',
            6..=7 => '=',
            8..=9 => '+',
            10..=11 => '*',
            12..=13 => '#',
            14 => '%',
            _ => '@',
        }
    };

    let mut bins = [0usize; 16];
    for &n in &nibbles {
        bins[n as usize] += 1;
    }
    let max_bin = bins.iter().copied().max().unwrap_or(1).max(1);
    let hist_half = |lo: usize, hi: usize| -> String {
        (lo..=hi)
            .map(|i| {
                let h = ((bins[i] as f64 / max_bin as f64) * 6.0).round() as usize;
                match h {
                    0..=1 => '.',
                    2..=3 => ':',
                    4..=5 => '*',
                    _ => '#',
                }
            })
            .collect()
    };

    let nib_tail: String = nibbles
        .iter()
        .rev()
        .take(24)
        .rev()
        .map(|n| format!("{n:x}"))
        .collect::<Vec<_>>()
        .join(" ");

    let mut lines = vec![
        Line::from(vec![
            Span::styled("model ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("rolling sensor-grain map from camera LSB nibbles {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        flow_status_line(state),
        Line::from(vec![
            Span::styled("map ", Style::default().fg(Color::DarkGray)),
            Span::raw("pixel nibble 0..f -> "),
            Span::styled(" .:-=+*#%@", Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("phase ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{phase}")),
        ]),
        Line::from(vec![
            Span::styled("hist 0-7 ", Style::default().fg(Color::DarkGray)),
            Span::styled(hist_half(0, 7), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("8-f ", Style::default().fg(Color::DarkGray)),
            Span::styled(hist_half(8, 15), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("nibbles ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&nib_tail, state.frame, 'X'), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
    ];

    for r in 0..rows {
        let mut row = String::with_capacity(cols);
        for c in 0..cols {
            let idx = (phase + (r * cols) + c) % window_len;
            let n = nibbles.get(start + idx).copied().unwrap_or(0);
            let mut ch = nibble_to_glyph(n);
            if c == (state.frame as usize + r) % cols && ch == ' ' {
                ch = '.';
            }
            row.push(ch);
        }
        lines.push(Line::from(row));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Camera Shot Noise "));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}

fn draw_radioactive_decay(f: &mut Frame, area: Rect, state: &QuantumFlowState, name: &str) {
    if state.recent_bytes.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Radioactive Decay "),
            "Waiting for stream data...",
        );
        return;
    }
    let raw_bits: String = state
        .recent_bits
        .iter()
        .rev()
        .take(128)
        .rev()
        .map(|b| if *b == 1 { '1' } else { '0' })
        .collect();
    let pulse_train: String = raw_bits
        .chars()
        .map(|c| if c == '1' { '|' } else { '.' })
        .collect();
    let click_density = raw_bits.chars().filter(|c| *c == '1').count();

    let mut intervals = Vec::new();
    let mut last = None::<usize>;
    for (i, c) in raw_bits.chars().enumerate() {
        if c == '1' {
            if let Some(prev) = last {
                intervals.push(i.saturating_sub(prev));
            }
            last = Some(i);
        }
    }
    let interval_trace: String = intervals
        .iter()
        .rev()
        .take(28)
        .rev()
        .map(|d| match *d {
            0..=1 => '@',
            2..=3 => '#',
            4..=6 => '*',
            7..=10 => ':',
            _ => '.',
        })
        .collect();

    let nucleus = if state.frame % 2 == 0 { "[*]" } else { "[ ]" };
    let decay_chain = format!(
        "{} -> beta -> {} -> gamma -> {}",
        nucleus,
        if click_density % 2 == 0 { "[*]" } else { "[ ]" },
        if click_density % 3 == 0 { "[*]" } else { "[ ]" }
    );

    let lines = vec![
        Line::from(vec![
            Span::styled("model ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("geiger-style click train + inter-arrival track {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        flow_status_line(state),
        Line::from(vec![
            Span::styled("decay chain ", Style::default().fg(Color::DarkGray)),
            Span::styled(decay_chain, Style::default().fg(Color::LightRed)),
        ]),
        Line::from(vec![
            Span::styled("clicks ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{click_density}/128")),
            Span::raw("  "),
            Span::styled("inter-arrival ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&interval_trace, state.frame, 'X'), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("pulse  ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&pulse_train, state.frame, '*'), Style::default().fg(pulse_color(state.frame))),
        ]),
        Line::from(vec![
            Span::styled("bytes tail ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_tail_bytes(&state.recent_bytes, 12), Style::default().fg(Color::Yellow)),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Radioactive Decay "));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
}

fn draw_avalanche_noise(f: &mut Frame, area: Rect, state: &QuantumFlowState, name: &str) {
    if state.recent_bytes.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Avalanche Noise "),
            "Waiting for stream data...",
        );
        return;
    }

    let burst_line: String = state
        .recent_bytes
        .iter()
        .rev()
        .take(32)
        .rev()
        .map(|b| match b.count_ones() {
            0..=1 => '.',
            2..=3 => ':',
            4..=5 => '*',
            6..=7 => '#',
            _ => '@',
        })
        .collect();

    let spark_pos = (state.frame as usize) % 24;
    let spark_tail = (spark_pos + 6) % 24;
    let arc: String = (0..24)
        .map(|i| {
            if i == spark_pos {
                'X'
            } else if i == spark_tail {
                '*'
            } else if i % 6 == 0 {
                '|'
            } else {
                '-'
            }
        })
        .collect();

    let mut field = [0usize; 6];
    for (i, b) in state.recent_bytes.iter().enumerate() {
        field[i % 6] += (b.count_ones() as usize) + ((b >> 6) as usize & 0x3);
    }
    let peak = field.iter().copied().max().unwrap_or(1).max(1);
    let field_bars: Vec<String> = field
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let width = ((*v as f64 / peak as f64) * 12.0).round().max(1.0) as usize;
            let base = "=".repeat(width);
            animated_cursor(&base, state.frame + i as u64, '!')
        })
        .collect();

    let lines = vec![
        Line::from(vec![
            Span::styled("model ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("pn-junction breakdown arc + burst field {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        flow_status_line(state),
        Line::from(vec![
            Span::styled("diode ", Style::default().fg(Color::DarkGray)),
            Span::styled("A --|<|-- K", Style::default().fg(Color::LightBlue)),
            Span::raw("  "),
            Span::styled("arc ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&arc, state.frame, '#'), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("burst ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&burst_line, state.frame, '@'), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(format!("field0 {}", field_bars[0])),
        Line::from(format!("field1 {}", field_bars[1])),
        Line::from(format!("field2 {}", field_bars[2])),
        Line::from(format!("field3 {}", field_bars[3])),
        Line::from(format!("field4 {}", field_bars[4])),
        Line::from(format!("field5 {}", field_bars[5])),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Avalanche Noise "));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
}

fn draw_vacuum_fluctuations(f: &mut Frame, area: Rect, state: &QuantumFlowState, name: &str) {
    if state.recent_bits.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Vacuum Fluctuations "),
            "Waiting for stream data...",
        );
        return;
    }

    let mut walk = Vec::new();
    let mut sum = 0i32;
    for bit in state.recent_bits.iter().rev().take(96).rev() {
        sum += if *bit == 1 { 1 } else { -1 };
        walk.push(sum);
    }
    let min = *walk.iter().min().unwrap_or(&0);
    let max = *walk.iter().max().unwrap_or(&0);
    let end = *walk.last().unwrap_or(&0);
    let steps = walk.len().max(1);
    let zero_cross = walk.windows(2).filter(|w| (w[0] <= 0 && w[1] > 0) || (w[0] >= 0 && w[1] < 0)).count();
    let zero_cross_rate = zero_cross as f64 / steps as f64;
    let drift_per_step = end as f64 / steps as f64;
    let ones = state.recent_bits.iter().filter(|b| **b == 1).count();
    let ones_ratio = ones as f64 / state.recent_bits.len().max(1) as f64;
    let trend: String = walk
        .windows(2)
        .map(|w| {
            if w[1] > w[0] {
                '^'
            } else if w[1] < w[0] {
                'v'
            } else {
                '-'
            }
        })
        .collect();
    let wave = compact_wave(&walk, area.width.saturating_sub(10) as usize);
    let foam_a: String = trend
        .chars()
        .enumerate()
        .map(|(i, c)| match c {
            '^' if i % 2 == 0 => '/',
            '^' => '^',
            'v' if i % 2 == 0 => '\\',
            'v' => 'v',
            _ => '.',
        })
        .collect();
    let foam_b: String = foam_a
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 7 == (state.frame as usize % 7) {
                'o'
            } else {
                c
            }
        })
        .collect();

    let lines = vec![
        Line::from(vec![
            Span::styled("model ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("virtual-pair proxy from raw bitflow {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        flow_status_line(state),
        Line::from(vec![
            Span::styled("map  ", Style::default().fg(Color::DarkGray)),
            Span::raw("bit1 => +1 step, bit0 => -1 step; walk near 0 means low bias"),
        ]),
        Line::from(vec![
            Span::styled("walk ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("end={end} min={min} max={max} zero-cross={zero_cross}")),
        ]),
        Line::from(vec![
            Span::styled("entropy signal ", Style::default().fg(Color::DarkGray)),
            Span::raw("higher zero-cross + bounded walk drift => better unpredictability"),
        ]),
        Line::from(vec![
            Span::styled("health ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!(
                "ones={:.2}  zcr={:.2}  drift/step={:.2}",
                ones_ratio, zero_cross_rate, drift_per_step
            )),
            Span::raw("  "),
            Span::styled(
                if (0.40..=0.60).contains(&ones_ratio) && drift_per_step.abs() < 0.25 {
                    "balanced"
                } else {
                    "biased/memory"
                },
                Style::default().fg(if (0.40..=0.60).contains(&ones_ratio) && drift_per_step.abs() < 0.25 {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("key ", Style::default().fg(Color::DarkGray)),
            Span::raw("trend: '^' up(+1)  'v' down(-1)  '-' flat"),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default().fg(Color::DarkGray)),
            Span::raw("foamA/B: phase textures from trend; 'o' marks moving phase tap"),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default().fg(Color::DarkGray)),
            Span::raw("wave: compressed walk amplitude ("),
            Span::styled(".:-=+*#%@", Style::default().fg(Color::Yellow)),
            Span::raw(" = low->high)"),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default().fg(Color::DarkGray)),
            Span::raw("bits: raw 64-bit tail driving all rows"),
        ]),
        Line::from(vec![
            Span::styled("trend ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&trend, state.frame, 'o'), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("foamA ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&foam_a, state.frame, '*'), Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("foamB ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&foam_b, state.frame + 3, '@'), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("wave  ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&wave, state.frame, '*'), Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("bits  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_tail_bits(&state.recent_bits, 64), Style::default().fg(Color::Yellow)),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Vacuum Fluctuations "));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
}

fn draw_multi_source_quantum(f: &mut Frame, area: Rect, state: &QuantumFlowState, name: &str) {
    if state.recent_bytes.is_empty() {
        draw_placeholder(
            f,
            area,
            format!(" {name} — [g] Multi-Source XOR "),
            "Waiting for stream data...",
        );
        return;
    }

    let tail: Vec<u8> = state.recent_bytes.iter().rev().take(24).copied().collect();
    let mut lane_a = String::with_capacity(tail.len());
    let mut lane_b = String::with_capacity(tail.len());
    let mut lane_c = String::with_capacity(tail.len());
    let mut lane_d = String::with_capacity(tail.len());
    let mut out = String::with_capacity(tail.len());

    let mut hi_xor = 0u8;
    let mut lo_xor = 0u8;
    for &b in tail.iter().rev() {
        hi_xor ^= b >> 4;
        lo_xor ^= b & 0x0f;
        lane_a.push(if b & 0b0000_0001 == 0 { '.' } else { 'a' });
        lane_b.push(if b & 0b0000_0100 == 0 { '.' } else { 'b' });
        lane_c.push(if b & 0b0001_0000 == 0 { '.' } else { 'c' });
        lane_d.push(if b & 0b0100_0000 == 0 { '.' } else { 'd' });
        out.push(if b.count_ones() % 2 == 0 { '0' } else { '1' });
    }
    let lane_a = animated_cursor(&lane_a, state.frame, 'A');
    let lane_b = animated_cursor(&lane_b, state.frame + 2, 'B');
    let lane_c = animated_cursor(&lane_c, state.frame + 4, 'C');
    let lane_d = animated_cursor(&lane_d, state.frame + 6, 'D');
    let parity_anim = animated_cursor(&out, state.frame, 'X');
    let lane_mix: String = (0..8)
        .map(|bit| {
            let ones = state
                .recent_bytes
                .iter()
                .filter(|b| ((*b >> bit) & 1) == 1)
                .count();
            match ones {
                0..=2 => '.',
                3..=5 => ':',
                6..=8 => '*',
                9..=12 => '#',
                _ => '@',
            }
        })
        .collect();

    let lines = vec![
        Line::from(vec![
            Span::styled("model ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("lane braid -> xor parity combiner {}", spinner(state.frame)),
                Style::default().fg(pulse_color(state.frame)),
            ),
        ]),
        flow_status_line(state),
        Line::from(vec![
            Span::styled("xor hi/lo ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{hi_xor:01x}/{lo_xor:01x}")),
            Span::raw("  "),
            Span::styled("lane mix ", Style::default().fg(Color::DarkGray)),
            Span::styled(animated_cursor(&lane_mix, state.frame, '#'), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("lane a ", Style::default().fg(Color::DarkGray)),
            Span::styled(lane_a, Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("lane b ", Style::default().fg(Color::DarkGray)),
            Span::styled(lane_b, Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("lane c ", Style::default().fg(Color::DarkGray)),
            Span::styled(lane_c, Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("lane d ", Style::default().fg(Color::DarkGray)),
            Span::styled(lane_d, Style::default().fg(Color::LightBlue)),
        ]),
        Line::from(vec![
            Span::styled("xor out ", Style::default().fg(Color::DarkGray)),
            Span::styled(parity_anim, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("bytes  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_tail_bytes(&state.recent_bytes, 12), Style::default().fg(Color::Yellow)),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {name}  [g] Multi-Source XOR "));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
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

    #[test]
    fn quantum_summary_marks_quantum_sources() {
        let q = quantum_summary("ssd_tunneling");
        assert!(q.is_some());
        let q = q.unwrap();
        assert!(!q.level.is_empty());
        assert!(!q.trust.is_empty());
    }

    #[test]
    fn quantum_summary_skips_non_quantum_sources() {
        assert!(quantum_summary("clock_jitter").is_none());
    }
}
