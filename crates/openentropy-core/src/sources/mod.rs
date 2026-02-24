//! All 51 entropy source implementations.

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
        Box::new(frontier::AudioPLLTimingSource),
        Box::new(frontier::USBTimingSource),
        // Frontier: unprecedented entropy sources (2026-02-14)
        Box::new(frontier::NVMeLatencySource),
        Box::new(frontier::RNDRTrapTimingSource),
        Box::new(frontier::MachContinuousTimingSource),
        Box::new(frontier::GPUDivergenceSource),
        Box::new(frontier::IOSurfaceCrossingSource),
        Box::new(frontier::FsyncJournalSource),
        // Frontier: two-oscillator beat frequency (CPU counter vs audio PLL)
        Box::new(frontier::CounterBeatSource),
        // Frontier: independent oscillator/PLL sources (2026-02-15)
        Box::new(frontier::DisplayPllSource),
        Box::new(frontier::PciePllSource),
        // Frontier: deep hardware sources (2026-02-22)
        Box::new(frontier::SEPTimingSource),
        Box::new(frontier::PECoreArithmeticSource),
        Box::new(frontier::MemoryBusCryptoSource),
        Box::new(frontier::LPDDR5RowConflictSource),
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
    ]
}
