#[cfg(target_os = "macos")]
mod coreaudio_ffi;

mod audio_pll_timing;
mod counter_beat;
mod display_pll;
mod pcie_pll;

pub use audio_pll_timing::AudioPLLTimingSource;
pub use counter_beat::CounterBeatSource;
pub use display_pll::DisplayPllSource;
pub use pcie_pll::PciePllSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(AudioPLLTimingSource),
        Box::new(CounterBeatSource),
        Box::new(DisplayPllSource),
        Box::new(PciePllSource),
    ]
}
