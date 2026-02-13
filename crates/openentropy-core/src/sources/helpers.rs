//! Shared helpers used by multiple entropy source implementations.
//!
//! This module prevents code duplication across sources that need common
//! low-level primitives like high-resolution timestamps and LSB extraction.

// ---------------------------------------------------------------------------
// High-resolution timing
// ---------------------------------------------------------------------------

/// High-resolution timestamp in nanoseconds.
///
/// On macOS, this reads the ARM system counter directly via `mach_absolute_time()`.
/// On other platforms, it falls back to `std::time::Instant` relative to a
/// process-local epoch.
#[cfg(target_os = "macos")]
pub fn mach_time() -> u64 {
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
    }
    unsafe { mach_absolute_time() }
}

#[cfg(not(target_os = "macos"))]
pub fn mach_time() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_nanos() as u64
}

// ---------------------------------------------------------------------------
// LSB extraction
// ---------------------------------------------------------------------------

/// Extract the least-significant bit of each `u64` delta and pack into bytes.
///
/// For every 8 input values, one output byte is produced (MSB-first packing).
pub fn extract_lsbs_u64(deltas: &[u64]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::with_capacity(deltas.len());
    for d in deltas {
        bits.push((d & 1) as u8);
    }

    let mut bytes = Vec::with_capacity(bits.len() / 8 + 1);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
}

/// Extract the least-significant bit of each `i64` delta and pack into bytes.
///
/// Identical to [`extract_lsbs_u64`] but for signed deltas.
pub fn extract_lsbs_i64(deltas: &[i64]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::with_capacity(deltas.len());
    for d in deltas {
        bits.push((d & 1) as u8);
    }

    let mut bytes = Vec::with_capacity(bits.len() / 8 + 1);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_lsbs_u64_basic() {
        // 8 values with alternating LSBs: 0,1,0,1,0,1,0,1 → 0b01010101 = 0x55
        let deltas = vec![2, 3, 4, 5, 6, 7, 8, 9];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0b01010101);
    }

    #[test]
    fn extract_lsbs_i64_basic() {
        let deltas = vec![2i64, 3, 4, 5, 6, 7, 8, 9];
        let bytes = extract_lsbs_i64(&deltas);
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0b01010101);
    }

    #[test]
    fn mach_time_is_monotonic() {
        let t1 = mach_time();
        let t2 = mach_time();
        assert!(t2 >= t1);
    }
}
