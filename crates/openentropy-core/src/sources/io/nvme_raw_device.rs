//! NVMe raw block device reads — bypass filesystem for closer NAND timing.
//!
//! Reads directly from `/dev/rdiskN` (macOS) or `/dev/nvmeXnYpZ` (Linux) using
//! `libc` FFI with page-aligned buffers and `F_NOCACHE` (macOS) / `O_DIRECT`
//! (Linux) to bypass the OS buffer cache. This removes the filesystem and buffer
//! cache layers from the timing path, getting closer to NVMe controller + NAND
//! flash timing.
//!
//! ## Entropy mechanism
//!
//! - **NVMe controller timing**: Command submission, FTL lookup, wear leveling
//! - **NAND page read latency**: Charge sensing through quantum tunneling
//!   (Fowler-Nordheim for writes, threshold voltage sensing for reads)
//! - **Cross-die variation**: Reads at widely-spaced offsets (1 MB apart) hit
//!   different NAND dies/planes with independent timing characteristics
//!
//! ## Entropy quality
//!
//! With the filesystem layer removed, a larger fraction of the measured timing
//! comes from NVMe controller firmware (FTL, GC, wear leveling) and NAND access
//! latency. The NAND charge sensing physics has quantum-mechanical underpinnings
//! (electron tunneling), but the dominant variance is classical firmware timing.
//! There is also no guarantee that reads are served from NAND rather than the
//! controller's DRAM cache.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static NVME_RAW_DEVICE_INFO: SourceInfo = SourceInfo {
    name: "nvme_raw_device",
    description: "Direct raw block device reads bypassing filesystem with page-aligned I/O",
    physics: "Reads directly from raw block devices (/dev/rdiskN on macOS, /dev/nvmeXnYpZ on \
              Linux) with page-aligned buffers and cache bypass (F_NOCACHE/O_DIRECT). This \
              eliminates the filesystem, buffer cache, and VFS layers from the timing path. \
              The remaining timing variance comes from NVMe controller firmware (FTL lookup, \
              wear leveling, garbage collection) and NAND flash page read latency. NAND reads \
              involve charge sensing where threshold voltage depends on trapped electron count \
              from Fowler-Nordheim tunneling. Note: the dominant timing variance is classical \
              (firmware scheduling, DRAM cache hits) — the quantum-mechanical contribution \
              from charge sensing cannot be isolated without specialized metrology equipment.",
    category: SourceCategory::IO,
    platform: Platform::Any,
    requirements: &[Requirement::RawBlockDevice],
    entropy_rate_estimate: 2000.0,
    composite: false,
    is_fast: true,
};

/// Number of widely-spaced offsets to cycle through (hit different NAND dies).
const N_OFFSETS: usize = 8;
/// Block size for aligned reads.
const BLOCK_SIZE: usize = 4096;
/// Spacing between offsets to hit different NAND dies/planes.
const OFFSET_STRIDE: u64 = 1024 * 1024; // 1 MB

/// NVMe raw block device entropy source.
pub struct NvmeRawDeviceSource;

/// Try to find and open a readable raw block device.
/// Returns the fd on success.
fn try_open_raw_device() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        let devices = ["/dev/rdisk0", "/dev/rdisk1", "/dev/rdisk2"];
        for dev in &devices {
            let c_path = match std::ffi::CString::new(*dev) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // SAFETY: open() with O_RDONLY on a device path. May fail with EACCES
            // if not root, which is handled by checking the return value.
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
            if fd >= 0 {
                // Disable buffer cache (macOS-specific).
                // SAFETY: fcntl F_NOCACHE is a valid operation on an open fd.
                unsafe { libc::fcntl(fd, libc::F_NOCACHE, 1) };
                return Some(fd);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let devices = ["/dev/nvme0n1", "/dev/nvme1n1", "/dev/sda", "/dev/sdb"];
        for dev in &devices {
            let c_path = match std::ffi::CString::new(*dev) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // SAFETY: open() with O_RDONLY | O_DIRECT on a device path.
            // O_DIRECT bypasses the page cache on Linux.
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECT) };
            if fd >= 0 {
                return Some(fd);
            }
        }
    }

    None
}

/// Check if raw block device access is available (without keeping the fd).
fn can_open_raw_device() -> bool {
    if let Some(fd) = try_open_raw_device() {
        // SAFETY: close() on a valid fd.
        unsafe { libc::close(fd) };
        true
    } else {
        false
    }
}

/// Perform timed reads on a raw block device fd.
/// Returns a vec of timing values (CNTVCT ticks on macOS, nanos on Linux).
fn timed_raw_reads(fd: i32, count: usize) -> Vec<u64> {
    use crate::sources::helpers::read_cntvct;

    // Allocate a page-aligned buffer.
    let mut aligned_buf: *mut libc::c_void = std::ptr::null_mut();
    // SAFETY: posix_memalign allocates an aligned buffer. We check the return value.
    let ret = unsafe { libc::posix_memalign(&mut aligned_buf, BLOCK_SIZE, BLOCK_SIZE) };
    if ret != 0 || aligned_buf.is_null() {
        return Vec::new();
    }

    // Pre-compute offsets (widely spaced to hit different NAND dies/planes).
    let offsets: Vec<i64> = (0..N_OFFSETS)
        .map(|i| (i as u64 * OFFSET_STRIDE) as i64)
        .collect();

    let mut timings = Vec::with_capacity(count);

    for i in 0..count {
        let offset = offsets[i % N_OFFSETS];

        // Seek to the target offset.
        // SAFETY: lseek on a valid fd with a valid offset.
        let seek_result = unsafe { libc::lseek(fd, offset, libc::SEEK_SET) };
        if seek_result < 0 {
            // If seek fails (offset beyond device), wrap around to offset 0.
            unsafe { libc::lseek(fd, 0, libc::SEEK_SET) };
        }

        let t_before = read_cntvct();

        // SAFETY: read() into a valid aligned buffer of BLOCK_SIZE.
        let _bytes_read = unsafe { libc::read(fd, aligned_buf, BLOCK_SIZE) };

        let t_after = read_cntvct();
        timings.push(t_after.wrapping_sub(t_before));
    }

    // SAFETY: free() on a buffer allocated by posix_memalign.
    unsafe { libc::free(aligned_buf) };

    timings
}

impl EntropySource for NvmeRawDeviceSource {
    fn info(&self) -> &SourceInfo {
        &NVME_RAW_DEVICE_INFO
    }

    fn is_available(&self) -> bool {
        can_open_raw_device()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let fd = match try_open_raw_device() {
            Some(fd) => fd,
            None => return Vec::new(),
        };

        // Over-sample: ~4x raw readings per output byte.
        let raw_count = n_samples * 4 + 64;
        let timings = timed_raw_reads(fd, raw_count);

        // SAFETY: close() on a valid fd.
        unsafe { libc::close(fd) };

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = NvmeRawDeviceSource;
        assert_eq!(src.name(), "nvme_raw_device");
        assert_eq!(src.info().category, SourceCategory::IO);
        assert!(!src.info().composite);
        assert_eq!(src.info().platform, Platform::Any);
    }

    #[test]
    fn physics_mentions_nand() {
        let src = NvmeRawDeviceSource;
        assert!(src.info().physics.contains("NAND"));
        assert!(src.info().physics.contains("Fowler-Nordheim"));
        assert!(src.info().physics.contains("raw block"));
    }

    #[test]
    #[ignore] // Requires root or disk group membership for raw device access
    fn collects_bytes() {
        let src = NvmeRawDeviceSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
