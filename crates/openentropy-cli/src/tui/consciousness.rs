//! Consciousness experiment TUI dashboard.
//!
//! Live visualization of consciousness-RNG experiments with Z-score traces,
//! cumulative effects, per-source heatmaps, and PEAR Lab-style feedback.

use std::collections::HashMap;
use std::io;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use ratatui::widgets::*;

use openentropy_core::consciousness::{
    ExperimentMode, IntentionDirection, format_p_value, format_z, z_to_p_two_tailed,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// What the consciousness TUI displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsciousnessView {
    /// Main overview: Z-score trace + feedback bar + source list.
    Overview,
    /// Per-source heatmap of Z-scores across trials.
    SourceHeatmap,
    /// Cumulative Z trend across all trials.
    CumulativeTrend,
    /// Phase comparison bars.
    PhaseComparison,
    /// Forest plot of per-source effect sizes with confidence intervals.
    ForestPlot,
}

/// A single data point captured during a consciousness trial.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TrialSnapshot {
    pub trial_index: usize,
    pub direction: IntentionDirection,
    pub pooled_z: f64,
    pub cumulative_z: f64,
    pub p_value: f64,
    pub source_z_scores: HashMap<String, f64>,
    pub ones_count: u32,
    pub timestamp_secs: f64,
}

/// Shared state updated by the experiment thread, read by the TUI.
pub struct ConsciousnessSharedState {
    pub trials: Vec<TrialSnapshot>,
    pub current_phase: IntentionDirection,
    pub phase_index: usize,
    pub total_phases: usize,
    pub trial_in_phase: usize,
    pub trials_per_phase: usize,
    pub mode: ExperimentMode,
    pub source_names: Vec<String>,
    pub experiment_complete: bool,
    pub experiment_start: Instant,
    /// Phase-level cumulative Z-scores: (direction, cumulative Z, p-value).
    pub phase_cumulative_z: Vec<(IntentionDirection, f64, f64)>,
}

impl ConsciousnessSharedState {
    pub fn new(
        mode: ExperimentMode,
        source_names: Vec<String>,
        trials_per_phase: usize,
        total_phases: usize,
    ) -> Self {
        Self {
            trials: Vec::new(),
            current_phase: IntentionDirection::Baseline,
            phase_index: 0,
            total_phases,
            trial_in_phase: 0,
            trials_per_phase,
            mode,
            source_names,
            experiment_complete: false,
            experiment_start: Instant::now(),
            phase_cumulative_z: Vec::new(),
        }
    }
}

/// Main consciousness TUI app.
pub struct ConsciousnessApp {
    pub view: ConsciousnessView,
    pub running: Arc<AtomicBool>,
    pub state: Arc<Mutex<ConsciousnessSharedState>>,
}

impl ConsciousnessApp {
    pub fn new(state: Arc<Mutex<ConsciousnessSharedState>>) -> Self {
        Self {
            view: ConsciousnessView::Overview,
            running: Arc::new(AtomicBool::new(true)),
            state,
        }
    }

    /// Run the TUI event loop. Call from the main thread.
    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick_rate = Duration::from_millis(100);

