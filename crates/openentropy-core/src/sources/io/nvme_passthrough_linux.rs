//! NVMe admin passthrough — raw NVMe commands on Linux via ioctl.
//!
//! Submits raw NVMe admin commands (Get Log Page for SMART/Health) via
//! `ioctl(NVME_IOCTL_ADMIN_CMD)` on `/dev/nvme0`. This bypasses the filesystem,
//! block layer, and I/O scheduler entirely — the timing path is:
//! userspace → NVMe kernel driver → NVMe controller → NAND.
//!
//! ## Entropy mechanism
//!
//! - **NVMe command round-trip timing**: Minimal host overhead, dominated by
//!   controller firmware processing and NAND access
//! - **SMART temperature ADC noise**: On-die temperature sensor quantization noise
//! - **Controller internal state**: FTL state, GC scheduling, wear leveling
//!   all affect command latency nondeterministically
//!
//! ## Entropy quality
//!
//! This is the closest to NVMe hardware achievable from userspace on Linux.
//! The filesystem and block layers are completely eliminated. The dominant
//! timing variance comes from NVMe driver submission/completion overhead (~4us)
//! and controller firmware processing (FTL, GC). The NAND charge sensing
//! physics has quantum-mechanical underpinnings, but quantifying the fraction
//! of timing variance attributable to quantum effects is not possible without
//! specialized metrology equipment.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "linux")]
use crate::sources::helpers::extract_timing_entropy;

static NVME_PASSTHROUGH_INFO: SourceInfo = SourceInfo {
    name: "nvme_passthrough_linux",
    description: "Raw NVMe admin commands via ioctl passthrough on Linux (closest to NAND hardware)",
    physics: "Submits NVMe admin commands (Get Log Page for SMART/Health Information, Log ID 02h) \
              via ioctl(NVME_IOCTL_ADMIN_CMD) on /dev/nvme0. This bypasses the filesystem, block \
              layer, and I/O scheduler entirely. The timing path is: userspace \u{2192} NVMe kernel \
              driver \u{2192} NVMe controller \u{2192} NAND flash. Command round-trip timing is \
              dominated by NVMe controller firmware processing (FTL lookup, wear leveling, garbage \
              collection scheduling) and NAND flash page access. NAND charge sensing has quantum-\
              mechanical underpinnings (Fowler-Nordheim tunneling), but the dominant timing variance \
              is classical (driver overhead, firmware scheduling). SMART temperature values provide \
              additional ADC quantization noise.",
    category: SourceCategory::IO,
    platform: Platform::Linux,
    requirements: &[Requirement::RawBlockDevice],
    entropy_rate_estimate: 2500.0,
    composite: false,
    is_fast: true,
};

/// NVMe admin passthrough entropy source (Linux only).
pub struct NvmePassthroughLinuxSource;

/// NVMe passthrough implementation for Linux.
#[cfg(target_os = "linux")]
mod passthrough {
    use std::time::Instant;

    /// NVMe passthrough command struct matching `struct nvme_passthru_cmd`
    /// from `linux/nvme_ioctl.h`.
    #[repr(C)]
    #[derive(Default)]
    struct NvmePassthruCmd {
        opcode: u8,
        flags: u8,
        rsvd1: u16,
        nsid: u32,
        cdw2: u32,
        cdw3: u32,
        metadata: u64,
        addr: u64,
        metadata_len: u32,
        data_len: u32,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
        cdw13: u32,
        cdw14: u32,
        cdw15: u32,
        timeout_ms: u32,
        result: u32,
    }

    /// NVME_IOCTL_ADMIN_CMD = _IOWR('N', 0x41, struct nvme_passthru_cmd)
    /// On Linux: direction = _IOWR = 0xC0000000, size = sizeof(nvme_passthru_cmd) = 72 = 0x48
    /// type = 'N' = 0x4E, nr = 0x41
    /// ioctl number = 0xC0484E41
    const NVME_IOCTL_ADMIN_CMD: libc::c_ulong = 0xC048_4E41;

    /// NVMe Admin command opcode: Get Log Page
    const NVME_ADMIN_GET_LOG_PAGE: u8 = 0x02;
    /// SMART / Health Information log (Log ID 02h)
    const NVME_LOG_SMART: u32 = 0x02;
    /// Size of SMART/Health Information log page
    const SMART_LOG_SIZE: u32 = 512;

    /// Try to open the NVMe character device.
    pub fn try_open_nvme() -> Option<i32> {
        let devices = ["/dev/nvme0", "/dev/nvme1", "/dev/nvme0n1"];
        for dev in &devices {
            let c_path = match std::ffi::CString::new(*dev) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // SAFETY: open() with O_RDONLY on the NVMe character device.
            // Requires CAP_SYS_ADMIN or being in the nvme/disk group.
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
            if fd >= 0 {
                return Some(fd);
            }
        }
        None
    }

    /// Check if NVMe passthrough is available.
    pub fn has_nvme_passthrough() -> bool {
        if let Some(fd) = try_open_nvme() {
            // SAFETY: close() on a valid fd.
            unsafe { libc::close(fd) };
            true
        } else {
            false
        }
    }

