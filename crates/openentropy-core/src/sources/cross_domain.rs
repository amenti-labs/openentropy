//! Cross-domain beat frequency entropy sources.
//!
//! These sources measure timing across independent clock domains (CPU, I/O,
//! memory, kernel).  The beat frequency between PLLs driving each domain
//! creates timing jitter that serves as entropy.

use std::io::Write;

use tempfile::NamedTempFile;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

use super::helpers::mach_time;

// ---------------------------------------------------------------------------
// CPUIOBeatSource
// ---------------------------------------------------------------------------

static CPU_IO_BEAT_INFO: SourceInfo = SourceInfo {
    name: "cpu_io_beat",
    description: "Cross-domain beat frequency between CPU computation and disk I/O timing",
    physics: "Alternates CPU-bound computation with disk I/O operations and measures the \
              transition timing. The CPU and I/O subsystem run on independent clock domains \
              with separate PLLs. When operations cross domains, the beat frequency of their \
              PLLs creates timing jitter. This is analogous to the acoustic beat frequency \
              between two tuning forks.",
    category: SourceCategory::CrossDomain,
    platform_requirements: &[],
    entropy_rate_estimate: 1500.0,
};

/// Entropy source that captures beat frequency between CPU and I/O clock domains.
pub struct CPUIOBeatSource;

