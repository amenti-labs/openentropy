//! AMX coprocessor timing — entropy from the Apple Matrix eXtensions unit.

#![cfg(target_os = "macos")]

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

use super::extract_timing_entropy_debiased;

/// Configuration for AMX timing entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::AMXTimingConfig;
/// // Use defaults (recommended)
/// let config = AMXTimingConfig::default();
///
/// // Or customize
/// let config = AMXTimingConfig {
///     matrix_sizes: vec![32, 128],       // only two sizes
///     interleave_memory_ops: true,
///     von_neumann_debias: true,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct AMXTimingConfig {
    /// Matrix dimensions to cycle through for SGEMM dispatches.
    ///
    /// Different sizes stress different AMX pipeline configurations:
    /// - Small (16-32): register-bound, fast dispatch
    /// - Medium (48-64): L1-cache-bound
    /// - Large (96-128): L2/SLC-bound, higher memory bandwidth pressure
    ///
    /// Must be non-empty. Each value is used as both M, N, and K dimensions.
    ///
    /// **Default:** `[16, 32, 48, 64, 96, 128]`
    pub matrix_sizes: Vec<usize>,

    /// Interleave volatile memory reads/writes between AMX dispatches.
    ///
    /// This thrashes a 64KB scratch buffer between matrix operations, disrupting
    /// the AMX pipeline state and preventing it from settling into a steady-state
    /// pattern. Increases min-entropy at the cost of slightly higher CPU usage.
    ///
    /// **Default:** `true`
    pub interleave_memory_ops: bool,

    /// Apply Von Neumann debiasing to raw timing deltas.
    ///
    /// The AMX timing source has severe bias.
    /// Von Neumann debiasing pairs consecutive deltas and discards equal pairs,
    /// emitting one unbiased bit per unequal pair. This costs ~50% of the raw
    /// data but dramatically improves min-entropy.
    ///
    /// **Default:** `true`
    pub von_neumann_debias: bool,
}

impl Default for AMXTimingConfig {
    fn default() -> Self {
        Self {
            matrix_sizes: vec![16, 32, 48, 64, 96, 128],
            interleave_memory_ops: true,
            von_neumann_debias: true,
        }
    }
}

/// Harvests timing jitter from the AMX (Apple Matrix eXtensions) coprocessor.
///
/// # What it measures
/// Nanosecond timing of SGEMM (single-precision matrix multiply) dispatches
/// to the AMX coprocessor via the Accelerate framework's `cblas_sgemm`.
///
/// # Why it's entropic
/// The AMX is a dedicated coprocessor on the Apple Silicon die with its own
/// register file, pipeline, and memory paths. Its timing depends on:
/// - Pipeline occupancy from ALL prior AMX operations (every process)
/// - Memory bandwidth contention on the unified memory controller
/// - Power state transitions (idle → active ramp-up latency)
/// - SLC (System Level Cache) eviction patterns
/// - Thermal throttling affecting AMX frequency independently of CPU cores
///
/// # What makes it unique
/// No prior work has used AMX coprocessor timing as an entropy source. The AMX
/// is a completely independent execution domain from CPU cores, providing
/// entropy that is uncorrelated with CPU-based timing sources.
///
/// # Configuration
/// See [`AMXTimingConfig`] for tunable parameters. Key options:
/// - `von_neumann_debias`: fixes severe LSB bias (recommended: `true`)
/// - `interleave_memory_ops`: disrupts pipeline steady-state
/// - `matrix_sizes`: controls which AMX pipeline configurations are exercised
#[derive(Default)]
pub struct AMXTimingSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
    pub config: AMXTimingConfig,
}

static AMX_TIMING_INFO: SourceInfo = SourceInfo {
    name: "amx_timing",
    description: "Apple AMX coprocessor matrix multiply timing jitter (debiased)",
    physics: "Dispatches matrix multiplications to the AMX (Apple Matrix eXtensions) \
              coprocessor via Accelerate BLAS and measures per-operation timing. The AMX is \
              a dedicated execution unit with its own pipeline, register file, and memory \
              paths. Timing depends on: AMX pipeline occupancy from ALL system AMX users, \
              memory bandwidth contention, AMX power state transitions, and SLC cache state. \
              Von Neumann debiasing corrects heavy LSB bias. Interleaved memory operations \
              disrupt pipeline steady-state for higher min-entropy.",
    category: SourceCategory::Microarch,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon],
    entropy_rate_estimate: 2500.0,
    composite: false,
};

