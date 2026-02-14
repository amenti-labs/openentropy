//! All 39 entropy source implementations.

pub mod helpers;

pub mod audio;
pub mod bluetooth;
pub mod camera;
pub mod compression;
pub mod cross_domain;
pub mod disk;
pub mod frontier;
pub mod gpu;
pub mod ioregistry;
pub mod memory;
pub mod network;
pub mod novel;
pub mod process;
pub mod sensor;
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
        Box::new(timing::MachTimingSource),
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
        Box::new(memory::MemoryTimingSource),
        Box::new(gpu::GPUTimingSource),
        Box::new(audio::AudioNoiseSource),
        Box::new(camera::CameraNoiseSource),
        Box::new(sensor::SensorNoiseSource),
        Box::new(bluetooth::BluetoothNoiseSource),
        // Silicon
        Box::new(silicon::DRAMRowBufferSource),
        Box::new(silicon::CacheContentionSource),
        Box::new(silicon::PageFaultTimingSource),
        Box::new(silicon::SpeculativeExecutionSource),
        // IORegistry
        Box::new(ioregistry::IORegistryEntropySource),
        // Cross-domain beat
        Box::new(cross_domain::CPUIOBeatSource),
        Box::new(cross_domain::CPUMemoryBeatSource),
        Box::new(cross_domain::MultiDomainBeatSource),
        // Compression/hash timing
        Box::new(compression::CompressionTimingSource),
        Box::new(compression::HashTimingSource),
        // Novel
        Box::new(novel::DispatchQueueSource),
        Box::new(novel::DyldTimingSource),
        Box::new(novel::VMPageTimingSource),
        Box::new(novel::SpotlightTimingSource),
        // Frontier (novel unexplored sources)
        Box::new(frontier::AMXTimingSource::default()),
        Box::new(frontier::ThreadLifecycleSource),
        Box::new(frontier::MachIPCSource::default()),
        Box::new(frontier::TLBShootdownSource::default()),
        Box::new(frontier::PipeBufferSource::default()),
        Box::new(frontier::KqueueEventsSource::default()),
        Box::new(frontier::DVFSRaceSource),
        Box::new(frontier::CASContentionSource::default()),
        Box::new(frontier::KeychainTimingSource::default()),
    ]
}