    /// Submit a Get Log Page (SMART/Health) command and return the timing.
    /// Returns (timing_nanos, smart_temperature) or None on failure.
    fn submit_smart_log_page(fd: i32) -> Option<(u64, u16)> {
        let mut log_buf = [0u8; SMART_LOG_SIZE as usize];

        // Number of dwords to return (0-based): (512/4 - 1) = 127
        let numd = (SMART_LOG_SIZE / 4) - 1;

        let mut cmd = NvmePassthruCmd {
            opcode: NVME_ADMIN_GET_LOG_PAGE,
            nsid: 0xFFFF_FFFF, // Global log page
            addr: log_buf.as_mut_ptr() as u64,
            data_len: SMART_LOG_SIZE,
            cdw10: (numd << 16) | NVME_LOG_SMART, // NUMDL[15:0] | LID
            timeout_ms: 1000,
            ..Default::default()
        };

        let t_before = Instant::now();

        // SAFETY: ioctl with NVME_IOCTL_ADMIN_CMD on a valid NVMe character device fd.
        // The cmd struct matches the kernel's expected layout. The log_buf is stack-allocated
        // and large enough for the SMART log page (512 bytes).
        let ret =
            unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD, &mut cmd as *mut NvmePassthruCmd) };

        let elapsed_nanos = t_before.elapsed().as_nanos() as u64;

        if ret < 0 {
            return None;
        }

        // Extract composite temperature from SMART log (bytes 1-2, Kelvin).
        let temp_kelvin = u16::from_le_bytes([log_buf[1], log_buf[2]]);

        Some((elapsed_nanos, temp_kelvin))
    }

    /// Perform multiple SMART log page reads and return timings and temperatures.
    pub fn timed_smart_reads(fd: i32, count: usize) -> (Vec<u64>, Vec<u16>) {
        let mut timings = Vec::with_capacity(count);
        let mut temps = Vec::with_capacity(count);

        for _ in 0..count {
            match submit_smart_log_page(fd) {
                Some((timing, temp)) => {
                    timings.push(timing);
                    temps.push(temp);
                }
                None => {
                    // Command failed, skip this sample.
                }
            }
        }

        (timings, temps)
    }
}

impl EntropySource for NvmePassthroughLinuxSource {
    fn info(&self) -> &SourceInfo {
        &NVME_PASSTHROUGH_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            passthrough::has_nvme_passthrough()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = n_samples;
            Vec::new()
        }

        #[cfg(target_os = "linux")]
        {
            use crate::sources::helpers::xor_fold_u64;

            let fd = match passthrough::try_open_nvme() {
                Some(fd) => fd,
                None => return Vec::new(),
            };

            // Over-sample for the extraction pipeline.
            let raw_count = n_samples * 4 + 64;
            let (timings, temps) = passthrough::timed_smart_reads(fd, raw_count);

            // SAFETY: close() on a valid fd.
            unsafe { libc::close(fd) };

            if timings.len() < 4 {
                return Vec::new();
            }

            // Primary entropy: command round-trip timing.
            let timing_bytes = extract_timing_entropy(&timings, n_samples);

            // Secondary entropy: temperature ADC LSB noise.
            let temp_deltas: Vec<u64> = temps
                .windows(2)
                .map(|w| (w[1] as u64).wrapping_sub(w[0] as u64))
                .collect();
            let temp_xored: Vec<u64> = temp_deltas.windows(2).map(|w| w[0] ^ w[1]).collect();
            let temp_bytes: Vec<u8> = temp_xored
                .iter()
                .map(|&x| xor_fold_u64(x))
                .take(n_samples)
                .collect();

            // XOR both streams together.
            let mut output = Vec::with_capacity(n_samples);
            for i in 0..timing_bytes.len().max(temp_bytes.len()).min(n_samples) {
                let tb = timing_bytes.get(i).copied().unwrap_or(0);
                let tempb = temp_bytes.get(i).copied().unwrap_or(0);
                output.push(tb ^ tempb);
            }
            output.truncate(n_samples);
            output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = NvmePassthroughLinuxSource;
        assert_eq!(src.name(), "nvme_passthrough_linux");
        assert_eq!(src.info().category, SourceCategory::IO);
        assert_eq!(src.info().platform, Platform::Linux);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_ioctl() {
        let src = NvmePassthroughLinuxSource;
        assert!(src.info().physics.contains("ioctl"));
        assert!(src.info().physics.contains("SMART"));
        assert!(src.info().physics.contains("Fowler-Nordheim"));
    }

    #[test]
    fn not_available_on_non_linux() {
        let src = NvmePassthroughLinuxSource;
        #[cfg(not(target_os = "linux"))]
        assert!(!src.is_available());
        #[cfg(target_os = "linux")]
        let _ = src; // availability depends on /dev/nvme0 access
    }

    #[test]
    #[ignore] // Requires Linux with /dev/nvme0 access (CAP_SYS_ADMIN)
    fn collects_bytes() {
        let src = NvmePassthroughLinuxSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
