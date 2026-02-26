mod keychain_timing;
mod kqueue_events;
mod mach_ipc;
mod pipe_buffer;

pub use keychain_timing::{KeychainTimingConfig, KeychainTimingSource};
pub use kqueue_events::{KqueueEventsConfig, KqueueEventsSource};
pub use mach_ipc::{MachIPCConfig, MachIPCSource};
pub use pipe_buffer::{PipeBufferConfig, PipeBufferSource};

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(KeychainTimingSource::default()),
        Box::new(KqueueEventsSource::default()),
        Box::new(MachIPCSource::default()),
        Box::new(PipeBufferSource::default()),
    ]
}
