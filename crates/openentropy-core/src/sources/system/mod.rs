mod getentropy_timing;
mod ioregistry;
mod proc_info_timing;
mod process;
mod sysctl;
mod vmstat;

pub use getentropy_timing::GetentropyTimingSource;
pub use ioregistry::IORegistryEntropySource;
pub use proc_info_timing::ProcInfoTimingSource;
pub use process::ProcessSource;
pub use sysctl::SysctlSource;
pub use vmstat::VmstatSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(GetentropyTimingSource),
        Box::new(IORegistryEntropySource),
        Box::new(ProcInfoTimingSource),
        Box::new(ProcessSource::new()),
        Box::new(SysctlSource::new()),
        Box::new(VmstatSource::new()),
    ]
}