        while self.running.load(Ordering::SeqCst) {
            terminal.draw(|f| draw_consciousness_ui(f, self))?;

            if event::poll(tick_rate)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.running.store(false, Ordering::SeqCst);
                            }
                            KeyCode::Tab => {
                                self.view = match self.view {
                                    ConsciousnessView::Overview => {
                                        ConsciousnessView::SourceHeatmap
                                    }
                                    ConsciousnessView::SourceHeatmap => {
                                        ConsciousnessView::CumulativeTrend
                                    }
                                    ConsciousnessView::CumulativeTrend => {
                                        ConsciousnessView::PhaseComparison
                                    }
                                    ConsciousnessView::PhaseComparison => {
                                        ConsciousnessView::ForestPlot
                                    }
                                    ConsciousnessView::ForestPlot => {
                                        ConsciousnessView::Overview
                                    }
                                };
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Check if experiment is complete.
            let complete = {
                let state = self.state.lock().unwrap();
                state.experiment_complete
            };
            if complete {
                // Show final results for 3 more seconds.
                std::thread::sleep(Duration::from_secs(3));
                break;
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw_consciousness_ui(f: &mut Frame, app: &ConsciousnessApp) {
    let state = app.state.lock().unwrap();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Length(5), // Feedback bar + phase info
            Constraint::Min(10),  // Main content area
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    draw_title_bar(f, chunks[0], &state);
    draw_feedback_bar(f, chunks[1], &state);

    match app.view {
        ConsciousnessView::Overview => draw_overview(f, chunks[2], &state),
        ConsciousnessView::SourceHeatmap => draw_source_heatmap(f, chunks[2], &state),
        ConsciousnessView::CumulativeTrend => draw_cumulative_trend(f, chunks[2], &state),
        ConsciousnessView::PhaseComparison => draw_phase_comparison(f, chunks[2], &state),
        ConsciousnessView::ForestPlot => draw_forest_plot(f, chunks[2], &state),
    }

    draw_status_bar(f, chunks[3], app, &state);
}

fn draw_title_bar(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let elapsed = state.experiment_start.elapsed().as_secs();
    let title = format!(
        " CONSCIOUSNESS-RNG EXPERIMENT  |  Mode: {}  |  Phase {}/{}  |  {}s elapsed ",
        state.mode,
        state.phase_index + 1,
        state.total_phases,
        elapsed
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(block, area);
}

fn draw_feedback_bar(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(3)])
        .split(area);

    // Phase instruction line.
    let instruction = match state.current_phase {
        IntentionDirection::Baseline => Span::styled(
            "  BASELINE -- Relax. No intention. Just observe.",
            Style::default().fg(Color::Gray),
        ),
        IntentionDirection::High => Span::styled(
            "  HIGH -- Focus: INCREASE 1-bits. Visualize numbers going UP.",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        IntentionDirection::Low => Span::styled(
            "  LOW -- Focus: DECREASE 1-bits. Visualize numbers going DOWN.",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    };
    f.render_widget(Paragraph::new(Line::from(instruction)), inner[0]);

    // PEAR Lab-style feedback bar: visual bar showing current Z-score direction.
    let latest_z = state.trials.last().map(|t| t.pooled_z).unwrap_or(0.0);
    let latest_cum_z = state.trials.last().map(|t| t.cumulative_z).unwrap_or(0.0);

    let bar_width = inner[1].width.saturating_sub(30) as usize;
    if bar_width < 10 {
        return;
    }
    let center = bar_width / 2;

    // Map Z to bar position: -3 sigma to +3 sigma range.
    let pos = ((latest_z / 3.0) * center as f64) as i32 + center as i32;
    let pos = pos.clamp(0, bar_width as i32 - 1) as usize;

    let mut bar_chars: Vec<Span> =
        vec![Span::styled("─", Style::default().fg(Color::DarkGray)); bar_width];
    bar_chars[center] = Span::styled("│", Style::default().fg(Color::White));

    // Fill from center to current position.
    if pos > center {
        for i in center..=pos {
            bar_chars[i] = Span::styled("█", Style::default().fg(Color::Green));
        }
    } else if pos < center {
        for i in pos..=center {
            bar_chars[i] = Span::styled("█", Style::default().fg(Color::Red));
        }
    }
    bar_chars[pos] = Span::styled(
        "◆",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let p_str = format_p_value(z_to_p_two_tailed(latest_cum_z));
    let stats_span = Span::styled(
        format!(
            "  Z:{:>7} cum:{:>7} p:{}",
            format_z(latest_z),
            format_z(latest_cum_z),
            p_str
        ),
        Style::default().fg(Color::Cyan),
    );

    let mut spans: Vec<Span> = vec![Span::raw("  ")];
    spans.extend(bar_chars);
    spans.push(stats_span);

    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::TOP)),
        inner[1],
    );
}

fn draw_overview(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Left: Z-score trace chart.
    draw_z_trace(f, chunks[0], state);

    // Right: Source list with Z-scores.
    draw_source_list(f, chunks[1], state);
}

fn draw_z_trace(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    if state.trials.is_empty() {
        let block = Block::default()
            .title(" Z-Score Trace ")
            .borders(Borders::ALL);
        f.render_widget(
            Paragraph::new("  Waiting for trials...").block(block),
            area,
        );
        return;
    }

    let data: Vec<(f64, f64)> = state
        .trials
        .iter()
        .enumerate()
        .map(|(i, t)| (i as f64, t.pooled_z))
        .collect();

    let cumulative_data: Vec<(f64, f64)> = state
        .trials
        .iter()
        .enumerate()
        .map(|(i, t)| (i as f64, t.cumulative_z))
        .collect();

    let min_y = data
        .iter()
        .chain(cumulative_data.iter())
        .map(|(_, y)| *y)
        .fold(f64::MAX, f64::min)
        .min(-2.0);
    let max_y = data
        .iter()
        .chain(cumulative_data.iter())
        .map(|(_, y)| *y)
        .fold(f64::MIN, f64::max)
        .max(2.0);

    let x_max = (state.trials.len() as f64).max(10.0);

    let datasets = vec![
        Dataset::default()
            .name("Z (trial)")
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().fg(Color::Yellow))
            .data(&data),
        Dataset::default()
            .name("Cumulative Z")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&cumulative_data),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(" Z-Score Trace ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .x_axis(
            Axis::default()
                .title("Trial")
                .bounds([0.0, x_max])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{}", x_max as usize)),
                ]),
        )
        .y_axis(
            Axis::default()
                .title("Z")
                .bounds([min_y, max_y])
                .labels(vec![
                    Span::raw(format!("{:.1}", min_y)),
                    Span::styled("0.0", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1}", max_y)),
                ]),
        );

    f.render_widget(chart, area);
}

