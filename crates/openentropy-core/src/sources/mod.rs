//! All entropy source implementations.
//!
//! ## Source Categories
//!
//! - **Sensor**: Camera dark-frame noise, audio ADC noise
//! - **Thermal**: Johnson-Nyquist noise in oscillators
//! - **Timing**: Clock jitter, scheduler noise
//! - **System**: Kernel counters, process state
//! - **IO**: Disk, network timing
//! - **Silicon**: DRAM, pipeline state
//! - **Frontier**: Apple Silicon-specific and kernel-internal sources

pub mod helpers;

pub mod audio;
pub mod bluetooth;
pub mod camera;
pub mod compression;
pub mod disk;
pub mod frontier;
pub mod ioregistry;
pub mod network;
pub mod novel;
pub mod process;

pub mod silicon;
pub mod sysctl;
pub mod timing;
pub mod vmstat;
pub mod wifi;

use crate::source::EntropySource;

/// All entropy source constructors. Each returns a boxed source.
pub fn all_sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        // Timing
        Box::new(timing::ClockJitterSource),
        Box::new(timing::SleepJitterSource),
        // System
        Box::new(sysctl::SysctlSource::new()),
        Box::new(vmstat::VmstatSource::new()),
        Box::new(process::ProcessSource::new()),
        // Network
        Box::new(network::DNSTimingSource::new()),
        Box::new(network::TCPConnectSource::new()),
        Box::new(wifi::WiFiRSSISource::new()),
        // Hardware
        Box::new(disk::DiskIOSource),
        Box::new(audio::AudioNoiseSource::default()),
        Box::new(camera::CameraNoiseSource::default()),
        Box::new(bluetooth::BluetoothNoiseSource),
        // Silicon
        Box::new(silicon::DRAMRowBufferSource),
        Box::new(silicon::PageFaultTimingSource),
        Box::new(silicon::SpeculativeExecutionSource),
        // IORegistry
        Box::new(ioregistry::IORegistryEntropySource),
        // Compression/hash timing
        Box::new(compression::CompressionTimingSource),
        Box::new(compression::HashTimingSource),
        // Novel
        Box::new(novel::SpotlightTimingSource),
        // Frontier (novel unexplored sources)
        Box::new(frontier::AMXTimingSource::default()),
        Box::new(frontier::ThreadLifecycleSource),
        Box::new(frontier::MachIPCSource::default()),
        Box::new(frontier::TLBShootdownSource::default()),
        Box::new(frontier::PipeBufferSource::default()),
        Box::new(frontier::KqueueEventsSource::default()),
        Box::new(frontier::DVFSRaceSource),
        Box::new(frontier::KeychainTimingSource::default()),
        Box::new(frontier::AudioPLLTimingSource),
        Box::new(frontier::MachContinuousTimingSource),
        Box::new(frontier::GPUDivergenceSource),
        Box::new(frontier::IOSurfaceCrossingSource),
        Box::new(frontier::FsyncJournalSource),
        Box::new(frontier::DisplayPllSource),
        Box::new(frontier::PciePllSource),
        Box::new(frontier::PECoreArithmeticSource),
        Box::new(frontier::MemoryBusCryptoSource),
        Box::new(frontier::TimerCoalescingSource),
        Box::new(frontier::DispatchQueueTimingSource),
        Box::new(frontier::NLInferenceTimingSource),
        Box::new(frontier::ICCAtomicContentionSource),
        Box::new(frontier::APRRJitTimingSource),
        Box::new(frontier::PreemptionBoundarySource),
        Box::new(frontier::SEVEventTimingSource),
        Box::new(frontier::CommPageClockTimingSource),
        Box::new(frontier::SMCHighVarTimingSource),
        Box::new(frontier::ProcInfoTimingSource),
        Box::new(frontier::GetentropyTimingSource),
        Box::new(frontier::PrefetcherStateSource),
        Box::new(frontier::USBEnumerationSource),
        Box::new(frontier::GxfRegisterTimingSource),
        Box::new(frontier::CntfrqCacheTimingSource),
        Box::new(frontier::CommonCryptoAesTimingSource),
        Box::new(frontier::DualClockDomainSource),
        Box::new(frontier::SITVASource),
        // Frontier: ANE + NVMe deep sources
        Box::new(frontier::AneTimingSource),
        Box::new(frontier::NvmeIokitSensorsSource),
        Box::new(frontier::NvmeRawDeviceSource),
        Box::new(frontier::NvmePassthroughLinuxSource),
    ]
}