impl EntropySource for AMXTimingSource {
    fn info(&self) -> &SourceInfo {
        &AMX_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(all(target_os = "macos", target_arch = "aarch64"))
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let debias = self.config.von_neumann_debias;
        let raw_count = if debias {
            n_samples * 8 + 128
        } else {
            n_samples * 4 + 64
        };
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        let sizes = &self.config.matrix_sizes;
        if sizes.is_empty() {
            return Vec::new();
        }
        let mut lcg: u64 = mach_time() | 1;

        let interleave = self.config.interleave_memory_ops;
        let mut scratch = if interleave {
            vec![0u8; 65536]
        } else {
            Vec::new()
        };

        // Pre-allocate matrices at the maximum size to avoid per-iteration allocation.
        let max_n = *sizes.iter().max().unwrap_or(&128);
        let max_len = max_n * max_n;
        let mut a = vec![0.0f32; max_len];
        let mut b = vec![0.0f32; max_len];
        let mut c = vec![0.0f32; max_len];

        for i in 0..raw_count {
            let n = sizes[i % sizes.len()];
            let len = n * n;

            for val in a[..len].iter_mut().chain(b[..len].iter_mut()) {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                *val = (lcg >> 32) as f32 / u32::MAX as f32;
            }

            if interleave && !scratch.is_empty() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                let idx = (lcg >> 32) as usize % scratch.len();
                unsafe {
                    let ptr = scratch.as_mut_ptr().add(idx);
                    std::ptr::write_volatile(ptr, std::ptr::read_volatile(ptr).wrapping_add(1));
                }
            }

            let t0 = mach_time();
            let trans_b = if i % 3 == 1 { 112 } else { 111 }; // CblasTrans vs CblasNoTrans

            // SAFETY: cblas_sgemm is a well-defined C function from the Accelerate
            // framework. On Apple Silicon, this dispatches to the AMX coprocessor.
            unsafe {
                cblas_sgemm(
                    101, // CblasRowMajor
                    111, // CblasNoTrans
                    trans_b,
                    n as i32,
                    n as i32,
                    n as i32,
                    1.0,
                    a.as_ptr(),
                    n as i32,
                    b.as_ptr(),
                    n as i32,
                    0.0,
                    c.as_mut_ptr(),
                    n as i32,
                );
            }

            let t1 = mach_time();
            std::hint::black_box(&c);
            timings.push(t1.wrapping_sub(t0));
        }

        if debias {
            extract_timing_entropy_debiased(&timings, n_samples)
        } else {
            extract_timing_entropy(&timings, n_samples)
        }
    }
}

// Accelerate framework CBLAS binding (Apple-provided, always available on macOS).
unsafe extern "C" {
    fn cblas_sgemm(
        order: i32,
        transa: i32,
        transb: i32,
        m: i32,
        n: i32,
        k: i32,
        alpha: f32,
        a: *const f32,
        lda: i32,
        b: *const f32,
        ldb: i32,
        beta: f32,
        c: *mut f32,
        ldc: i32,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = AMXTimingSource::default();
        assert_eq!(src.name(), "amx_timing");
        assert_eq!(src.info().category, SourceCategory::Microarch);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = AMXTimingConfig::default();
        assert_eq!(config.matrix_sizes, vec![16, 32, 48, 64, 96, 128]);
        assert!(config.interleave_memory_ops);
        assert!(config.von_neumann_debias);
    }

    #[test]
    fn custom_config() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                matrix_sizes: vec![32, 64],
                interleave_memory_ops: false,
                von_neumann_debias: false,
            },
        };
        assert_eq!(src.config.matrix_sizes.len(), 2);
        assert!(!src.config.interleave_memory_ops);
    }

    #[test]
    fn empty_sizes_returns_empty() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                matrix_sizes: vec![],
                interleave_memory_ops: false,
                von_neumann_debias: false,
            },
        };
        if src.is_available() {
            assert!(src.collect(64).is_empty());
        }
    }

    #[test]
    #[ignore] // Requires macOS aarch64
    fn collects_bytes() {
        let src = AMXTimingSource::default();
        if src.is_available() {
            let data = src.collect(128);
            assert!(!data.is_empty());
            assert!(data.len() <= 128);
        }
    }

    #[test]
    #[ignore] // Requires macOS aarch64
    fn no_debias_collects_bytes() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                von_neumann_debias: false,
                ..AMXTimingConfig::default()
            },
        };
        if src.is_available() {
            assert!(!src.collect(64).is_empty());
        }
    }
}
