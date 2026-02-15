//! Accelerate framework inference jitter — AMX/NEON coprocessor timing entropy.
//!
//! Apple's Accelerate framework dispatches BLAS/vDSP operations to the AMX
//! coprocessor or NEON SIMD units. Timing each matrix multiply captures jitter
//! from coprocessor scheduling, memory controller arbitration, and DVFS.
//!
//! This is distinct from `amx_timing` which uses raw AMX instructions — here
//! we go through the Accelerate framework's dispatch layer, adding another
//! source of nondeterminism.
//!
//! PoC measured H∞ ≈ 2.5 bits/byte for BLAS sgemm timing.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

/// Matrix size for BLAS operations.
const MATRIX_SIZE: usize = 64;

/// Operations per timing sample.
const OPS_PER_SAMPLE: usize = 4;

static ACCELERATE_JITTER_INFO: SourceInfo = SourceInfo {
    name: "accelerate_jitter",
    description: "Accelerate framework BLAS/vDSP inference timing jitter",
    physics: "Times BLAS matrix multiplications dispatched through Apple\u{2019}s Accelerate \
              framework. Each operation may be routed to AMX coprocessor or NEON SIMD units, \
              with timing jitter from: coprocessor dispatch scheduling, unified memory \
              controller arbitration between CPU/GPU/ANE, DVFS frequency transitions, and \
              cache hierarchy contention. The framework\u{2019}s runtime dispatch adds another \
              layer of nondeterminism beyond raw instruction timing. \
              PoC measured H\u{221e} \u{2248} 2.5 bits/byte.",
    category: SourceCategory::Frontier,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 800.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from Accelerate framework operations.
pub struct AccelerateJitterSource;

/// Accelerate framework FFI (macOS only).
#[cfg(target_os = "macos")]
mod accelerate {
    // CBLAS row-major, no-transpose constants.
    pub const CBLAS_ROW_MAJOR: i32 = 101;
    pub const CBLAS_NO_TRANS: i32 = 111;

    #[link(name = "Accelerate", kind = "framework")]
    unsafe extern "C" {
        pub fn cblas_sgemm(
            order: i32,
            trans_a: i32,
            trans_b: i32,
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
}

impl EntropySource for AccelerateJitterSource {
    fn info(&self) -> &SourceInfo {
        &ACCELERATE_JITTER_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = n_samples;
            return Vec::new();
        }

        #[cfg(target_os = "macos")]
        {
            let m = MATRIX_SIZE as i32;

            // Initialize matrices with deterministic but non-trivial values.
            let a: Vec<f32> = (0..MATRIX_SIZE * MATRIX_SIZE)
                .map(|i| (i as f32 * 0.01).sin())
                .collect();
            let b: Vec<f32> = (0..MATRIX_SIZE * MATRIX_SIZE)
                .map(|i| (i as f32 * 0.01).cos())
                .collect();
            let mut c = vec![0.0f32; MATRIX_SIZE * MATRIX_SIZE];

            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            for _ in 0..raw_count {
                c.fill(0.0);
                let t0 = std::time::Instant::now();
                for _ in 0..OPS_PER_SAMPLE {
                    // SAFETY: cblas_sgemm is a stable Accelerate framework API.
                    // We pass valid pointers to properly-sized arrays with correct
                    // dimensions (m×m matrices, row-major layout).
                    unsafe {
                        accelerate::cblas_sgemm(
                            accelerate::CBLAS_ROW_MAJOR,
                            accelerate::CBLAS_NO_TRANS,
                            accelerate::CBLAS_NO_TRANS,
                            m,
                            m,
                            m,
                            1.0,
                            a.as_ptr(),
                            m,
                            b.as_ptr(),
                            m,
                            0.0,
                            c.as_mut_ptr(),
                            m,
                        );
                    }
                }
                let elapsed = t0.elapsed();
                std::hint::black_box(&c);
                timings.push(elapsed.as_nanos() as u64);
            }

            extract_timing_entropy(&timings, n_samples)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = AccelerateJitterSource;
        assert_eq!(src.name(), "accelerate_jitter");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Hardware-dependent
    fn collects_bytes() {
        let src = AccelerateJitterSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