fn draw_source_list(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let block = Block::default()
        .title(" Sources ")
        .borders(Borders::ALL);

    if state.trials.is_empty() || state.source_names.is_empty() {
        f.render_widget(Paragraph::new("  Waiting...").block(block), area);
        return;
    }

    // Get latest Z-scores per source.
    let latest = &state.trials[state.trials.len() - 1];
    let mut source_z: Vec<(&str, f64)> = state
        .source_names
        .iter()
        .map(|name| {
            let z = latest
                .source_z_scores
                .get(name.as_str())
                .copied()
                .unwrap_or(0.0);
            (name.as_str(), z)
        })
        .collect();
    source_z.sort_by(|a, b| {
        b.1.abs()
            .partial_cmp(&a.1.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let rows: Vec<Row> = source_z
        .iter()
        .take(area.height.saturating_sub(3) as usize)
        .map(|(name, z)| {
            let z_color = if *z > 1.0 {
                Color::Green
            } else if *z < -1.0 {
                Color::Red
            } else {
                Color::Gray
            };
            let ctrl = if *name == "prng_control" { " [C]" } else { "" };
            let truncated = if name.len() > 16 {
                &name[..16]
            } else {
                name
            };
            Row::new(vec![
                Cell::from(format!("{truncated}{ctrl}")),
                Cell::from(format!("{:>7}", format_z(*z)))
                    .style(Style::default().fg(z_color)),
            ])
        })
        .collect();

    let table = Table::new(rows, [Constraint::Min(18), Constraint::Length(8)])
        .block(block)
        .header(
            Row::new(vec!["Source", "Z"]).style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(table, area);
}

fn draw_source_heatmap(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let block = Block::default()
        .title(" Source Z-Score Heatmap (rows=sources, cols=trials) ")
        .borders(Borders::ALL);

    if state.trials.is_empty() {
        f.render_widget(
            Paragraph::new("  Waiting for data...").block(block),
            area,
        );
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_sources = inner.height.saturating_sub(1) as usize;
    let max_trials = inner.width.saturating_sub(20) as usize;

    let sources: Vec<&str> = state
        .source_names
        .iter()
        .take(max_sources)
        .map(|s| s.as_str())
        .collect();

    let trial_start = state.trials.len().saturating_sub(max_trials);
    let visible_trials = &state.trials[trial_start..];

    let mut lines: Vec<Line> = Vec::new();
    for src_name in &sources {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!(
                "{:>18} ",
                if src_name.len() > 18 {
                    &src_name[..18]
                } else {
                    src_name
                }
            ),
            Style::default().fg(Color::White),
        )];

        for trial in visible_trials {
            let z = trial
                .source_z_scores
                .get(*src_name)
                .copied()
                .unwrap_or(0.0);
            let (ch, color) = z_to_heatmap_cell(z);
            spans.push(Span::styled(ch, Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn z_to_heatmap_cell(z: f64) -> (&'static str, Color) {
    if z > 2.0 {
        ("█", Color::Green)
    } else if z > 1.0 {
        ("▓", Color::LightGreen)
    } else if z > 0.5 {
        ("▒", Color::DarkGray)
    } else if z > -0.5 {
        ("░", Color::DarkGray)
    } else if z > -1.0 {
        ("▒", Color::DarkGray)
    } else if z > -2.0 {
        ("▓", Color::LightRed)
    } else {
        ("█", Color::Red)
    }
}

fn draw_cumulative_trend(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    if state.trials.is_empty() {
        let block = Block::default()
            .title(" Cumulative Z Trend ")
            .borders(Borders::ALL);
        f.render_widget(Paragraph::new("  Waiting...").block(block), area);
        return;
    }

    let data: Vec<(f64, f64)> = state
        .trials
        .iter()
        .enumerate()
        .map(|(i, t)| (i as f64, t.cumulative_z))
        .collect();

    let min_y = data
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::MAX, f64::min)
        .min(-2.0);
    let max_y = data
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::MIN, f64::max)
        .max(2.0);
    let x_max = (data.len() as f64).max(10.0);

    // Draw significance thresholds at Z = +/- 1.96.
    let sig_line_pos: Vec<(f64, f64)> = vec![(0.0, 1.96), (x_max, 1.96)];
    let sig_line_neg: Vec<(f64, f64)> = vec![(0.0, -1.96), (x_max, -1.96)];

    let datasets = vec![
        Dataset::default()
            .name("p=0.05")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::DarkGray))
            .data(&sig_line_pos),
        Dataset::default()
            .name("p=0.05")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::DarkGray))
            .data(&sig_line_neg),
        Dataset::default()
            .name("Cumulative Z")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .data(&data),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(" Cumulative Z Trend (+/-1.96 = p<0.05) ")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Trial")
                .bounds([0.0, x_max])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{}", x_max as usize)),
                ]),
        )
        .y_axis(
            Axis::default()
                .title("Cumulative Z")
                .bounds([min_y, max_y])
                .labels(vec![
                    Span::raw(format!("{:.1}", min_y)),
                    Span::styled("0.0", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1}", max_y)),
                ]),
        );

    f.render_widget(chart, area);
}

