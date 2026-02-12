//! TUI application state and event loop.

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use esoteric_core::pool::{EntropyPool, HealthReport};

pub struct App {
    pool: EntropyPool,
    refresh_rate: Duration,
    selected: usize,
    show_info: bool,
    health: Option<HealthReport>,
    entropy_history: Vec<f64>,
    rng_output: String,
    running: bool,
}

impl App {
    pub fn new(pool: EntropyPool, refresh_secs: f64) -> Self {
        Self {
            pool,
            refresh_rate: Duration::from_secs_f64(refresh_secs),
            selected: 0,
            show_info: false,
            health: None,
            entropy_history: Vec::new(),
            rng_output: String::new(),
            running: true,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Initial collection
        self.pool.collect_all();
        self.health = Some(self.pool.health_report());

        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|f| super::ui::draw(f, self))?;

            let timeout = self.refresh_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => self.running = false,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Some(ref h) = self.health
                            && self.selected < h.sources.len().saturating_sub(1)
                        {
                            self.selected += 1;
                        }
                    }
                    KeyCode::Char('i') => self.show_info = !self.show_info,
                    KeyCode::Char('r') => {
                        // Force refresh
                        self.tick();
                        last_tick = Instant::now();
                    }
                    _ => {}
                }
            }

            if last_tick.elapsed() >= self.refresh_rate {
                self.tick();
                last_tick = Instant::now();
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn tick(&mut self) {
        self.pool.collect_all();
        self.health = Some(self.pool.health_report());

        // Track average entropy
        if let Some(ref h) = self.health {
            let avg = if h.sources.is_empty() {
                0.0
            } else {
                h.sources.iter().map(|s| s.entropy).sum::<f64>() / h.sources.len() as f64
            };
            self.entropy_history.push(avg);
            if self.entropy_history.len() > 60 {
                self.entropy_history.remove(0);
            }
        }

        // Generate RNG sample
        let bytes = self.pool.get_random_bytes(16);
        self.rng_output = bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
    }

    pub fn health(&self) -> Option<&HealthReport> {
        self.health.as_ref()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn show_info(&self) -> bool {
        self.show_info
    }

    pub fn entropy_history(&self) -> &[f64] {
        &self.entropy_history
    }

    pub fn rng_output(&self) -> &str {
        &self.rng_output
    }

    pub fn source_infos(&self) -> Vec<esoteric_core::pool::SourceInfoSnapshot> {
        self.pool.source_infos()
    }
}
