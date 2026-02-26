mod amx_timing;
mod aprr_jit_timing;
mod cas_contention;
mod cntfrq_cache_timing;
mod commoncrypto_aes_timing;
mod denormal_timing;
mod dual_clock_domain;
mod dvfs_race;
mod gxf_register_timing;
mod icc_atomic_contention;
mod memory_bus_crypto;
mod prefetcher_state;
mod sev_event_timing;
mod sitva;
mod speculative_execution;
mod tlb_shootdown;

pub use amx_timing::{AMXTimingConfig, AMXTimingSource};
pub use aprr_jit_timing::APRRJitTimingSource;
pub use cas_contention::CASContentionSource;
pub use cntfrq_cache_timing::CntfrqCacheTimingSource;
pub use commoncrypto_aes_timing::CommonCryptoAesTimingSource;
pub use denormal_timing::DenormalTimingSource;
pub use dual_clock_domain::DualClockDomainSource;
pub use dvfs_race::DVFSRaceSource;
pub use gxf_register_timing::GxfRegisterTimingSource;
pub use icc_atomic_contention::ICCAtomicContentionSource;
pub use memory_bus_crypto::MemoryBusCryptoSource;
pub use prefetcher_state::PrefetcherStateSource;
pub use sev_event_timing::SEVEventTimingSource;
pub use sitva::SITVASource;
pub use speculative_execution::SpeculativeExecutionSource;
pub use tlb_shootdown::{TLBShootdownConfig, TLBShootdownSource};

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(AMXTimingSource::default()),
        Box::new(APRRJitTimingSource),
        Box::new(CntfrqCacheTimingSource),
        Box::new(CommonCryptoAesTimingSource),
        Box::new(DualClockDomainSource),
        Box::new(DVFSRaceSource),
        Box::new(GxfRegisterTimingSource),
        Box::new(ICCAtomicContentionSource),
        Box::new(MemoryBusCryptoSource),
        Box::new(PrefetcherStateSource),
        Box::new(SEVEventTimingSource),
        Box::new(SITVASource),
        Box::new(SpeculativeExecutionSource),
        Box::new(TLBShootdownSource::default()),
    ]
}
