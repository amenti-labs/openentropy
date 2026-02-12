//! TUI application state and event loop.
//!
//! Design: Collection runs on a background thread. The UI thread never blocks
//! on entropy gathering — it only reads the latest snapshot from shared state.
//! Sources start disabled; the user enables what they want to watch.

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

use esoteric_core::pool::{EntropyPool, HealthReport};

/// Per-source enable/disable state (survives across ticks).
pub struct SourceToggle {
    name: String,
    enabled: bool,
}

/// Shared state between UI thread and background collector.
struct SharedState {
    health: Option<HealthReport>,
    rng_output: String,
    collecting: bool,
    entropy_history: Vec<f64>,
    total_bytes: u64,
    last_collection_ms: u64,
}

pub struct App {
    pool: Arc<EntropyPool>,
    refresh_rate: Duration,
    selected: usize,
    show_info: bool,
    running: bool,
    toggles: Vec<SourceToggle>,
    shared: Arc<Mutex<SharedState>>,
    collector_running: Arc<AtomicBool>,
    mode: ViewMode,
    stream_buffer: Vec<String>,   // rolling hex lines for stream view
    #[allow(dead_code)]
    stream_bytes_total: u64,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode {
    Dashboard,  // source table + chart (default)
    Stream,     // live hex stream of RNG output
}

impl App {
    pub fn new(pool: EntropyPool, refresh_secs: f64) -> Self {
        let infos = pool.source_infos();
        let toggles: Vec<SourceToggle> = infos
            .iter()
            .map(|info| SourceToggle {
                name: info.name.clone(),
                // Start with only fast sources enabled (<1s typical collection)
                enabled: is_fast_source(&info.name),
            })
            .collect();

        Self {
            pool: Arc::new(pool),
            refresh_rate: Duration::from_secs_f64(refresh_secs),
            selected: 0,
            show_info: false,
            running: true,
            toggles,
            shared: Arc::new(Mutex::new(SharedState {
                health: None,
                rng_output: String::new(),
                collecting: false,
                entropy_history: Vec::new(),
                total_bytes: 0,
                last_collection_ms: 0,
            })),
            collector_running: Arc::new(AtomicBool::new(false)),
            mode: ViewMode::Dashboard,
            stream_buffer: Vec::new(),
            stream_bytes_total: 0,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Kick off first collection
        self.start_background_collect();

        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|f| super::ui::draw(f, self))?;

            // Poll for input with short timeout so UI stays responsive
            let timeout = Duration::from_millis(50);
            if event::poll(timeout)?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key.code);
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

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < self.toggles.len().saturating_sub(1) {
                    self.selected += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Toggle selected source
                if self.selected < self.toggles.len() {
                    self.toggles[self.selected].enabled = !self.toggles[self.selected].enabled;
                }
            }
            KeyCode::Char('i') => self.show_info = !self.show_info,
            KeyCode::Char('a') => {
                // Enable all
                for t in &mut self.toggles {
                    t.enabled = true;
                }
            }
            KeyCode::Char('n') => {
                // Disable all
                for t in &mut self.toggles {
                    t.enabled = false;
                }
            }
            KeyCode::Char('f') => {
                // Enable only fast sources
                for t in &mut self.toggles {
                    t.enabled = is_fast_source(&t.name);
                }
            }
            KeyCode::Char('s') => {
                // Toggle stream view
                self.mode = match self.mode {
                    ViewMode::Dashboard => ViewMode::Stream,
                    ViewMode::Stream => ViewMode::Dashboard,
                };
            }
            KeyCode::Char('r') => {
                // Force refresh
                self.start_background_collect();
            }
            _ => {}
        }
    }

    fn tick(&mut self) {
        // Update stream buffer from shared state
        {
            let shared = self.shared.lock().unwrap();
            if !shared.rng_output.is_empty() {
                self.stream_buffer.push(shared.rng_output.clone());
                // Keep last 100 lines
                if self.stream_buffer.len() > 100 {
                    self.stream_buffer.remove(0);
                }
            }
        }

        // Start a new background collection if the previous one finished
        if !self.collector_running.load(Ordering::Relaxed) {
            self.start_background_collect();
        }
    }

    fn start_background_collect(&self) {
        if self.collector_running.load(Ordering::Relaxed) {
            return; // already collecting
        }

        let pool = Arc::clone(&self.pool);
        let shared = Arc::clone(&self.shared);
        let flag = Arc::clone(&self.collector_running);

        // Build list of enabled source names
        let enabled: Vec<String> = self
            .toggles
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.name.clone())
            .collect();

        if enabled.is_empty() {
            // Nothing to collect — just generate from existing buffer
            let mut s = shared.lock().unwrap();
            s.collecting = false;
            s.rng_output = "(no sources enabled)".to_string();
            return;
        }

        flag.store(true, Ordering::Relaxed);

        thread::spawn(move || {
            {
                let mut s = shared.lock().unwrap();
                s.collecting = true;
            }

            let t0 = Instant::now();

            // Collect only from enabled sources
            pool.collect_enabled(&enabled);
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
                s.last_collection_ms = elapsed_ms;
                s.total_bytes += bytes.len() as u64;
                s.rng_output = hex;
                s.collecting = false;

                // Track entropy history
                if !health.sources.is_empty() {
                    let avg = health.sources.iter().map(|s| s.entropy).sum::<f64>()
                        / health.sources.len() as f64;
                    s.entropy_history.push(avg);
                    if s.entropy_history.len() > 120 {
                        s.entropy_history.remove(0);
                    }
                }

                s.health = Some(health);
            }

            flag.store(false, Ordering::Relaxed);
        });
    }

    // --- Public accessors for UI rendering ---

    pub fn health(&self) -> Option<HealthReport> {
        self.shared.lock().unwrap().health.clone()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn show_info(&self) -> bool {
        self.show_info
    }

    pub fn entropy_history(&self) -> Vec<f64> {
        self.shared.lock().unwrap().entropy_history.clone()
    }

    pub fn rng_output(&self) -> String {
        self.shared.lock().unwrap().rng_output.clone()
    }

    pub fn source_infos(&self) -> Vec<esoteric_core::pool::SourceInfoSnapshot> {
        self.pool.source_infos()
    }

    pub fn toggles(&self) -> &[SourceToggle] {
        &self.toggles
    }

    pub fn is_collecting(&self) -> bool {
        self.shared.lock().unwrap().collecting
    }

    pub fn total_bytes(&self) -> u64 {
        self.shared.lock().unwrap().total_bytes
    }

    pub fn last_collection_ms(&self) -> u64 {
        self.shared.lock().unwrap().last_collection_ms
    }

    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    pub fn stream_buffer(&self) -> &[String] {
        &self.stream_buffer
    }
}

impl SourceToggle {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

/// Sources that typically collect in <500ms.
fn is_fast_source(name: &str) -> bool {
    matches!(
        name,
        "clock_jitter"
            | "mach_timing"
            | "sleep_jitter"
            | "sysctl_deltas"
            | "vmstat_deltas"
            | "disk_io"
            | "memory_timing"
            | "dram_row_buffer"
            | "cache_contention"
            | "page_fault_timing"
            | "speculative_execution"
            | "cpu_io_beat"
            | "cpu_memory_beat"
            | "multi_domain_beat"
            | "hash_timing"
            | "dispatch_queue"
            | "vm_page_timing"
            | "compression_timing"
    )
}