fn draw_phase_comparison(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let block = Block::default()
        .title(" Phase Comparison ")
        .borders(Borders::ALL);

    if state.phase_cumulative_z.is_empty() {
        f.render_widget(
            Paragraph::new("  Waiting for phase data...").block(block),
            area,
        );
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (dir, cum_z, p) in &state.phase_cumulative_z {
        let bar_width: usize = 40;
        let center = bar_width / 2;
        let pos = ((cum_z / 3.0) * center as f64) as i32 + center as i32;
        let pos = pos.clamp(0, bar_width as i32 - 1) as usize;

        let dir_color = match dir {
            IntentionDirection::Baseline => Color::Gray,
            IntentionDirection::High => Color::Green,
            IntentionDirection::Low => Color::Red,
        };

        let mut bar: Vec<Span> =
            vec![Span::styled("─", Style::default().fg(Color::DarkGray)); bar_width];
        bar[center] = Span::styled("│", Style::default().fg(Color::White));
        if pos != center {
            let (start, end) = if pos > center {
                (center, pos)
            } else {
                (pos, center)
            };
            for i in start..=end {
                bar[i] = Span::styled("█", Style::default().fg(dir_color));
            }
        }

        let mut spans: Vec<Span> = vec![Span::styled(
            format!("  {:>10} ", dir),
            Style::default()
                .fg(dir_color)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(bar);
        spans.push(Span::raw(format!(
            "  Z={:>7} p={}",
            format_z(*cum_z),
            format_p_value(*p)
        )));

        lines.push(Line::from(spans));
        lines.push(Line::from(""));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Per-source effect size summary with confidence intervals.
///
/// Each source gets one row showing its mean Z-score across all trials as a
/// diamond on a horizontal -3..+3 axis, with +/- 1 SE bars. Sources are sorted
/// by absolute effect size (largest at top). The pooled estimate is appended at
/// the bottom with a distinct marker.
fn draw_forest_plot(f: &mut Frame, area: Rect, state: &ConsciousnessSharedState) {
    let block = Block::default()
        .title(" Forest Plot (per-source effect size +/- 1 SE) ")
        .borders(Borders::ALL);

    if state.trials.is_empty() || state.source_names.is_empty() {
        f.render_widget(
            Paragraph::new("  Waiting for trial data...").block(block),
            area,
        );
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // --- compute per-source mean Z and SE -----------------------------------

    struct SourceStat {
        name: String,
        mean_z: f64,
        se: f64,
        is_control: bool,
    }

    let mut stats: Vec<SourceStat> = Vec::new();
    for src in &state.source_names {
        let zs: Vec<f64> = state
            .trials
            .iter()
            .filter_map(|t| t.source_z_scores.get(src.as_str()).copied())
            .collect();
        if zs.is_empty() {
            continue;
        }
        let n = zs.len() as f64;
        let mean = zs.iter().sum::<f64>() / n;
        let variance = if n > 1.0 {
            zs.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / (n - 1.0)
        } else {
            0.0
        };
        let se = if n > 1.0 {
            variance.sqrt() / n.sqrt()
        } else {
            0.0
        };
        stats.push(SourceStat {
            name: src.clone(),
            mean_z: mean,
            se,
            is_control: src == "prng_control",
        });
    }

    // Sort by absolute mean Z, highest first.
    stats.sort_by(|a, b| {
        b.mean_z
            .abs()
            .partial_cmp(&a.mean_z.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // --- compute pooled estimate -------------------------------------------

    let pooled_mean = if !stats.is_empty() {
        stats.iter().map(|s| s.mean_z).sum::<f64>() / stats.len() as f64
    } else {
        0.0
    };
    let pooled_se = if stats.len() > 1 {
        let var = stats
            .iter()
            .map(|s| (s.mean_z - pooled_mean).powi(2))
            .sum::<f64>()
            / (stats.len() - 1) as f64;
        var.sqrt() / (stats.len() as f64).sqrt()
    } else {
        0.0
    };

    // --- render rows --------------------------------------------------------

    let axis_width = inner.width.saturating_sub(30) as usize;
    if axis_width < 10 {
        return;
    }

    let z_min: f64 = -3.0;
    let z_max: f64 = 3.0;
    let z_range = z_max - z_min;

    // Map a Z value to a column index within `axis_width`.
    let z_to_col = |z: f64| -> usize {
        let clamped = z.clamp(z_min, z_max);
        let frac = (clamped - z_min) / z_range;
        let col = (frac * (axis_width - 1) as f64).round() as usize;
        col.min(axis_width - 1)
    };

    let zero_col = z_to_col(0.0);

    // Build one line per source, plus one separator and the pooled row.
    let max_rows = inner.height.saturating_sub(2) as usize; // leave room for header + pooled
    let header = Line::from(vec![
        Span::styled(
            format!("{:>18} ", "Source"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<width$}", " -3      0      +3", width = axis_width),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "   Z-mean",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let mut lines: Vec<Line> = vec![header];

    let build_row = |stat: &SourceStat, marker: &'static str, marker_style: Style| -> Line {
        let label = if stat.is_control {
            let truncated = if stat.name.len() > 14 {
                &stat.name[..14]
            } else {
                &stat.name
            };
            format!("{} [C]", truncated)
        } else {
            let truncated = if stat.name.len() > 18 {
                &stat.name[..18]
            } else {
                &stat.name
            };
            truncated.to_string()
        };

        let z_color = if stat.mean_z > 0.5 {
            Color::Green
        } else if stat.mean_z < -0.5 {
            Color::Red
        } else {
            Color::Gray
        };

        let mean_col = z_to_col(stat.mean_z);
        let lo_col = z_to_col(stat.mean_z - stat.se);
        let hi_col = z_to_col(stat.mean_z + stat.se);

        // Build the axis character array.
        let mut axis: Vec<Span> = (0..axis_width)
            .map(|col| {
                if col == zero_col {
                    Span::styled("│", Style::default().fg(Color::White))
                } else {
                    Span::styled(" ", Style::default().fg(Color::DarkGray))
                }
            })
            .collect();

        // Draw the CI bar.
        let ci_start = lo_col.min(hi_col);
        let ci_end = lo_col.max(hi_col);
        for col in ci_start..=ci_end {
            if col < axis_width && col != mean_col {
                axis[col] = Span::styled("─", Style::default().fg(z_color));
            }
        }

        // Place the point estimate diamond.
        if mean_col < axis_width {
            axis[mean_col] = Span::styled(marker, marker_style);
        }

        let mut spans: Vec<Span> = vec![Span::styled(
            format!("{:>18} ", label),
            Style::default().fg(z_color),
        )];
        spans.extend(axis);
        spans.push(Span::styled(
            format!("  {:>6}", format_z(stat.mean_z)),
            Style::default().fg(z_color),
        ));

        Line::from(spans)
    };

    // Source rows (limit to available height minus header and pooled row).
    let source_limit = if max_rows > 2 { max_rows - 2 } else { 0 };
    for stat in stats.iter().take(source_limit) {
        let marker_style = Style::default()
            .fg(if stat.mean_z > 0.5 {
                Color::Green
            } else if stat.mean_z < -0.5 {
                Color::Red
            } else {
                Color::Gray
            })
            .add_modifier(Modifier::BOLD);
        lines.push(build_row(stat, "\u{25C6}", marker_style)); // ◆
    }

    // Separator line.
    let sep_spans: Vec<Span> = vec![Span::styled(
        format!("{:>18} {}", "", "─".repeat(axis_width)),
        Style::default().fg(Color::DarkGray),
    )];
    lines.push(Line::from(sep_spans));

    // Pooled estimate row.
    let pooled_stat = SourceStat {
        name: "POOLED".to_string(),
        mean_z: pooled_mean,
        se: pooled_se,
        is_control: false,
    };
    let pooled_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    lines.push(build_row(&pooled_stat, "\u{25C8}", pooled_style)); // ◈

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_status_bar(
    f: &mut Frame,
    area: Rect,
    app: &ConsciousnessApp,
    state: &ConsciousnessSharedState,
) {
    let total_trials = state.trials.len();
    let expected_total = state.trials_per_phase * state.total_phases;
    let progress = if expected_total > 0 {
        (total_trials as f64 / expected_total as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let view_name = match app.view {
        ConsciousnessView::Overview => "Overview",
        ConsciousnessView::SourceHeatmap => "Heatmap",
        ConsciousnessView::CumulativeTrend => "Cumulative",
        ConsciousnessView::PhaseComparison => "Phases",
        ConsciousnessView::ForestPlot => "Forest",
    };

    let status = if state.experiment_complete {
        format!(
            " COMPLETE | {total_trials} trials | View: {view_name} | Tab: switch view | q: quit"
        )
    } else {
        format!(
            " {:.0}% | Trial {}/{} | Phase: {} | View: {view_name} | Tab: switch | q: quit",
            progress, state.trial_in_phase, state.trials_per_phase, state.current_phase,
        )
    };

    let block = Block::default().borders(Borders::ALL).border_style(
        Style::default().fg(if state.experiment_complete {
            Color::Green
        } else {
            Color::DarkGray
        }),
    );

    f.render_widget(
        Paragraph::new(Span::styled(status, Style::default().fg(Color::White))).block(block),
        area,
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consciousness_view_tab_cycles_through_all() {
        let views = [
            ConsciousnessView::Overview,
            ConsciousnessView::SourceHeatmap,
            ConsciousnessView::CumulativeTrend,
            ConsciousnessView::PhaseComparison,
            ConsciousnessView::ForestPlot,
        ];
        for (i, &view) in views.iter().enumerate() {
            let next = match view {
                ConsciousnessView::Overview => ConsciousnessView::SourceHeatmap,
                ConsciousnessView::SourceHeatmap => ConsciousnessView::CumulativeTrend,
                ConsciousnessView::CumulativeTrend => ConsciousnessView::PhaseComparison,
                ConsciousnessView::PhaseComparison => ConsciousnessView::ForestPlot,
                ConsciousnessView::ForestPlot => ConsciousnessView::Overview,
            };
            let expected_next = views[(i + 1) % views.len()];
            assert_eq!(next, expected_next, "Tab from {view:?} should go to {expected_next:?}");
        }
    }

    #[test]
    fn shared_state_new_defaults() {
        let state = ConsciousnessSharedState::new(
            ExperimentMode::Standard,
            vec!["source_a".to_string(), "source_b".to_string()],
            50,
            3,
        );
        assert!(state.trials.is_empty());
        assert_eq!(state.current_phase, IntentionDirection::Baseline);
        assert_eq!(state.phase_index, 0);
        assert_eq!(state.total_phases, 3);
        assert_eq!(state.trials_per_phase, 50);
        assert_eq!(state.source_names.len(), 2);
        assert!(!state.experiment_complete);
        assert!(state.phase_cumulative_z.is_empty());
    }

    #[test]
    fn z_to_heatmap_cell_boundaries() {
        let (ch, color) = z_to_heatmap_cell(3.0);
        assert_eq!(ch, "█");
        assert_eq!(color, Color::Green);

        let (ch, color) = z_to_heatmap_cell(1.5);
        assert_eq!(ch, "▓");
        assert_eq!(color, Color::LightGreen);

        let (ch, color) = z_to_heatmap_cell(0.0);
        assert_eq!(ch, "░");
        assert_eq!(color, Color::DarkGray);

        let (ch, color) = z_to_heatmap_cell(-1.5);
        assert_eq!(ch, "▓");
        assert_eq!(color, Color::LightRed);

        let (ch, color) = z_to_heatmap_cell(-3.0);
        assert_eq!(ch, "█");
        assert_eq!(color, Color::Red);
    }

    #[test]
    fn trial_snapshot_clone() {
        let snap = TrialSnapshot {
            trial_index: 0,
            direction: IntentionDirection::High,
            pooled_z: 1.5,
            cumulative_z: 1.2,
            p_value: 0.13,
            source_z_scores: {
                let mut m = HashMap::new();
                m.insert("test".to_string(), 1.5);
                m
            },
            ones_count: 110,
            timestamp_secs: 1.0,
        };
        let cloned = snap.clone();
        assert_eq!(cloned.trial_index, 0);
        assert_eq!(cloned.pooled_z, 1.5);
        assert_eq!(cloned.source_z_scores.get("test"), Some(&1.5));
    }

    #[test]
    fn consciousness_app_starts_with_overview() {
        let state = Arc::new(Mutex::new(ConsciousnessSharedState::new(
            ExperimentMode::Standard,
            vec![],
            50,
            3,
        )));
        let app = ConsciousnessApp::new(state);
        assert_eq!(app.view, ConsciousnessView::Overview);
        assert!(app.running.load(Ordering::SeqCst));
    }

    #[test]
    fn all_experiment_modes_supported() {
        // Verify shared state can be constructed with all 8 modes.
        let modes = [
            ExperimentMode::Standard,
            ExperimentMode::Spectroscopy,
            ExperimentMode::Structure,
            ExperimentMode::Coherence,
            ExperimentMode::Temporal,
            ExperimentMode::Adversarial,
            ExperimentMode::Feedback,
            ExperimentMode::Anomaly,
        ];
        for mode in modes {
            let state = ConsciousnessSharedState::new(mode, vec![], 10, 3);
            assert_eq!(state.mode, mode);
        }
    }
}
