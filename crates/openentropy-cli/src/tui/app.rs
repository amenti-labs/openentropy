//! TUI application state and event loop.
//!
//! Design: Single-source selection. Navigate the list, press space to activate
//! a source. Only the active source collects — keeps everything fast and focused.
//! Collection runs on a background thread so the UI never blocks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use ratatui::widgets::TableState;

use openentropy_core::ConditioningMode;
use openentropy_core::conditioning::condition;
use openentropy_core::pool::{EntropyPool, SourceHealth};
use openentropy_core::session::{SessionConfig, SessionWriter};

// ---------------------------------------------------------------------------
// ChartMode
// ---------------------------------------------------------------------------

/// What the chart Y axis shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartMode {
    #[default]
    Shannon,
    MinEntropy,
    CollectTime,
    OutputValue,
    RandomWalk,
    ByteDistribution,
    Autocorrelation,
    /// Camera shot-noise visualization (sensor source)
    CameraShotNoise,
}

impl ChartMode {
    pub fn next(self) -> Self {
        match self {
            Self::Shannon => Self::MinEntropy,
            Self::MinEntropy => Self::CollectTime,
            Self::CollectTime => Self::OutputValue,
            Self::OutputValue => Self::RandomWalk,
            Self::RandomWalk => Self::ByteDistribution,
            Self::ByteDistribution => Self::Autocorrelation,
            Self::Autocorrelation => Self::CameraShotNoise,
            Self::CameraShotNoise => Self::Shannon,
        }
    }

    /// Returns true if this mode is source-specific
    /// (only relevant for a particular source)
    pub fn is_source_specific(self) -> bool {
        matches!(self, Self::CameraShotNoise)
    }

    /// Check if this mode is appropriate for the given source name
    pub fn is_applicable_for(self, source_name: &str) -> bool {
        match self {
            Self::CameraShotNoise => source_name == "camera_noise",
            _ => true,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Shannon => "Shannon H",
            Self::MinEntropy => "Min-entropy",
            Self::CollectTime => "Collect time",
            Self::OutputValue => "Output value",
            Self::RandomWalk => "Random walk",
            Self::ByteDistribution => "Byte dist",
            Self::Autocorrelation => "Autocorrelation",
            Self::CameraShotNoise => "Camera Shot Noise",
        }
    }

