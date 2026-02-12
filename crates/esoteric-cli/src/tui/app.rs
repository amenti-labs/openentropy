//! TUI application state and event loop.
//!
//! Design: Collection runs on a background thread. The UI thread never blocks
//! on entropy gathering — it only reads the latest snapshot from shared state.

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

use esoteric_core::pool::{EntropyPool, HealthReport};

pub struct SourceToggle {
    name: String,
    enabled: bool,
    category: String,
    speed_tier: SpeedTier,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SpeedTier {
    Fast,   // <500ms
    Medium, // 500ms-5s
    Slow,   // >5s
}

impl SpeedTier {
    pub fn label(&self) -> &'static str {
        match self {
            SpeedTier::Fast => "⚡",
            SpeedTier::Medium => "🔶",
            SpeedTier::Slow => "🐢",
        }
    }
}

/// Shared state between UI thread and background collector.
struct SharedState {
    health: Option<HealthReport>,
    rng_output: String,
    collecting: bool,
    /// Per-source entropy history keyed by source name.
    source_history: HashMap<String, Vec<f64>>,
    total_bytes: u64,
    last_collection_ms: u64,
}

pub struct App {
    pool: Arc<EntropyPool>,
    refresh_rate: Duration,
    selected: usize,
    running: bool,
    pub toggles: Vec<SourceToggle>,
    shared: Arc<Mutex<SharedState>>,
    collector_running: Arc<AtomicBool>,
    stream_buffer: Vec<String>,
}

impl App {
    pub fn new(pool: EntropyPool, refresh_secs: f64) -> Self {
        let infos = pool.source_infos();
        let toggles: Vec<SourceToggle> = infos
            .iter()
            .map(|info| {
                let speed = classify_speed(&info.name);
                SourceToggle {
                    name: info.name.clone(),
                    enabled: speed != SpeedTier::Slow, // fast + medium on by default
                    category: info.category.clone(),
                    speed_tier: speed,
                }
            })
            .collect();

        Self {
            pool: Arc::new(pool),
            refresh_rate: Duration::from_secs_f64(refresh_secs),
            selected: 0,
            running: true,
            toggles,
            shared: Arc::new(Mutex::new(SharedState {
                health: None,
                rng_output: String::new(),
                collecting: false,
                source_history: HashMap::new(),
                total_bytes: 0,
                last_collection_ms: 0,
            })),
            collector_running: Arc::new(AtomicBool::new(false)),
            stream_buffer: Vec::new(),
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.start_background_collect();
        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|f| super::ui::draw(f, self))?;

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
                if self.selected < self.toggles.len() {
                    self.toggles[self.selected].enabled = !self.toggles[self.selected].enabled;
                }
            }
            KeyCode::Char('a') => {
                for t in &mut self.toggles {
                    t.enabled = true;
                }
            }
            KeyCode::Char('n') => {
                for t in &mut self.toggles {
                    t.enabled = false;
                }
            }
            KeyCode::Char('1') => {
                // Only fast
                for t in &mut self.toggles {
                    t.enabled = t.speed_tier == SpeedTier::Fast;
                }
            }
            KeyCode::Char('2') => {
                // Fast + medium
                for t in &mut self.toggles {
                    t.enabled = t.speed_tier != SpeedTier::Slow;
                }
            }
            KeyCode::Char('3') => {
                // All
                for t in &mut self.toggles {
                    t.enabled = true;
                }
            }
            KeyCode::Char('r') => {
                self.start_background_collect();
            }
            _ => {}
        }
    }

    fn tick(&mut self) {
        // Update stream buffer
        {
            let shared = self.shared.lock().unwrap();
            if !shared.rng_output.is_empty() && shared.rng_output != "(no sources enabled)" {
                self.stream_buffer.push(shared.rng_output.clone());
                if self.stream_buffer.len() > 200 {
                    self.stream_buffer.remove(0);
                }
            }
        }

        if !self.collector_running.load(Ordering::Relaxed) {
            self.start_background_collect();
        }
    }

    fn start_background_collect(&self) {
        if self.collector_running.load(Ordering::Relaxed) {
            return;
        }

        let pool = Arc::clone(&self.pool);
        let shared = Arc::clone(&self.shared);
        let flag = Arc::clone(&self.collector_running);

        let enabled: Vec<String> = self
            .toggles
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.name.clone())
            .collect();

        if enabled.is_empty() {
            let mut s = shared.lock().unwrap();
            s.collecting = false;
            s.rng_output = "(no sources enabled)".to_string();
            return;
        }

        flag.store(true, Ordering::Relaxed);

        thread::spawn(move || {
            {
                shared.lock().unwrap().collecting = true;
            }

            let t0 = Instant::now();
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

                // Track per-source entropy history
                for src in &health.sources {
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

    // --- Public accessors ---

    pub fn health(&self) -> Option<HealthReport> {
        self.shared.lock().unwrap().health.clone()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.toggles.get(self.selected).map(|t| t.name.as_str())
    }

    pub fn selected_history(&self) -> Vec<f64> {
        if let Some(name) = self.selected_name() {
            self.shared.lock().unwrap()
                .source_history
                .get(name)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub fn rng_output(&self) -> String {
        self.shared.lock().unwrap().rng_output.clone()
    }

    pub fn source_infos(&self) -> Vec<esoteric_core::pool::SourceInfoSnapshot> {
        self.pool.source_infos()
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

    pub fn stream_buffer(&self) -> &[String] {
        &self.stream_buffer
    }
}

impl SourceToggle {
    pub fn name(&self) -> &str { &self.name }
    pub fn enabled(&self) -> bool { self.enabled }
    pub fn category(&self) -> &str { &self.category }
    pub fn speed_tier(&self) -> SpeedTier { self.speed_tier }
}

fn classify_speed(name: &str) -> SpeedTier {
    match name {
        // Fast: <500ms
        "clock_jitter" | "mach_timing" | "sleep_jitter"
        | "disk_io" | "memory_timing"
        | "dram_row_buffer" | "cache_contention" | "page_fault_timing" | "speculative_execution"
        | "cpu_io_beat" | "cpu_memory_beat" | "multi_domain_beat"
        | "hash_timing" | "dispatch_queue" | "vm_page_timing" => SpeedTier::Fast,

        // Medium: 500ms-5s
        "sysctl_deltas" | "vmstat_deltas" | "compression_timing"
        | "sensor_noise" | "dyld_timing" | "process_table" | "ioregistry" => SpeedTier::Medium,

        // Slow: >5s
        _ => SpeedTier::Slow,
    }
}
