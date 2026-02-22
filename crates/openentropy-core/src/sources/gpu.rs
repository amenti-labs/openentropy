//! GPUTimingSource — GPU dispatch timing via the `sips` command.
//!
//! Creates a small temporary TIFF image and uses macOS's `sips` command to
//! resize it, measuring per-operation timing. The `sips` tool dispatches
//! Metal/CoreImage compute work, and the timing jitter reflects GPU scheduling.

use std::io::Write;
use std::process::Command;
use std::time::Instant;

use tempfile::NamedTempFile;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

/// Path to the sips binary on macOS.
const SIPS_PATH: &str = "/usr/bin/sips";

static GPU_TIMING_INFO: SourceInfo = SourceInfo {
    name: "gpu_timing",
    description: "GPU dispatch timing jitter via sips image processing",
    physics: "Dispatches Metal compute shaders and measures completion time. GPU timing \
              jitter comes from: shader core occupancy, register file allocation, shared \
              memory bank conflicts, warp/wavefront scheduling, power throttling, and memory \
              controller arbitration between GPU cores, CPU, and Neural Engine on Apple \
              Silicon's unified memory.",
    category: SourceCategory::GPU,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 300.0,
    composite: false,
};

/// Minimal valid TIFF file (8x8 grayscale, uncompressed).
/// TIFF header + IFD + 64 bytes of pixel data.
fn create_minimal_tiff() -> Vec<u8> {
    let mut tiff = Vec::new();

    // TIFF Header: little-endian, magic 42, IFD offset 8
    tiff.extend_from_slice(&[0x49, 0x49]); // "II" = little-endian
    tiff.extend_from_slice(&42u16.to_le_bytes()); // TIFF magic
    tiff.extend_from_slice(&8u32.to_le_bytes()); // offset to first IFD

    // IFD at offset 8
    let num_entries: u16 = 8;
    tiff.extend_from_slice(&num_entries.to_le_bytes());

    // Helper: write a 12-byte IFD entry (tag, type, count, value)
    // Type 3 = SHORT (2 bytes), Type 4 = LONG (4 bytes)
    let write_entry = |tiff: &mut Vec<u8>, tag: u16, typ: u16, count: u32, value: u32| {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&typ.to_le_bytes());
        tiff.extend_from_slice(&count.to_le_bytes());
        tiff.extend_from_slice(&value.to_le_bytes());
    };

    // ImageWidth = 8
    write_entry(&mut tiff, 256, 3, 1, 8);
    // ImageLength = 8
    write_entry(&mut tiff, 257, 3, 1, 8);
    // BitsPerSample = 8
    write_entry(&mut tiff, 258, 3, 1, 8);
    // Compression = 1 (no compression)
    write_entry(&mut tiff, 259, 3, 1, 1);
    // PhotometricInterpretation = 1 (min-is-black)
    write_entry(&mut tiff, 262, 3, 1, 1);
    // StripOffsets — pixel data starts after IFD
    // IFD: 2 (count) + 8*12 (entries) + 4 (next IFD) = 102 bytes from offset 8
    // Total header = 8 + 102 = 110
    let pixel_offset: u32 = 8 + 2 + (num_entries as u32) * 12 + 4;
    write_entry(&mut tiff, 273, 4, 1, pixel_offset);
    // RowsPerStrip = 8
    write_entry(&mut tiff, 278, 3, 1, 8);
    // StripByteCounts = 64 (8x8 pixels, 1 byte each)
    write_entry(&mut tiff, 279, 4, 1, 64);

    // Next IFD offset = 0 (no more IFDs)
    tiff.extend_from_slice(&0u32.to_le_bytes());

    // Pixel data: 64 bytes of gray values
    tiff.extend_from_slice(&[128u8; 64]);

    tiff
}

/// Entropy source that harvests timing jitter from GPU image processing operations.
pub struct GPUTimingSource;

impl EntropySource for GPUTimingSource {
    fn info(&self) -> &SourceInfo {
        &GPU_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos") && std::path::Path::new(SIPS_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Create a temporary TIFF file.
        let mut tmpfile = match NamedTempFile::with_suffix(".tiff") {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let tiff_data = create_minimal_tiff();
        if tmpfile.write_all(&tiff_data).is_err() {
            return Vec::new();
        }
        if tmpfile.flush().is_err() {
            return Vec::new();
        }

        let path = tmpfile.path().to_path_buf();
        let mut output = Vec::with_capacity(n_samples);
        let mut prev_ns: u64 = 0;

        // Each iteration: resize the image via sips and measure timing.
        // Alternate between sizes to force actual GPU work each time.
        let sizes = [16, 32, 24, 48, 12, 36];
        let iterations = n_samples + 1;

        for i in 0..iterations {
            let size = sizes[i % sizes.len()];

            let t0 = Instant::now();

            let result = Command::new(SIPS_PATH)
                .args([
                    "--resampleWidth",
                    &size.to_string(),
                    "--resampleHeight",
                    &size.to_string(),
                    path.to_str().unwrap_or(""),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            let elapsed_ns = t0.elapsed().as_nanos() as u64;

            if result.is_err() {
                continue;
            }

            if i > 0 {
                let delta = elapsed_ns.wrapping_sub(prev_ns);
                // XOR low bytes for mixing.
                let mixed = (delta as u8) ^ ((delta >> 8) as u8) ^ ((delta >> 16) as u8);
                output.push(mixed);

                if output.len() >= n_samples {
                    break;
                }
            }

            prev_ns = elapsed_ns;
        }

        output.truncate(n_samples);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_timing_info() {
        let src = GPUTimingSource;
        assert_eq!(src.name(), "gpu_timing");
        assert_eq!(src.info().category, SourceCategory::GPU);
    }

    #[test]
    fn minimal_tiff_is_valid() {
        let tiff = create_minimal_tiff();
        // TIFF starts with "II" (little-endian) and magic 42
        assert_eq!(&tiff[0..2], b"II");
        assert_eq!(u16::from_le_bytes([tiff[2], tiff[3]]), 42);
        // Should have pixel data at the end
        assert!(tiff.len() > 64);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn gpu_timing_availability() {
        let src = GPUTimingSource;
        // On macOS, sips should always be present.
        assert!(src.is_available());
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires sips binary and GPU
    fn gpu_timing_collects_bytes() {
        let src = GPUTimingSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
