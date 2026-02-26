mod ane_timing;
mod clock_jitter;
mod commpage_clock_timing;
mod dram_row_buffer;
mod mach_continuous_timing;
mod mach_timing;
mod page_fault_timing;

pub use ane_timing::AneTimingSource;
pub use clock_jitter::ClockJitterSource;
pub use commpage_clock_timing::CommPageClockTimingSource;
pub use dram_row_buffer::DRAMRowBufferSource;
pub use mach_continuous_timing::MachContinuousTimingSource;
pub use mach_timing::MachTimingSource;
pub use page_fault_timing::PageFaultTimingSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(AneTimingSource),
        Box::new(ClockJitterSource),
        Box::new(CommPageClockTimingSource),
        Box::new(DRAMRowBufferSource),
        Box::new(MachContinuousTimingSource),
        Box::new(MachTimingSource),
        Box::new(PageFaultTimingSource),
    ]
}