impl EntropySource for CPUIOBeatSource {
    fn info(&self) -> &SourceInfo {
        &CPU_IO_BEAT_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut tmpfile = match NamedTempFile::new() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        // Over-collect raw timings: we need 8 bits per byte, and XOR/LSB
        // extraction reduces the count.
        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        for i in 0..raw_count {
            let t0 = mach_time();

            // CPU-bound computation: 50 iterations of LCG
            let mut x: u64 = t0;
            for _ in 0..50 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(x);

            let t1 = mach_time();

            // Disk I/O: write to temp file
            let buf = [i as u8; 64];
            let _ = tmpfile.write_all(&buf);
            if i % 16 == 0 {
                let _ = tmpfile.flush();
            }

            let t2 = mach_time();

            // Record the domain-crossing latencies.
            timings.push(t1.wrapping_sub(t0)); // CPU domain
            timings.push(t2.wrapping_sub(t1)); // I/O domain
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // XOR consecutive deltas.
        let xor_deltas: Vec<u64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract LSBs.
        let mut entropy: Vec<u8> = xor_deltas.iter().map(|&d| d as u8).collect();

        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// CPUMemoryBeatSource
// ---------------------------------------------------------------------------

/// Size of the memory buffer: 16 MB to exceed L2 cache and force DRAM access.
const MEM_BUFFER_SIZE: usize = 16 * 1024 * 1024;

static CPU_MEMORY_BEAT_INFO: SourceInfo = SourceInfo {
    name: "cpu_memory_beat",
    description: "Cross-domain beat frequency between CPU computation and random memory access timing",
    physics: "Interleaves CPU computation with random memory accesses to large arrays \
              (>L2 cache). The memory controller runs on its own clock domain. Cache misses \
              force the CPU to wait for the memory controller\u{2019}s arbitration, whose timing \
              depends on: DRAM refresh state, competing DMA from GPU/ANE, and row buffer \
              conflicts.",
    category: SourceCategory::CrossDomain,
    platform_requirements: &[],
    entropy_rate_estimate: 2500.0,
};

/// Entropy source that captures beat frequency between CPU and memory controller
/// clock domains.
pub struct CPUMemoryBeatSource;

impl EntropySource for CPUMemoryBeatSource {
    fn info(&self) -> &SourceInfo {
        &CPU_MEMORY_BEAT_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Allocate a 16 MB buffer to force DRAM access (exceeds L2 cache).
        let mut buffer = vec![0u8; MEM_BUFFER_SIZE];

        // Initialize with a simple pattern so the pages are faulted in.
        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = i as u8;
        }

        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Use an LCG to generate pseudo-random indices into the buffer.
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            let t0 = mach_time();

            // CPU-bound computation: 50 iterations of LCG
            let mut x: u64 = t0;
            for _ in 0..50 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(x);

            let t1 = mach_time();

            // Random memory access (likely cache miss for large buffer).
            lcg = lcg
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = (lcg as usize) % MEM_BUFFER_SIZE;
            let val = unsafe { std::ptr::read_volatile(&buffer[idx]) };
            std::hint::black_box(val);

            let t2 = mach_time();

            timings.push(t1.wrapping_sub(t0)); // CPU domain
            timings.push(t2.wrapping_sub(t1)); // Memory domain
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // XOR consecutive deltas.
        let xor_deltas: Vec<u64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract LSBs.
        let mut entropy: Vec<u8> = xor_deltas.iter().map(|&d| d as u8).collect();

        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// MultiDomainBeatSource
// ---------------------------------------------------------------------------

/// Size of the memory buffer for multi-domain source: 4 MB.
const MULTI_BUFFER_SIZE: usize = 4 * 1024 * 1024;

static MULTI_DOMAIN_BEAT_INFO: SourceInfo = SourceInfo {
    name: "multi_domain_beat",
    description: "Composite beat frequency across CPU, memory, disk I/O, and kernel syscall clock domains",
    physics: "Rapidly interleaves operations across 4 clock domains: CPU computation, memory \
              access, disk I/O, and kernel syscalls. Each domain has its own PLL and \
              arbitration logic. The composite timing captures interference patterns \
              between all domains simultaneously.",
    category: SourceCategory::CrossDomain,
    platform_requirements: &[],
    entropy_rate_estimate: 3000.0,
};

/// Entropy source that captures beat frequency across CPU, memory, I/O, and
/// kernel syscall clock domains simultaneously.
pub struct MultiDomainBeatSource;

impl EntropySource for MultiDomainBeatSource {
    fn info(&self) -> &SourceInfo {
        &MULTI_DOMAIN_BEAT_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Allocate a 4 MB buffer for memory accesses.
        let mut buffer = vec![0u8; MULTI_BUFFER_SIZE];
        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = i as u8;
        }

        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count * 4);

        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            // Domain 1: CPU computation (XOR operations)
            let t0 = mach_time();
            let mut x: u64 = t0;
            for _ in 0..50 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(x);
            let t1 = mach_time();

            // Domain 2: Random memory access
            lcg = lcg
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = (lcg as usize) % MULTI_BUFFER_SIZE;
            let val = unsafe { std::ptr::read_volatile(&buffer[idx]) };
            std::hint::black_box(val);
            let t2 = mach_time();

            // Domain 3: Kernel syscall (getpid)
            unsafe { libc::getpid() };
            let t3 = mach_time();

            // Record all domain crossing timings.
            timings.push(t1.wrapping_sub(t0)); // CPU
            timings.push(t2.wrapping_sub(t1)); // Memory
            timings.push(t3.wrapping_sub(t2)); // Syscall
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // XOR consecutive deltas.
        let xor_deltas: Vec<u64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract LSBs.
        let mut entropy: Vec<u8> = xor_deltas.iter().map(|&d| d as u8).collect();

        entropy.truncate(n_samples);
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::super::helpers::extract_lsbs_u64;
    use super::*;

    #[test]
    fn cpu_io_beat_info() {
        let src = CPUIOBeatSource;
        assert_eq!(src.name(), "cpu_io_beat");
        assert_eq!(src.info().category, SourceCategory::CrossDomain);
        assert!((src.info().entropy_rate_estimate - 1500.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn cpu_io_beat_collects_bytes() {
        let src = CPUIOBeatSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn cpu_memory_beat_info() {
        let src = CPUMemoryBeatSource;
        assert_eq!(src.name(), "cpu_memory_beat");
        assert_eq!(src.info().category, SourceCategory::CrossDomain);
        assert!((src.info().entropy_rate_estimate - 2500.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn cpu_memory_beat_collects_bytes() {
        let src = CPUMemoryBeatSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn multi_domain_beat_info() {
        let src = MultiDomainBeatSource;
        assert_eq!(src.name(), "multi_domain_beat");
        assert_eq!(src.info().category, SourceCategory::CrossDomain);
        assert!((src.info().entropy_rate_estimate - 3000.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn multi_domain_beat_collects_bytes() {
        let src = MultiDomainBeatSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    fn extract_lsbs_basic() {
        let deltas = vec![1u64, 2, 3, 4, 5, 6, 7, 8];
        let bytes = extract_lsbs_u64(&deltas);
        // Bits: 1,0,1,0,1,0,1,0 -> 0xAA
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0xAA);
    }
}
