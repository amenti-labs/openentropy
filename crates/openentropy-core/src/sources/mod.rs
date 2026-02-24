//! All entropy source implementations.
//!
//! ## Source Categories
//!
//! - **Quantum**: Hardware entropy with documented quantum components
//! - **Thermal**: Johnson-Nyquist noise in oscillators
//! - **Timing**: Clock jitter, scheduler noise
//! - **System**: Kernel counters, process state
//! - **IO**: Disk, network timing
//! - **Silicon**: Cache, DRAM, pipeline state

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

pub mod silicon;
pub mod sysctl;
pub mod timing;
pub mod vmstat;
pub mod wifi;

// QUANTUM sources - hardware entropy with documented quantum components
pub mod quantum;

// PRNG control source for consciousness experiments
pub mod prng_control;

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
        // Compression/hash timing
        Box::new(compression::CompressionTimingSource),
        Box::new(compression::HashTimingSource),
        // Novel
        Box::new(novel::DispatchQueueSource),
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
        // Frontier: thermal noise research (2026-02-14)
        Box::new(frontier::DenormalTimingSource),
        Box::new(frontier::AudioPLLTimingSource),
        Box::new(frontier::USBTimingSource),
        // Frontier: unprecedented entropy sources (2026-02-14)
        Box::new(frontier::NVMeLatencySource),
        Box::new(frontier::GPUDivergenceSource),
        Box::new(frontier::PDNResonanceSource),
        Box::new(frontier::IOSurfaceCrossingSource),
        Box::new(frontier::FsyncJournalSource),
        // Frontier: two-oscillator beat frequency (CPU counter vs audio PLL)
        Box::new(frontier::CounterBeatSource),
        // Frontier: independent oscillator/PLL sources (2026-02-15)
        Box::new(frontier::DisplayPllSource),
        Box::new(frontier::PciePllSource),
        // Frontier: novel hardware domain sources (2026-02-22)
        Box::new(frontier::AneTimingSource),
        Box::new(frontier::ImuNoiseSource),
        Box::new(frontier::SmcPowerSource),
        // QUANTUM sources (2026-02-19)
        Box::new(quantum::CosmicMuonSource),
        Box::new(quantum::RadioactiveDecaySource),
        Box::new(quantum::MultiSourceQuantumSource::new()),
        // QUANTUM: NVMe kernel-level entropy sources (2026-02-22)
        Box::new(quantum::NvmeIokitSensorsSource),
        Box::new(quantum::NvmeSmartThermalSource),
        Box::new(quantum::NvmeRawDeviceSource),
        Box::new(quantum::NvmePassthroughLinuxSource),
        // PRNG control (negative control for consciousness experiments)
        Box::new(prng_control::PrngControlSource::default()),
    ]
}
