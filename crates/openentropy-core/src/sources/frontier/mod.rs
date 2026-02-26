//! # Frontier entropy sources
//!
//! Previously-unharvested nondeterminism from Apple Silicon hardware and
//! macOS/BSD kernel internals. These sources exploit entropy domains that no
//! prior work has tapped.
//!
//! Each source measures a single, independent physical entropy domain.
//! They work in isolation and can be benchmarked independently. Source
//! combination is handled by the [`EntropyPool`](crate::pool::EntropyPool),
//! not by individual sources.
//!
//! ## Configuration
//!
//! Most sources accept a `*Config` struct with sensible defaults. Use
//! `Default::default()` for standard behavior, or construct a custom config
//! to tune for specific hardware or entropy requirements. See each source's
//! config struct documentation for field descriptions and valid ranges.

// Shared CoreAudio FFI bindings (used by audio_pll_timing).
#[cfg(target_os = "macos")]
mod coreaudio_ffi;

// Standalone sources — one independent entropy domain each.
mod amx_timing;
mod ane_timing;
mod aprr_jit_timing;
mod audio_pll_timing;
mod cntfrq_cache_timing;
mod commoncrypto_aes_timing;
mod commpage_clock_timing;
mod dispatch_queue_timing;
mod display_pll;
mod dual_clock_domain;
mod dvfs_race;
mod fsync_journal;
mod getentropy_timing;
mod gpu_divergence;
mod gxf_register_timing;
mod icc_atomic_contention;
mod iosurface_crossing;
mod keychain_timing;
mod kqueue_events;
mod mach_continuous_timing;
mod mach_ipc;
mod memory_bus_crypto;
mod nl_inference_timing;
mod nvme_iokit_sensors;
mod nvme_passthrough_linux;
mod nvme_raw_device;
mod pcie_pll;
mod pe_core_arithmetic;
mod pipe_buffer;
mod preemption_boundary;
mod prefetcher_state;
mod proc_info_timing;
mod sev_event_timing;
mod sitva;
mod smc_highvar_timing;
mod thread_lifecycle;
mod timer_coalescing;
mod tlb_shootdown;
mod usb_enumeration;

// Re-export all source structs and their configs.
pub use amx_timing::{AMXTimingConfig, AMXTimingSource};
pub use ane_timing::AneTimingSource;
pub use aprr_jit_timing::APRRJitTimingSource;
pub use audio_pll_timing::AudioPLLTimingSource;
pub use cntfrq_cache_timing::CntfrqCacheTimingSource;
pub use commoncrypto_aes_timing::CommonCryptoAesTimingSource;
pub use commpage_clock_timing::CommPageClockTimingSource;
pub use dispatch_queue_timing::DispatchQueueTimingSource;
pub use display_pll::DisplayPllSource;
pub use dual_clock_domain::DualClockDomainSource;
pub use dvfs_race::DVFSRaceSource;
pub use fsync_journal::FsyncJournalSource;
pub use getentropy_timing::GetentropyTimingSource;
pub use gpu_divergence::GPUDivergenceSource;
pub use gxf_register_timing::GxfRegisterTimingSource;
pub use icc_atomic_contention::ICCAtomicContentionSource;
pub use iosurface_crossing::IOSurfaceCrossingSource;
pub use keychain_timing::{KeychainTimingConfig, KeychainTimingSource};
pub use kqueue_events::{KqueueEventsConfig, KqueueEventsSource};
pub use mach_continuous_timing::MachContinuousTimingSource;
pub use mach_ipc::{MachIPCConfig, MachIPCSource};
pub use memory_bus_crypto::MemoryBusCryptoSource;
pub use nl_inference_timing::NLInferenceTimingSource;
pub use nvme_iokit_sensors::NvmeIokitSensorsSource;
pub use nvme_passthrough_linux::NvmePassthroughLinuxSource;
pub use nvme_raw_device::NvmeRawDeviceSource;
pub use pcie_pll::PciePllSource;
pub use pe_core_arithmetic::PECoreArithmeticSource;
pub use pipe_buffer::{PipeBufferConfig, PipeBufferSource};
pub use preemption_boundary::PreemptionBoundarySource;
pub use prefetcher_state::PrefetcherStateSource;
pub use proc_info_timing::ProcInfoTimingSource;
pub use sev_event_timing::SEVEventTimingSource;
pub use sitva::SITVASource;
pub use smc_highvar_timing::SMCHighVarTimingSource;
pub use thread_lifecycle::ThreadLifecycleSource;
pub use timer_coalescing::TimerCoalescingSource;
pub use tlb_shootdown::{TLBShootdownConfig, TLBShootdownSource};
pub use usb_enumeration::USBEnumerationSource;

// ---------------------------------------------------------------------------
// Shared extraction helpers (used by multiple frontier sources)
// ---------------------------------------------------------------------------

use super::helpers::xor_fold_u64;

