//! # Frontier entropy sources
//!
//! Previously-unharvested nondeterminism from Apple Silicon hardware and
//! macOS/BSD kernel internals. These sources exploit entropy domains that no
//! prior work has tapped.
//!
//! ## Architecture
//!
//! ```text
//! frontier/
//! ├── mod.rs              ← you are here (re-exports + shared helpers)
//! ├── amx_timing.rs       ← AMX coprocessor matrix multiply jitter
//! ├── thread_lifecycle.rs ← pthread create/join scheduling jitter
//! ├── mach_ipc.rs         ← Mach port OOL message + VM remapping jitter
//! ├── tlb_shootdown.rs    ← mprotect-induced TLB invalidation IPI jitter
//! ├── pipe_buffer.rs      ← multi-pipe kernel zone allocator contention
//! ├── kqueue_events.rs    ← kqueue event multiplexing (timers + files + sockets)
//! ├── dvfs_race.rs        ← cross-core DVFS frequency race
//! ├── cas_contention.rs   ← CAS atomic contention timing
//! ├── keychain_timing.rs  ← Keychain/securityd round-trip timing
//! ├── counter_beat.rs     ← Two-oscillator beat frequency: CPU counter vs audio PLL
//! ├── display_pll.rs      ← Display PLL phase noise from pixel clock domain crossing
//! └── pcie_pll.rs         ← PCIe PHY PLL jitter from IOKit clock domain crossings
//! ```
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

// Standalone sources — one independent entropy domain each.
mod amx_timing;
mod audio_pll_timing;
mod cas_contention;
mod counter_beat;
mod denormal_timing;
mod display_pll;
mod dvfs_race;
mod fsync_journal;
mod gpu_divergence;
mod iosurface_crossing;
mod keychain_timing;
mod kqueue_events;
mod mach_ipc;
mod nvme_latency;
mod pcie_pll;
mod pdn_resonance;
mod pipe_buffer;
mod thread_lifecycle;
mod tlb_shootdown;
mod usb_timing;

// Re-export all source structs and their configs.
pub use amx_timing::{AMXTimingConfig, AMXTimingSource};
pub use audio_pll_timing::AudioPLLTimingSource;
pub use cas_contention::{CASContentionConfig, CASContentionSource};
pub use counter_beat::CounterBeatSource;
pub use denormal_timing::DenormalTimingSource;
pub use display_pll::DisplayPllSource;
pub use dvfs_race::DVFSRaceSource;
pub use fsync_journal::FsyncJournalSource;
pub use gpu_divergence::GPUDivergenceSource;
pub use iosurface_crossing::IOSurfaceCrossingSource;
pub use keychain_timing::{KeychainTimingConfig, KeychainTimingSource};
pub use kqueue_events::{KqueueEventsConfig, KqueueEventsSource};
pub use mach_ipc::{MachIPCConfig, MachIPCSource};
pub use nvme_latency::NVMeLatencySource;
pub use pcie_pll::PciePllSource;
pub use pdn_resonance::PDNResonanceSource;
pub use pipe_buffer::{PipeBufferConfig, PipeBufferSource};
pub use thread_lifecycle::ThreadLifecycleSource;
pub use tlb_shootdown::{TLBShootdownConfig, TLBShootdownSource};
pub use usb_timing::USBTimingSource;

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
            Box::new(CASContentionSource::default()),
            Box::new(KeychainTimingSource::default()),
            Box::new(DenormalTimingSource),
            Box::new(AudioPLLTimingSource),
            Box::new(USBTimingSource),
            Box::new(NVMeLatencySource),
            Box::new(GPUDivergenceSource),
            Box::new(PDNResonanceSource),
            Box::new(IOSurfaceCrossingSource),
            Box::new(FsyncJournalSource),
            Box::new(CounterBeatSource),
            Box::new(DisplayPllSource),
            Box::new(PciePllSource),
        ];
        for src in &sources {
            assert!(!src.name().is_empty());
            assert!(!src.info().description.is_empty());
            assert!(!src.info().physics.is_empty());
        }
    }
}
