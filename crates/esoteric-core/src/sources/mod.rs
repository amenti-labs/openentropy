//! All 30 entropy source implementations.

pub mod timing;
pub mod sysctl;
pub mod vmstat;
pub mod network;
pub mod wifi;
pub mod disk;
pub mod memory;
pub mod gpu;
pub mod audio;
pub mod camera;
pub mod sensor;
pub mod bluetooth;
pub mod silicon;
pub mod ioregistry;
pub mod cross_domain;
pub mod compression;
pub mod novel;
pub mod process;

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
    ]
}
