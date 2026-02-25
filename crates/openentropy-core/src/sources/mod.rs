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
        // Frontier: thermal noise research (2026-02-14)
        Box::new(frontier::AudioPLLTimingSource),
        // Frontier: unprecedented entropy sources (2026-02-14)
        Box::new(frontier::NVMeLatencySource),
        Box::new(frontier::MachContinuousTimingSource),
        Box::new(frontier::GPUDivergenceSource),
        Box::new(frontier::IOSurfaceCrossingSource),
        Box::new(frontier::FsyncJournalSource),
        // Frontier: independent oscillator/PLL sources (2026-02-15)
        Box::new(frontier::DisplayPllSource),
        Box::new(frontier::PciePllSource),
        // Frontier: deep hardware sources (2026-02-22)
        Box::new(frontier::PECoreArithmeticSource),
        Box::new(frontier::MemoryBusCryptoSource),
        // Frontier: esoteric sources — SMC, OS timer, DRBG oracle (2026-02-24)
        Box::new(frontier::TimerCoalescingSource),
        Box::new(frontier::DispatchQueueTimingSource),
        Box::new(frontier::NLInferenceTimingSource),
        // Frontier: covert-channel level sources — ICC, DVFS boost, AES pipeline (2026-02-24)
        Box::new(frontier::ICCAtomicContentionSource),
        // Frontier: Apple APRR undocumented register JIT toggle (2026-02-24)
        Box::new(frontier::APRRJitTimingSource),
        // Frontier: instruction-level entropy — ISB pipeline, preemption, SEV broadcast (2026-02-24)
        Box::new(frontier::PreemptionBoundarySource),
        Box::new(frontier::SEVEventTimingSource),
        // Frontier: crypto + exclusive monitor (2026-02-24)
        // Frontier: PAC unit, COMMPAGE seqlock, physical timer (2026-02-24)
        Box::new(frontier::CommPageClockTimingSource),
        // Frontier: PMULL sparse event detector, cross-core coherency, DC CIVAC (2026-02-24)
        // Frontier: SMC thermistor/fuel-gauge outliers, proc_lock contention (2026-02-24)
        Box::new(frontier::SMCHighVarTimingSource),
        Box::new(frontier::ProcInfoTimingSource),
        // Frontier: SEP TRNG reseed timing via getentropy (2026-02-24)
        Box::new(frontier::GetentropyTimingSource),
        // Frontier: hardware prefetcher state, USB enumeration (2026-02-24)
        Box::new(frontier::PrefetcherStateSource),
        Box::new(frontier::USBEnumerationSource),
        // Frontier: JIT register timing, CNTFRQ cache, CommonCrypto AES (2026-02-24)
        Box::new(frontier::GxfRegisterTimingSource),
        Box::new(frontier::CntfrqCacheTimingSource),
        Box::new(frontier::CommonCryptoAesTimingSource),
        // Frontier: dual clock domain beat, SITVA (2026-02-24)
        Box::new(frontier::DualClockDomainSource),
        Box::new(frontier::SITVASource),
    ]
}
