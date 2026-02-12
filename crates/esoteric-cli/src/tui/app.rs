//! TUI application state and event loop.
//!
//! Design: Single-source selection. Navigate the list, press space to activate
//! a source. Only the active source collects — keeps everything fast and focused.
//! Collection runs on a background thread so the UI never blocks.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use esoteric_core::pool::{EntropyPool, HealthReport, SourceHealth};

/// Shared state between UI thread and background collector.
struct SharedState {
    health: Option<HealthReport>,
    rng_hex: String,
    collecting: bool,
    /// Per-source entropy history.
    source_history: HashMap<String, Vec<f64>>,
    /// Per-source last known stats (persists when switching sources).
    source_stats: HashMap<String, SourceHealth>,
    total_bytes: u64,
    cycle_count: u64,
    last_ms: u64,
}

pub struct App {
    pool: Arc<EntropyPool>,
    refresh_rate: Duration,
    cursor: usize,
    active: Option<usize>, // which source is actively collecting (only one)
    running: bool,
    source_names: Vec<String>,
    source_categories: Vec<String>,
    shared: Arc<Mutex<SharedState>>,
    collector_flag: Arc<AtomicBool>,
}

impl App {
    pub fn new(pool: EntropyPool, refresh_secs: f64) -> Self {
        let infos = pool.source_infos();
        let names: Vec<String> = infos.iter().map(|i| i.name.clone()).collect();
        let cats: Vec<String> = infos.iter().map(|i| i.category.clone()).collect();

        Self {
            pool: Arc::new(pool),
            refresh_rate: Duration::from_secs_f64(refresh_secs),
            cursor: 0,
            active: Some(0), // start with first source active
            running: true,
            source_names: names,
            source_categories: cats,
            shared: Arc::new(Mutex::new(SharedState {
                health: None,
                rng_hex: String::new(),
                collecting: false,
                source_history: HashMap::new(),
                source_stats: HashMap::new(),
                total_bytes: 0,
                cycle_count: 0,
                last_ms: 0,
            })),
            collector_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.kick_collect();
        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|f| super::ui::draw(f, self))?;

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key.code);
            }

            if last_tick.elapsed() >= self.refresh_rate {
                if !self.collector_flag.load(Ordering::Relaxed) {
                    self.kick_collect();
                }
                last_tick = Instant::now();
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor < self.source_names.len().saturating_sub(1) {
                    self.cursor += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Activate the source under cursor (deactivates previous)
                if self.active == Some(self.cursor) {
                    self.active = None; // deactivate
                } else {
                    self.active = Some(self.cursor);
                    self.kick_collect(); // start collecting immediately
                }
            }
            KeyCode::Char('r') => {
                self.kick_collect();
            }
            _ => {}
        }
    }

    fn kick_collect(&self) {
        if self.collector_flag.load(Ordering::Relaxed) {
            return;
        }
        let active_name = match self.active {
            Some(idx) => self.source_names[idx].clone(),
            None => return,
        };

        let pool = Arc::clone(&self.pool);
        let shared = Arc::clone(&self.shared);
        let flag = Arc::clone(&self.collector_flag);

        flag.store(true, Ordering::Relaxed);

        thread::spawn(move || {
            {
                shared.lock().unwrap().collecting = true;
            }

            let t0 = Instant::now();
            pool.collect_enabled(&[active_name.clone()]);
            let bytes = pool.get_random_bytes(32);
            let health = pool.health_report();
            let elapsed_ms = t0.elapsed().as_millis() as u64;

            let hex: String = bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");

            {
                let mut s = shared.lock().unwrap();
                s.last_ms = elapsed_ms;
                s.total_bytes += bytes.len() as u64;
                s.cycle_count += 1;
                s.rng_hex = hex;
                s.collecting = false;

                // Update per-source stats and history
                for src in &health.sources {
                    s.source_stats.insert(src.name.clone(), src.clone());
                    let hist = s.source_history.entry(src.name.clone()).or_default();
                    hist.push(src.entropy);
                    if hist.len() > 120 {
                        hist.remove(0);
                    }
                }

                s.health = Some(health);
            }

            flag.store(false, Ordering::Relaxed);
        });
    }

    // --- Public accessors for UI ---

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active.map(|i| self.source_names[i].as_str())
    }

    pub fn cursor_name(&self) -> &str {
        &self.source_names[self.cursor]
    }

    pub fn source_names(&self) -> &[String] {
        &self.source_names
    }

    pub fn source_categories(&self) -> &[String] {
        &self.source_categories
    }

    pub fn active_history(&self) -> Vec<f64> {
        if let Some(name) = self.active_name() {
            self.shared.lock().unwrap()
                .source_history.get(name).cloned().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub fn source_stat(&self, name: &str) -> Option<SourceHealth> {
        self.shared.lock().unwrap().source_stats.get(name).cloned()
    }

    pub fn rng_hex(&self) -> String {
        self.shared.lock().unwrap().rng_hex.clone()
    }

    pub fn is_collecting(&self) -> bool {
        self.shared.lock().unwrap().collecting
    }

    pub fn total_bytes(&self) -> u64 {
        self.shared.lock().unwrap().total_bytes
    }

    pub fn cycle_count(&self) -> u64 {
        self.shared.lock().unwrap().cycle_count
    }

    pub fn last_ms(&self) -> u64 {
        self.shared.lock().unwrap().last_ms
    }

    pub fn source_infos(&self) -> Vec<esoteric_core::pool::SourceInfoSnapshot> {
        self.pool.source_infos()
    }
}