    pub fn y_label(self) -> &'static str {
        match self {
            Self::Shannon | Self::MinEntropy => "bits/byte",
            Self::CollectTime => "ms",
            Self::OutputValue => "[0, 1]",
            Self::RandomWalk => "sum",
            Self::ByteDistribution => "count",
            Self::Autocorrelation => "r",
            Self::CameraShotNoise => "pixels",
        }
    }

    /// Short one-line summary for the chart title bar.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Shannon => "Information content per byte (8.0 = maximum)",
            Self::MinEntropy => "Worst-case guessability per byte (NIST MCV)",
            Self::CollectTime => "Hardware collection latency",
            Self::OutputValue => "Per-sample uniformity check",
            Self::RandomWalk => "Cumulative bias detector",
            Self::ByteDistribution => "Byte value histogram",
            Self::Autocorrelation => "Sequential independence check",
            Self::CameraShotNoise => "Camera sensor shot-noise lattice",
        }
    }

    /// Multi-line description explaining what this chart shows and how to read it.
    pub fn description(self) -> &'static [&'static str] {
        match self {
            Self::Shannon => &[
                "Shannon entropy measures how unpredictable each byte is.",
                "8.0 bits/byte = perfectly random (every byte equally likely).",
                "Below 7.0 = significant patterns. Below 4.0 = mostly predictable.",
                "This is an upper bound — real randomness quality may be lower.",
            ],
            Self::MinEntropy => &[
                "Min-entropy measures how easy the most common byte is to guess.",
                "Uses the NIST SP 800-90B Most Common Value estimator with 99% CI.",
                "Always <= Shannon. This is what matters for cryptographic security.",
                "Below 6.0 = an attacker has a meaningful advantage guessing bytes.",
            ],
            Self::CollectTime => &[
                "Time taken by the hardware source to produce each sample.",
                "Natural jitter in collection time is expected and healthy —",
                "it reflects real physical processes (bus contention, scheduling).",
                "Flat line = suspicious (source may not be doing real work).",
            ],
            Self::OutputValue => &[
                "Each collection's conditioned bytes are folded into a single",
                "number between 0 and 1. For a good source, these should scatter",
                "uniformly across the range with no visible pattern or clustering.",
                "Bands or gaps suggest the source has structural bias.",
            ],
            Self::RandomWalk => &[
                "Each conditioned byte adds (byte - 128) to a running total.",
                "Good randomness wanders like Brownian motion (no trend).",
                "Steady upward/downward drift = byte bias (too many high/low values).",
                "Smooth waves = correlated output. Flat line = stuck source.",
            ],
            Self::ByteDistribution => &[
                "Counts how often each byte value (0-255) appears across all samples.",
                "A good source produces a flat, even histogram (uniform distribution).",
                "Spikes = certain values appear far more often than expected.",
                "chi2 in the title measures overall deviation from uniform.",
            ],
            Self::Autocorrelation => &[
                "Measures whether each output value predicts the next one.",
                "r near 0 = each sample is independent of the previous (good).",
                "|r| above 0.3 = concerning dependency between consecutive samples.",
                "Persistent non-zero correlation = the source has memory/structure.",
            ],
            Self::CameraShotNoise => &[
                "Visualizes live camera sensor shot-noise and dark-current grain.",
                "Grid intensity is driven directly by recent raw bytes from camera_noise.",
                "Light level and lens coverage visibly change the texture pattern.",
                "Quantum-origin noise is present, but mixed with analog readout effects.",
            ],
        }
    }

    /// Extract the relevant metric from a Sample for this chart mode.
    pub fn value_from(self, s: &Sample) -> f64 {
        match self {
            Self::Shannon => s.shannon,
            Self::MinEntropy => s.min_entropy,
            Self::CollectTime => s.collect_time_ms,
            Self::OutputValue => s.output_value,
            Self::RandomWalk
            | Self::ByteDistribution
            | Self::Autocorrelation
            | Self::CameraShotNoise => 0.0,
        }
    }

    /// Compute appropriate Y axis bounds for this chart mode.
    pub fn y_bounds(self, min_val: f64, max_val: f64) -> (f64, f64) {
        match self {
            Self::Shannon | Self::MinEntropy => {
                ((min_val - 0.5).max(0.0), (max_val + 0.5).min(8.0))
            }
            Self::CollectTime => (0.0, (max_val * 1.2).max(1.0)),
            Self::OutputValue => (0.0, 1.0),
            Self::RandomWalk => {
                let bound = min_val.abs().max(max_val.abs()).max(10.0) * 1.1;
                (-bound, bound)
            }
            Self::Autocorrelation => {
                let bound = (min_val.abs().max(max_val.abs()) + 0.1).min(1.0);
                (-bound, bound)
            }
            Self::ByteDistribution | Self::CameraShotNoise => (0.0, 1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Sample sizes the user can cycle through.
pub const SAMPLE_SIZES: [usize; 4] = [16, 32, 64, 128];

/// Maximum samples retained per source.
const MAX_HISTORY: usize = 120;

/// Canonical category display order for the TUI source list.
const CATEGORY_ORDER: &[&str] = &[
    "timing",
    "scheduling",
    "system",
    "network",
    "io",
    "sensor",
    "microarch",
    "ipc",
    "thermal",
    "gpu",
    "signal",
];

// ---------------------------------------------------------------------------
// VirtualRow — mixed header/source list for category grouping
// ---------------------------------------------------------------------------

/// A row in the TUI source list: either a category header or a source entry.
#[derive(Debug, Clone)]
pub enum VirtualRow {
    /// Category header row (collapsible).
    Header { cat_key: String },
    /// Source row — `source_idx` indexes into `source_names` / `source_categories` / etc.
    Source { source_idx: usize },
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Compute rolling lag-1 autocorrelation from a value series.
pub fn rolling_autocorr(values: &[f64], window: usize) -> Vec<f64> {
    if values.len() < 3 {
        return vec![];
    }
    let mut result = Vec::with_capacity(values.len() - 1);
    for end in 2..=values.len() {
        let start = end.saturating_sub(window);
        let w = &values[start..end];
        let n = w.len() as f64;
        let mean: f64 = w.iter().sum::<f64>() / n;
        let var: f64 = w.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        if var < 1e-10 {
            result.push(0.0);
            continue;
        }
        let cov: f64 = w
            .windows(2)
            .map(|p| (p[0] - mean) * (p[1] - mean))
            .sum::<f64>();
        result.push(cov / var);
    }
    result
}

/// Convert a byte slice to a uniform f64 in [0, 1] using all bytes.
///
/// XOR-folds the entire slice into 8 bytes, then maps to [0, 1].
/// This uses all collected bytes (not just the first 8) so the output
/// reflects the full sample regardless of sample size.
pub fn bytes_to_uniform(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut folded = [0u8; 8];
    for (i, &b) in bytes.iter().enumerate() {
        folded[i % 8] ^= b;
    }
    u64::from_le_bytes(folded) as f64 / u64::MAX as f64
}

/// Format a byte slice as space-separated hex.
pub fn format_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// Cycle to the next conditioning mode.
pub fn next_conditioning(mode: ConditioningMode) -> ConditioningMode {
    match mode {
        ConditioningMode::Sha256 => ConditioningMode::Raw,
        ConditioningMode::Raw => ConditioningMode::VonNeumann,
        ConditioningMode::VonNeumann => ConditioningMode::Sha256,
    }
}

fn preferred_chart_mode_for_source(source_name: &str) -> ChartMode {
    match source_name {
        "camera_noise" => ChartMode::CameraShotNoise,
        _ => ChartMode::RandomWalk,
    }
}

// ---------------------------------------------------------------------------
// Sample
// ---------------------------------------------------------------------------

/// One sample of per-source metrics captured each collection cycle.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub shannon: f64,
    pub min_entropy: f64,
    pub collect_time_ms: f64,
    pub output_value: f64,
}

// ---------------------------------------------------------------------------
// Visualization state
// ---------------------------------------------------------------------------

/// Keep latest raw bytes visible in visualization panels.
const MAX_STREAM_BYTES: usize = 32;

/// Keep latest raw bits visible in visualization panels.
const MAX_STREAM_BITS: usize = 256;

/// Generic stream state for source-specific visualizations.
#[derive(Debug, Clone, Default)]
pub struct SensorFlowState {
    /// Latest raw bytes used to drive visualization
    pub recent_bytes: VecDeque<u8>,
    /// Latest raw bits used to drive visualization (0/1)
    pub recent_bits: VecDeque<u8>,
    /// Number of bits changed vs previous collection cycle
    pub changed_bits_last: usize,
    /// Number of consecutive cycles with identical raw bytes
    pub repeat_streak: u64,
    /// Fingerprint of recent stream tail (for quick visual comparison)
    pub stream_fingerprint: u64,
    /// Last cycle's raw bytes
    pub last_cycle_bytes: Vec<u8>,
    /// Frame counter
    pub frame: u64,
}

// ---------------------------------------------------------------------------
// Snapshot — single-lock capture of shared state for UI rendering
// ---------------------------------------------------------------------------

/// All shared state the UI needs, captured in a single mutex lock.
pub struct Snapshot {
    pub raw_hex: String,
    pub rng_hex: String,
    pub collecting: bool,
    pub total_bytes: u64,
    pub cycle_count: u64,
    pub last_ms: u64,
    pub last_export: Option<PathBuf>,
    pub byte_freq: [u64; 256],
    pub source_stats: HashMap<String, SourceHealth>,
    pub active_history: Vec<Sample>,
    pub compare_history: Vec<Sample>,
    pub recording_samples: u64,
    /// Accumulated random walk values (cumulative sum across collections).
    pub walk: Vec<f64>,
    /// Camera shot-noise visualization state
    pub camera_noise: SensorFlowState,
}

// ---------------------------------------------------------------------------
// SharedState — internal, written by collector thread
// ---------------------------------------------------------------------------

struct SharedState {
    raw_hex: String,
    rng_hex: String,
    collecting: bool,
    source_history: HashMap<String, VecDeque<Sample>>,
    source_stats: HashMap<String, SourceHealth>,
    total_bytes: u64,
    cycle_count: u64,
    last_ms: u64,
    last_export: Option<PathBuf>,
    byte_freq: [u64; 256],
    /// Accumulated random walk: cumulative sum of (byte - 128) across all collections.
    /// Keyed by source name so switching sources shows different walks.
    walk: HashMap<String, Vec<f64>>,
    /// Session writer for TUI recording. Created when 'r' is pressed, dropped on stop.
    session_writer: Option<SessionWriter>,
    /// Camera shot-noise visualization state
    camera_noise: SensorFlowState,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pool: Arc<EntropyPool>,
    refresh_rate: Duration,
    cursor: usize,
    active: Option<usize>,
    running: bool,
    source_names: Vec<String>,
    source_categories: Vec<String>,
    source_platforms: Vec<String>,
    shared: Arc<Mutex<SharedState>>,
    collector_flag: Arc<AtomicBool>,
    conditioning_mode: ConditioningMode,
    chart_mode: ChartMode,
    paused: bool,
    compare_source: Option<usize>,
    sample_size_idx: usize,
    table_state: TableState,
    /// Whether the TUI is in recording mode (toggled with 'r').
    recording: bool,
    /// When recording started (for elapsed display).
    recording_since: Option<Instant>,
    /// Path of the session directory while recording, or last finished session.
    recording_path: Option<PathBuf>,
    /// Last start/stop recording error to surface in the TUI.
    recording_error: Option<String>,
    /// Which categories are collapsed in the source list.
    collapsed: HashSet<String>,
    /// Computed virtual row list (headers + sources).
    virtual_rows: Vec<VirtualRow>,
    /// Ordered category keys present in the pool.
    category_order: Vec<String>,
    /// Map from category key to list of source indices in that category.
    category_sources: HashMap<String, Vec<usize>>,
}

impl App {
    pub fn new(pool: EntropyPool, refresh_secs: f64) -> Self {
        let infos = pool.source_infos();
        let names: Vec<String> = infos.iter().map(|i| i.name.clone()).collect();
        let cats: Vec<String> = infos.iter().map(|i| i.category.clone()).collect();
        let plats: Vec<String> = infos.iter().map(|i| i.platform.clone()).collect();

        // Build category_sources map: category key -> [source indices]
        let mut category_sources: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, cat) in cats.iter().enumerate() {
            category_sources.entry(cat.clone()).or_default().push(i);
        }

        // Build category_order: only categories that have sources, in canonical order
        let mut category_order: Vec<String> = CATEGORY_ORDER
            .iter()
            .filter(|&&k| category_sources.contains_key(k))
            .map(|&k| k.to_string())
            .collect();
        // Append any categories not in CATEGORY_ORDER (shouldn't happen, but be safe)
        for cat in category_sources.keys() {
            if !category_order.contains(cat) {
                category_order.push(cat.clone());
            }
        }

        let collapsed: HashSet<String> = category_order.iter().cloned().collect();
        let virtual_rows = build_virtual_rows(&category_order, &category_sources, &collapsed);

        let mut app = Self {
            pool: Arc::new(pool),
            refresh_rate: Duration::from_secs_f64(refresh_secs),
            cursor: 0,
            active: None,
            running: true,
            source_names: names,
            source_categories: cats,
            source_platforms: plats,
            shared: Arc::new(Mutex::new(SharedState {
                raw_hex: String::new(),
                rng_hex: String::new(),
                collecting: false,
                source_history: HashMap::new(),
                source_stats: HashMap::new(),
                total_bytes: 0,
                cycle_count: 0,
                last_ms: 0,
                last_export: None,
                byte_freq: [0u64; 256],
                walk: HashMap::new(),
                session_writer: None,
                camera_noise: SensorFlowState::default(),
            })),
            collector_flag: Arc::new(AtomicBool::new(false)),
            conditioning_mode: ConditioningMode::default(),
            chart_mode: ChartMode::default(),
            paused: false,
            compare_source: None,
            sample_size_idx: 1, // default 32 bytes
            table_state: TableState::default().with_selected(Some(0)),
            recording: false,
            recording_since: None,
            recording_path: None,
            recording_error: None,
            collapsed,
            virtual_rows,
            category_order,
            category_sources,
        };

        // Find first source row to auto-activate, or stay on first header
        for (i, row) in app.virtual_rows.iter().enumerate() {
            if let VirtualRow::Source { source_idx } = row {
                app.cursor = i;
                app.table_state.select(Some(i));
                app.active = Some(*source_idx);
                break;
            }
        }

        app
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Install panic hook that restores terminal before printing the panic.
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
            original_hook(info);
        }));

        let result = self.run_loop(&mut terminal);

        // Always restore terminal, even if the loop returned an error.
        let _ = std::panic::take_hook(); // remove our hook
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;

        // Stop any active recording when quitting
        if self.recording {
            self.stop_recording();
        }

        // Print session path after terminal is restored so the user can see it
        if let Some(path) = &self.recording_path {
            println!("Session saved to {}", path.display());
        }

        result
    }

    fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
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
                if !self.paused && !self.collector_flag.load(Ordering::Relaxed) {
                    self.kick_collect();
                }
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.table_state.select(Some(self.cursor));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor < self.virtual_rows.len().saturating_sub(1) {
                    self.cursor += 1;
                    self.table_state.select(Some(self.cursor));
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => match &self.virtual_rows[self.cursor] {
                VirtualRow::Header { cat_key } => {
                    let cat_key = cat_key.clone();
                    if self.collapsed.contains(&cat_key) {
                        self.collapsed.remove(&cat_key);
                    } else {
                        self.collapsed.insert(cat_key);
                    }
                    self.rebuild_virtual_rows();
                }
                VirtualRow::Source { source_idx } => {
                    let source_idx = *source_idx;
                    if self.active == Some(source_idx) {
                        self.active = None;
                    } else {
                        let name = &self.source_names[source_idx];
                        let mut s = self.shared.lock().unwrap();
                        s.source_history.remove(name);
                        s.byte_freq = [0u64; 256];
                        drop(s);
                        self.active = Some(source_idx);
                        self.chart_mode = preferred_chart_mode_for_source(name);
                        self.kick_collect();
                    }
                }
            },
            KeyCode::Char('{') => {
                // Jump to previous category header
                for i in (0..self.cursor).rev() {
                    if matches!(&self.virtual_rows[i], VirtualRow::Header { .. }) {
                        self.cursor = i;
                        self.table_state.select(Some(self.cursor));
                        break;
                    }
                }
            }
            KeyCode::Char('}') => {
                // Jump to next category header
                for i in (self.cursor + 1)..self.virtual_rows.len() {
                    if matches!(&self.virtual_rows[i], VirtualRow::Header { .. }) {
                        self.cursor = i;
                        self.table_state.select(Some(self.cursor));
                        break;
                    }
                }
            }
            KeyCode::Char('C') => {
                // Toggle collapse/expand all
                if self.collapsed.len() == self.category_order.len() {
                    // All collapsed → expand all
                    self.collapsed.clear();
                } else {
                    // Some or none collapsed → collapse all
                    for cat in &self.category_order {
                        self.collapsed.insert(cat.clone());
                    }
                }
                self.rebuild_virtual_rows();
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if self.recording {
                    self.stop_recording();
                } else {
                    self.start_recording();
                }
            }
            KeyCode::Char('c') => {
                self.conditioning_mode = next_conditioning(self.conditioning_mode);
                // Reset random walks — walk shape depends on conditioning mode
                if let Ok(mut s) = self.shared.lock() {
                    s.walk.clear();
                }
                self.kick_collect();
            }
            KeyCode::Char('g') => {
                let active = self.active_name().unwrap_or("");
                let mut next = self.chart_mode.next();
                // Skip source-specific visualizations when they do not match active source.
                for _ in 0..20 {
                    if !next.is_source_specific() || next.is_applicable_for(active) {
                        break;
                    }
                    next = next.next();
                }
                self.chart_mode = next;
            }
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('s') => self.export_snapshot(),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char(']') => {
                let secs = (self.refresh_rate.as_secs_f64() / 2.0).max(0.1);
                self.refresh_rate = Duration::from_secs_f64(secs);
            }
            KeyCode::Char('-') | KeyCode::Char('[') => {
                let secs = (self.refresh_rate.as_secs_f64() * 2.0).min(10.0);
                self.refresh_rate = Duration::from_secs_f64(secs);
            }
            KeyCode::Tab => {
                // Compare only works when cursor is on a source row
                if self.compare_source.is_some() {
                    self.compare_source = None;
                } else if let Some(source_idx) = self.cursor_source_idx()
                    && self.active != Some(source_idx)
                {
                    self.compare_source = Some(source_idx);
                }
            }
            KeyCode::Char('n') => {
                self.sample_size_idx = (self.sample_size_idx + 1) % SAMPLE_SIZES.len();
                self.shared.lock().unwrap().byte_freq = [0u64; 256];
                self.kick_collect();
            }
            _ => {}
        }
    }

    /// Rebuild virtual rows after collapse state changes. Clamps cursor.
    fn rebuild_virtual_rows(&mut self) {
        self.virtual_rows = build_virtual_rows(
            &self.category_order,
            &self.category_sources,
            &self.collapsed,
        );
        // Clamp cursor
        if self.virtual_rows.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.virtual_rows.len() {
            self.cursor = self.virtual_rows.len() - 1;
        }
        self.table_state.select(Some(self.cursor));
    }

    fn start_recording(&mut self) {
        let config = SessionConfig {
            // Keep metadata stable even if the user switches active source while recording.
            sources: self.source_names.clone(),
            conditioning: self.conditioning_mode,
            output_dir: PathBuf::from("sessions"),
            ..Default::default()
        };

        match SessionWriter::new(config) {
            Ok(writer) => {
                self.recording_path = Some(writer.session_dir().to_path_buf());
                self.shared.lock().unwrap().session_writer = Some(writer);
                self.recording = true;
                self.recording_since = Some(Instant::now());
                self.recording_error = None;
            }
            Err(e) => {
                self.recording_error = Some(e.to_string());
            }
        }
    }

    fn stop_recording(&mut self) {
        self.recording = false;
        self.recording_since = None;

        // Take the writer out and finish it
        let writer = self.shared.lock().unwrap().session_writer.take();
        if let Some(writer) = writer {
            match writer.finish() {
                Ok(path) => {
                    self.recording_path = Some(path);
                    self.recording_error = None;
                }
                Err(e) => {
                    self.recording_error = Some(e.to_string());
                }
            }
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
        let mode = self.conditioning_mode;
        let sample_size = self.sample_size();

        flag.store(true, Ordering::Relaxed);

        thread::spawn(move || {
            shared.lock().unwrap().collecting = true;

            let inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let raw_bytes = pool
                    .get_source_raw_bytes(&active_name, sample_size)
                    .unwrap_or_default();
                let cond_bytes = condition(&raw_bytes, sample_size, mode);
                let health = pool.health_report();

                let mut s = shared.lock().unwrap();
                s.total_bytes += cond_bytes.len() as u64;
                s.cycle_count += 1;
                s.raw_hex = format_hex(&raw_bytes);
                s.rng_hex = format_hex(&cond_bytes);

                // Extend the random walk for the active source
                {
                    let walk = s.walk.entry(active_name.clone()).or_default();
                    let mut sum = walk.last().copied().unwrap_or(0.0);
                    for &b in &cond_bytes {
                        sum += b as f64 - 128.0;
                        walk.push(sum);
                    }
                    // Cap at 8192 points — trim from front to keep the latest
                    const MAX_WALK: usize = 8192;
                    if walk.len() > MAX_WALK {
                        let excess = walk.len() - MAX_WALK;
                        walk.drain(..excess);
                    }
                }
                for &b in &cond_bytes {
                    s.byte_freq[b as usize] += 1;
                }
                s.collecting = false;

                // Update camera shot-noise visualization state
                if active_name == "camera_noise" {
                    update_flow_state(&mut s.camera_noise, &raw_bytes);
                }

                // Write to session if recording
                if let Some(ref mut writer) = s.session_writer {
                    let _ = writer.write_sample(&active_name, &raw_bytes, &cond_bytes);
                }

                for src in &health.sources {
                    s.source_stats.insert(src.name.clone(), src.clone());
                    if src.name == active_name {
                        s.last_ms = (src.time * 1000.0) as u64;
                        let hist = s.source_history.entry(src.name.clone()).or_default();
                        hist.push_back(Sample {
                            shannon: src.entropy,
                            min_entropy: src.min_entropy,
                            collect_time_ms: src.time * 1000.0,
                            output_value: bytes_to_uniform(&cond_bytes),
                        });
                        if hist.len() > MAX_HISTORY {
                            hist.pop_front();
                        }
                    }
                }
            }));

            if inner.is_err()
                && let Ok(mut s) = shared.lock()
            {
                s.collecting = false;
            }
            flag.store(false, Ordering::Relaxed);
        });
    }

    fn export_snapshot(&self) {
        let s = self.shared.lock().unwrap();
        let source = self.active_name().unwrap_or("unknown");
        let history: Vec<serde_json::Value> = s
            .source_history
            .get(source)
            .map(|h| {
                h.iter()
                    .map(|sample| {
                        serde_json::json!({
                            "shannon": sample.shannon,
                            "min_entropy": sample.min_entropy,
                            "collect_time_ms": sample.collect_time_ms,
                            "output_value": sample.output_value,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let stat = s.source_stats.get(source);
        let json = serde_json::json!({
            "source": source,
            "conditioning": self.conditioning_mode.to_string(),
            "total_bytes": s.total_bytes,
            "cycle_count": s.cycle_count,
            "last_stat": stat.map(|st| serde_json::json!({
                "entropy": st.entropy,
                "min_entropy": st.min_entropy,
                "bytes": st.bytes,
                "time": st.time,
                "healthy": st.healthy,
            })),
            "history": history,
        });

        let epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = PathBuf::from(format!("openentropy-snapshot-{epoch}.json"));

        drop(s);

        if let Ok(contents) = serde_json::to_string_pretty(&json)
            && std::fs::write(&path, contents).is_ok()
        {
            self.shared.lock().unwrap().last_export = Some(path);
        }
    }

    // --- Public accessors (non-shared state, no lock needed) ---

    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn active(&self) -> Option<usize> {
        self.active
    }
    /// Returns the source index if the cursor is on a source row, None if on a header.
    pub fn cursor_source_idx(&self) -> Option<usize> {
        self.virtual_rows
            .get(self.cursor)
            .and_then(|row| match row {
                VirtualRow::Source { source_idx } => Some(*source_idx),
                VirtualRow::Header { .. } => None,
            })
    }
    pub fn virtual_rows(&self) -> &[VirtualRow] {
        &self.virtual_rows
    }
    pub fn is_collapsed(&self, cat_key: &str) -> bool {
        self.collapsed.contains(cat_key)
    }
    #[cfg(test)]
    pub fn category_order(&self) -> &[String] {
        &self.category_order
    }
    pub fn category_sources(&self) -> &HashMap<String, Vec<usize>> {
        &self.category_sources
    }
    pub fn source_names(&self) -> &[String] {
        &self.source_names
    }
    pub fn source_categories(&self) -> &[String] {
        &self.source_categories
    }
    pub fn source_platforms(&self) -> &[String] {
        &self.source_platforms
    }
    pub fn chart_mode(&self) -> ChartMode {
        self.chart_mode
    }
    pub fn conditioning_mode(&self) -> ConditioningMode {
        self.conditioning_mode
    }
    pub fn refresh_rate_secs(&self) -> f64 {
        self.refresh_rate.as_secs_f64()
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn sample_size(&self) -> usize {
        SAMPLE_SIZES[self.sample_size_idx]
    }
    pub fn compare_source(&self) -> Option<usize> {
        self.compare_source
    }
    pub fn table_state_mut(&mut self) -> &mut TableState {
        &mut self.table_state
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn recording_elapsed(&self) -> Option<Duration> {
        self.recording_since.map(|t| t.elapsed())
    }

    pub fn recording_path(&self) -> Option<&PathBuf> {
        self.recording_path.as_ref()
    }

    pub fn recording_error(&self) -> Option<&str> {
        self.recording_error.as_deref()
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active.map(|i| self.source_names[i].as_str())
    }

    pub fn compare_name(&self) -> Option<&str> {
        self.compare_source.map(|i| self.source_names[i].as_str())
    }

    pub fn source_infos(&self) -> Vec<openentropy_core::pool::SourceInfoSnapshot> {
        self.pool.source_infos()
    }

    /// Capture all shared state in a single mutex lock for one UI frame.
    pub fn snapshot(&self) -> Snapshot {
        let s = match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let history_for = |name: Option<&str>| -> Vec<Sample> {
            name.and_then(|n| s.source_history.get(n))
                .map(|d| d.iter().copied().collect())
                .unwrap_or_default()
        };

        // Update recording sample count from the writer
        let rec_samples = s.session_writer.as_ref().map_or(0, |w| w.total_samples());

        Snapshot {
            raw_hex: s.raw_hex.clone(),
            rng_hex: s.rng_hex.clone(),
            collecting: s.collecting,
            total_bytes: s.total_bytes,
            cycle_count: s.cycle_count,
            last_ms: s.last_ms,
            last_export: s.last_export.clone(),
            byte_freq: s.byte_freq,
            source_stats: s.source_stats.clone(),
            active_history: history_for(self.active_name()),
            compare_history: history_for(self.compare_name()),
            recording_samples: rec_samples,
            walk: self
                .active_name()
                .and_then(|n| s.walk.get(n))
                .cloned()
                .unwrap_or_default(),
            camera_noise: s.camera_noise.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Visualization state update functions
// ---------------------------------------------------------------------------

fn append_stream(state_bytes: &mut VecDeque<u8>, state_bits: &mut VecDeque<u8>, raw_bytes: &[u8]) {
    for &byte in raw_bytes {
        state_bytes.push_back(byte);
        while state_bytes.len() > MAX_STREAM_BYTES {
            state_bytes.pop_front();
        }

        // MSB->LSB for readability in panel display.
        for bit in (0..8).rev() {
            state_bits.push_back((byte >> bit) & 1);
        }
        while state_bits.len() > MAX_STREAM_BITS {
            state_bits.pop_front();
        }
    }
}

fn count_bit_changes(a: &[u8], b: &[u8]) -> usize {
    let max_len = a.len().max(b.len());
    let mut changes = 0usize;
    for i in 0..max_len {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        changes += (av ^ bv).count_ones() as usize;
    }
    changes
}

fn stream_fingerprint(bytes: &VecDeque<u8>) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Build the virtual row list from category order, source map, and collapse state.
fn build_virtual_rows(
    category_order: &[String],
    category_sources: &HashMap<String, Vec<usize>>,
    collapsed: &HashSet<String>,
) -> Vec<VirtualRow> {
    let mut rows = Vec::new();
    for cat in category_order {
        rows.push(VirtualRow::Header {
            cat_key: cat.clone(),
        });
        if !collapsed.contains(cat)
            && let Some(sources) = category_sources.get(cat)
        {
            for &idx in sources {
                rows.push(VirtualRow::Source { source_idx: idx });
            }
        }
    }
    rows
}

fn update_flow_state(state: &mut SensorFlowState, raw_bytes: &[u8]) {
    state.frame = state.frame.wrapping_add(1);
    state.changed_bits_last = count_bit_changes(raw_bytes, &state.last_cycle_bytes);
    if raw_bytes == state.last_cycle_bytes.as_slice() {
        state.repeat_streak = state.repeat_streak.saturating_add(1);
    } else {
        state.repeat_streak = 0;
    }
    state.last_cycle_bytes.clear();
    state.last_cycle_bytes.extend_from_slice(raw_bytes);
    append_stream(&mut state.recent_bytes, &mut state.recent_bits, raw_bytes);
    state.stream_fingerprint = stream_fingerprint(&state.recent_bytes);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_mode_cycles_through_all_variants() {
        let mode = ChartMode::Shannon;
        let mode = mode.next();
        assert_eq!(mode, ChartMode::MinEntropy);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::CollectTime);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::OutputValue);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::RandomWalk);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::ByteDistribution);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::Autocorrelation);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::CameraShotNoise);
        let mode = mode.next();
        assert_eq!(mode, ChartMode::Shannon);
    }

    #[test]
    fn chart_mode_default_is_shannon() {
        assert_eq!(ChartMode::default(), ChartMode::Shannon);
    }

    #[test]
    fn chart_mode_labels() {
        assert_eq!(ChartMode::Shannon.label(), "Shannon H");
        assert_eq!(ChartMode::MinEntropy.label(), "Min-entropy");
        assert_eq!(ChartMode::CollectTime.label(), "Collect time");
        assert_eq!(ChartMode::OutputValue.label(), "Output value");
        assert_eq!(ChartMode::RandomWalk.label(), "Random walk");
        assert_eq!(ChartMode::ByteDistribution.label(), "Byte dist");
        assert_eq!(ChartMode::Autocorrelation.label(), "Autocorrelation");
        assert_eq!(ChartMode::CameraShotNoise.label(), "Camera Shot Noise");
    }

    #[test]
    fn chart_mode_descriptions_non_empty() {
        for mode in [
            ChartMode::Shannon,
            ChartMode::MinEntropy,
            ChartMode::CollectTime,
            ChartMode::OutputValue,
            ChartMode::RandomWalk,
            ChartMode::ByteDistribution,
            ChartMode::Autocorrelation,
            ChartMode::CameraShotNoise,
        ] {
            assert!(
                !mode.description().is_empty(),
                "{mode:?} has empty description"
            );
        }
    }

    #[test]
    fn chart_mode_y_labels() {
        assert_eq!(ChartMode::Shannon.y_label(), "bits/byte");
        assert_eq!(ChartMode::MinEntropy.y_label(), "bits/byte");
        assert_eq!(ChartMode::CollectTime.y_label(), "ms");
        assert_eq!(ChartMode::OutputValue.y_label(), "[0, 1]");
        assert_eq!(ChartMode::RandomWalk.y_label(), "sum");
        assert_eq!(ChartMode::ByteDistribution.y_label(), "count");
        assert_eq!(ChartMode::Autocorrelation.y_label(), "r");
        assert_eq!(ChartMode::CameraShotNoise.y_label(), "pixels");
    }

    #[test]
    fn chart_mode_value_from_extracts_correct_field() {
        let s = Sample {
            shannon: 7.5,
            min_entropy: 6.2,
            collect_time_ms: 3.125,
            output_value: 0.42,
        };
        assert_eq!(ChartMode::Shannon.value_from(&s), 7.5);
        assert_eq!(ChartMode::MinEntropy.value_from(&s), 6.2);
        assert_eq!(ChartMode::CollectTime.value_from(&s), 3.125);
        assert_eq!(ChartMode::OutputValue.value_from(&s), 0.42);
        assert_eq!(ChartMode::ByteDistribution.value_from(&s), 0.0);
        assert_eq!(ChartMode::Autocorrelation.value_from(&s), 0.0);
    }

    #[test]
    fn chart_mode_y_bounds_entropy() {
        let (lo, hi) = ChartMode::Shannon.y_bounds(7.0, 7.8);
        assert!((lo - 6.5).abs() < 1e-10);
        assert!((hi - 8.0).abs() < 1e-10); // clamped to 8.0
    }

    #[test]
    fn chart_mode_y_bounds_collect_time() {
        let (lo, hi) = ChartMode::CollectTime.y_bounds(0.5, 2.0);
        assert_eq!(lo, 0.0);
        assert!((hi - 2.4).abs() < 1e-10);
    }

    #[test]
    fn chart_mode_y_bounds_output_value_fixed() {
        let (lo, hi) = ChartMode::OutputValue.y_bounds(0.2, 0.8);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 1.0);
    }

    #[test]
    fn chart_mode_y_bounds_autocorrelation_symmetric() {
        let (lo, hi) = ChartMode::Autocorrelation.y_bounds(-0.3, 0.5);
        assert!(lo < 0.0);
        assert!(hi > 0.0);
        assert!((lo + hi).abs() < 1e-10, "bounds should be symmetric");
    }

    #[test]
    fn bytes_to_uniform_zero() {
        assert_eq!(bytes_to_uniform(&[0u8; 8]), 0.0);
    }

    #[test]
    fn bytes_to_uniform_max() {
        assert_eq!(bytes_to_uniform(&[0xFF; 8]), 1.0);
    }

    #[test]
    fn bytes_to_uniform_in_range() {
        let val = bytes_to_uniform(&[0x80, 0, 0, 0, 0, 0, 0, 0]);
        assert!(val > 0.0 && val < 1.0, "expected (0, 1), got {val}");
    }

    #[test]
    fn bytes_to_uniform_short_input() {
        let val = bytes_to_uniform(&[0xFF, 0xFF]);
        assert!(
            val > 0.0 && val < 0.01,
            "short input should be small, got {val}"
        );
    }

    #[test]
    fn bytes_to_uniform_uses_all_bytes() {
        // With XOR-fold, changing any byte in the input should change the output
        let a = bytes_to_uniform(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let b = bytes_to_uniform(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 99]);
        assert_ne!(a, b, "changing byte 10 should affect the output");
    }

    #[test]
    fn bytes_to_uniform_empty() {
        assert_eq!(bytes_to_uniform(&[]), 0.0);
    }

    #[test]
    fn format_hex_basic() {
        assert_eq!(format_hex(&[0xab, 0xcd, 0x01]), "ab cd 01");
    }

    #[test]
    fn format_hex_empty() {
        assert_eq!(format_hex(&[]), "");
    }

    #[test]
    fn format_hex_single() {
        assert_eq!(format_hex(&[0xff]), "ff");
    }

    #[test]
    fn next_conditioning_cycles() {
        let a = next_conditioning(ConditioningMode::Sha256);
        assert_eq!(a, ConditioningMode::Raw);
        let b = next_conditioning(a);
        assert_eq!(b, ConditioningMode::VonNeumann);
        let c = next_conditioning(b);
        assert_eq!(c, ConditioningMode::Sha256);
    }

    #[test]
    fn rolling_autocorr_too_short() {
        assert!(rolling_autocorr(&[], 10).is_empty());
        assert!(rolling_autocorr(&[1.0], 10).is_empty());
        assert!(rolling_autocorr(&[1.0, 2.0], 10).is_empty());
    }

    #[test]
    fn rolling_autocorr_constant_series() {
        let vals = vec![5.0; 10];
        let result = rolling_autocorr(&vals, 20);
        assert_eq!(result.len(), 9);
        for r in &result {
            assert_eq!(*r, 0.0);
        }
    }

    #[test]
    fn rolling_autocorr_alternating() {
        let vals: Vec<f64> = (0..20)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let result = rolling_autocorr(&vals, 20);
        assert!(!result.is_empty());
        let last = *result.last().unwrap();
        assert!(
            last < -0.5,
            "expected negative autocorr for alternating, got {last}"
        );
    }

    #[test]
    fn rolling_autocorr_length() {
        let vals: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let result = rolling_autocorr(&vals, 30);
        assert_eq!(result.len(), 49);
    }

    #[test]
    fn sample_sizes_are_powers_of_two() {
        for &sz in &SAMPLE_SIZES {
            assert!(sz.is_power_of_two(), "{sz} is not a power of two");
        }
    }

    #[test]
    fn sample_sizes_sorted_ascending() {
        for w in SAMPLE_SIZES.windows(2) {
            assert!(w[0] < w[1], "SAMPLE_SIZES not sorted: {} >= {}", w[0], w[1]);
        }
    }

    // --- Virtual row / category grouping tests ---

    fn make_test_categories() -> (Vec<String>, HashMap<String, Vec<usize>>) {
        // Simulate 5 sources across 2 categories
        let order = vec!["timing".to_string(), "io".to_string()];
        let mut map = HashMap::new();
        map.insert("timing".to_string(), vec![0, 1, 2]);
        map.insert("io".to_string(), vec![3, 4]);
        (order, map)
    }

    #[test]
    fn build_virtual_rows_all_expanded() {
        let (order, map) = make_test_categories();
        let collapsed = HashSet::new();
        let rows = build_virtual_rows(&order, &map, &collapsed);
        // 2 headers + 5 sources = 7 rows
        assert_eq!(rows.len(), 7);
        assert!(matches!(&rows[0], VirtualRow::Header { cat_key } if cat_key == "timing"));
        assert!(matches!(&rows[1], VirtualRow::Source { source_idx: 0 }));
        assert!(matches!(&rows[2], VirtualRow::Source { source_idx: 1 }));
        assert!(matches!(&rows[3], VirtualRow::Source { source_idx: 2 }));
        assert!(matches!(&rows[4], VirtualRow::Header { cat_key } if cat_key == "io"));
        assert!(matches!(&rows[5], VirtualRow::Source { source_idx: 3 }));
        assert!(matches!(&rows[6], VirtualRow::Source { source_idx: 4 }));
    }

    #[test]
    fn build_virtual_rows_collapsed_hides_sources() {
        let (order, map) = make_test_categories();
        let mut collapsed = HashSet::new();
        collapsed.insert("timing".to_string());
        let rows = build_virtual_rows(&order, &map, &collapsed);
        // timing collapsed: 1 header, io expanded: 1 header + 2 sources = 4
        assert_eq!(rows.len(), 4);
        assert!(matches!(&rows[0], VirtualRow::Header { cat_key } if cat_key == "timing"));
        assert!(matches!(&rows[1], VirtualRow::Header { cat_key } if cat_key == "io"));
        assert!(matches!(&rows[2], VirtualRow::Source { source_idx: 3 }));
    }

    #[test]
    fn build_virtual_rows_all_collapsed() {
        let (order, map) = make_test_categories();
        let mut collapsed = HashSet::new();
        collapsed.insert("timing".to_string());
        collapsed.insert("io".to_string());
        let rows = build_virtual_rows(&order, &map, &collapsed);
        // Only 2 header rows
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0], VirtualRow::Header { .. }));
        assert!(matches!(&rows[1], VirtualRow::Header { .. }));
    }

    #[test]
    fn cursor_source_idx_returns_none_for_header() {
        let (order, map) = make_test_categories();
        let collapsed = HashSet::new();
        let rows = build_virtual_rows(&order, &map, &collapsed);
        // Row 0 is a header
        let result = rows.get(0).and_then(|r| match r {
            VirtualRow::Source { source_idx } => Some(*source_idx),
            VirtualRow::Header { .. } => None,
        });
        assert_eq!(result, None);
    }

    #[test]
    fn cursor_source_idx_returns_some_for_source() {
        let (order, map) = make_test_categories();
        let collapsed = HashSet::new();
        let rows = build_virtual_rows(&order, &map, &collapsed);
        // Row 1 is a source (idx 0)
        let result = rows.get(1).and_then(|r| match r {
            VirtualRow::Source { source_idx } => Some(*source_idx),
            VirtualRow::Header { .. } => None,
        });
        assert_eq!(result, Some(0));
    }
}