/// Von Neumann debiased timing extraction.
///
/// Takes pairs of consecutive timing deltas. If they differ, emit one bit
/// based on their relative order (first < second → 1, else → 0). This
/// removes bias from the raw timing stream at the cost of ~50% data loss.
///
/// Used by [`AMXTimingSource`] to correct its severe min-entropy bias.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn extract_timing_entropy_debiased(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 4 {
        return Vec::new();
    }

    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // Von Neumann debias: take pairs, discard equal, emit comparison bit.
    let mut debiased_bits: Vec<u8> = Vec::with_capacity(deltas.len() / 2);
    for pair in deltas.chunks_exact(2) {
        if pair[0] != pair[1] {
            debiased_bits.push(if pair[0] < pair[1] { 1 } else { 0 });
        }
    }

    // Pack bits into bytes (only full bytes).
    let mut bytes = Vec::with_capacity(n_samples);
    for chunk in debiased_bits.chunks(8) {
        if chunk.len() < 8 {
            break;
        }
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
        if bytes.len() >= n_samples {
            break;
        }
    }
    bytes.truncate(n_samples);
    bytes
}

/// Extract entropy from timing variance (delta-of-deltas).
///
/// Computes first-order deltas, then second-order deltas (capturing the
/// *change* in timing). This removes systematic bias and amplifies the
/// nondeterministic component.
///
/// Used by [`TLBShootdownSource`] in variance mode.
pub(crate) fn extract_timing_entropy_variance(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 4 {
        return Vec::new();
    }

    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    let variance: Vec<u64> = deltas.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();

    let xored: Vec<u64> = variance.windows(2).map(|w| w[0] ^ w[1]).collect();

    let mut raw: Vec<u8> = xored.iter().map(|&x| xor_fold_u64(x)).collect();
    raw.truncate(n_samples);
    raw
}

// ---------------------------------------------------------------------------
// Tests for shared helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Von Neumann debiasing
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn debiased_extraction_basic() {
        let timings: Vec<u64> = (0..200).map(|i| 100 + (i * 7 + i * i) % 50).collect();
        let result = extract_timing_entropy_debiased(&timings, 10);
        assert!(result.len() <= 10);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn debiased_extraction_too_few() {
        assert!(extract_timing_entropy_debiased(&[1, 2, 3], 10).is_empty());
        assert!(extract_timing_entropy_debiased(&[], 10).is_empty());
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn debiased_extraction_constant_input() {
        let timings = vec![42u64; 100];
        let result = extract_timing_entropy_debiased(&timings, 10);
        assert!(result.is_empty());
    }

    // Variance extraction
    #[test]
    fn variance_extraction_basic() {
        let timings: Vec<u64> = (0..100).map(|i| 100 + (i * 7 + i * i) % 50).collect();
        let result = extract_timing_entropy_variance(&timings, 10);
        assert!(!result.is_empty());
        assert!(result.len() <= 10);
    }

    #[test]
    fn variance_extraction_too_few() {
        assert!(extract_timing_entropy_variance(&[1, 2, 3], 10).is_empty());
    }

    // All frontier sources have valid metadata
    #[test]
    fn all_frontier_sources_have_valid_names() {
        let sources: Vec<Box<dyn crate::source::EntropySource>> = vec![
            Box::new(AMXTimingSource::default()),
            Box::new(ThreadLifecycleSource),
            Box::new(MachIPCSource::default()),
            Box::new(TLBShootdownSource::default()),
            Box::new(PipeBufferSource::default()),
            Box::new(KqueueEventsSource::default()),
            Box::new(DVFSRaceSource),
            Box::new(KeychainTimingSource::default()),
            Box::new(AudioPLLTimingSource),
            Box::new(MachContinuousTimingSource),
            Box::new(GPUDivergenceSource),
            Box::new(IOSurfaceCrossingSource),
            Box::new(FsyncJournalSource),
            Box::new(DisplayPllSource),
            Box::new(PciePllSource),
            Box::new(PECoreArithmeticSource),
            Box::new(MemoryBusCryptoSource),
            Box::new(TimerCoalescingSource),
            Box::new(DispatchQueueTimingSource),
            Box::new(NLInferenceTimingSource),
            Box::new(ICCAtomicContentionSource),
            Box::new(APRRJitTimingSource),
            Box::new(PreemptionBoundarySource),
            Box::new(SEVEventTimingSource),
            Box::new(CommPageClockTimingSource),
            Box::new(SMCHighVarTimingSource),
            Box::new(ProcInfoTimingSource),
            Box::new(GetentropyTimingSource),
            Box::new(PrefetcherStateSource),
            Box::new(USBEnumerationSource),
            Box::new(CntfrqCacheTimingSource),
            Box::new(GxfRegisterTimingSource),
            Box::new(CommonCryptoAesTimingSource),
            Box::new(DualClockDomainSource),
            Box::new(SITVASource),
            Box::new(AneTimingSource),
            Box::new(NvmeIokitSensorsSource),
            Box::new(NvmeRawDeviceSource),
            Box::new(NvmePassthroughLinuxSource),
        ];
        for src in &sources {
            assert!(!src.name().is_empty());
            assert!(!src.info().description.is_empty());
            assert!(!src.info().physics.is_empty());
        }
    }
}
