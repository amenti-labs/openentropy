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

/// Pack a stream of individual bits (0 or 1) into bytes (MSB-first packing).
///
/// For every 8 input bits, one output byte is produced.
fn pack_bits_into_bytes(bits: &[u8]) -> Vec<u8> {
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

/// Extract the least-significant bit of each `u64` delta and pack into bytes.
///
/// For every 8 input values, one output byte is produced (MSB-first packing).
pub fn extract_lsbs_u64(deltas: &[u64]) -> Vec<u8> {
    let bits: Vec<u8> = deltas.iter().map(|d| (d & 1) as u8).collect();
    pack_bits_into_bytes(&bits)
}

/// Extract the least-significant bit of each `i64` delta and pack into bytes.
///
/// Identical to [`extract_lsbs_u64`] but for signed deltas.
pub fn extract_lsbs_i64(deltas: &[i64]) -> Vec<u8> {
    let bits: Vec<u8> = deltas.iter().map(|d| (d & 1) as u8).collect();
    pack_bits_into_bytes(&bits)
}

// ---------------------------------------------------------------------------
// Shared command utilities
// ---------------------------------------------------------------------------

/// Check if a command exists by running `which`.
pub fn command_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Nibble packing
// ---------------------------------------------------------------------------

/// Pack pairs of 4-bit nibbles into bytes.
///
/// Used by audio and camera sources to pack noise LSBs efficiently.
/// Returns at most `max_bytes` output bytes.
pub fn pack_nibbles(nibbles: impl Iterator<Item = u8>, max_bytes: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(max_bytes);
    let mut buf: u8 = 0;
    let mut count: u8 = 0;

    for nibble in nibbles {
        if count == 0 {
            buf = nibble << 4;
            count = 1;
        } else {
            buf |= nibble;
            output.push(buf);
            count = 0;
            if output.len() >= max_bytes {
                break;
            }
        }
    }

    // If we have an odd nibble left and still need more, include it.
    if count == 1 && output.len() < max_bytes {
        output.push(buf);
    }

    output.truncate(max_bytes);
    output
}

// ---------------------------------------------------------------------------
// i64 delta byte extraction (sysctl/vmstat/ioregistry pattern)
// ---------------------------------------------------------------------------

/// Extract entropy bytes from a list of i64 deltas.
///
/// First emits raw LE bytes from all deltas, then XOR'd consecutive delta bytes
/// if more output is needed. Returns at most `n_samples` bytes.
pub fn extract_delta_bytes_i64(deltas: &[i64], n_samples: usize) -> Vec<u8> {
    // XOR consecutive deltas for extra mixing
    let xor_deltas: Vec<i64> = if deltas.len() >= 2 {
        deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
    } else {
        Vec::new()
    };

    let mut entropy = Vec::with_capacity(n_samples);

    // First: raw LE bytes from all non-zero deltas
    for d in deltas {
        for &b in &d.to_le_bytes() {
            entropy.push(b);
        }
        if entropy.len() >= n_samples {
            entropy.truncate(n_samples);
            return entropy;
        }
    }

    // Then: XOR'd delta bytes for more mixing
    for d in &xor_deltas {
        for &b in &d.to_le_bytes() {
            entropy.push(b);
        }
        if entropy.len() >= n_samples {
            break;
        }
    }

    entropy.truncate(n_samples);
    entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // LSB extraction tests
    // -----------------------------------------------------------------------

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
    fn extract_lsbs_u64_empty() {
        let bytes = extract_lsbs_u64(&[]);
        assert!(bytes.is_empty());
    }

    #[test]
    fn extract_lsbs_i64_empty() {
        let bytes = extract_lsbs_i64(&[]);
        assert!(bytes.is_empty());
    }

    #[test]
    fn extract_lsbs_u64_all_odd() {
        // All odd -> all LSBs are 1 -> 0xFF
        let deltas = vec![1u64, 3, 5, 7, 9, 11, 13, 15];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes[0], 0xFF);
    }

    #[test]
    fn extract_lsbs_u64_all_even() {
        // All even -> all LSBs are 0 -> 0x00
        let deltas = vec![0u64, 2, 4, 6, 8, 10, 12, 14];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes[0], 0x00);
    }

    #[test]
    fn extract_lsbs_partial_byte() {
        // 5 values -> only 5 bits, still produces 1 byte (padded)
        let deltas = vec![1u64, 0, 1, 0, 1];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes.len(), 1);
        // Bits: 1,0,1,0,1,0,0,0 = 0b10101000 = 0xA8
        assert_eq!(bytes[0], 0b10101000);
    }

    #[test]
    fn extract_lsbs_u64_i64_agree() {
        // Same absolute values should produce same LSBs
        let u_deltas = vec![1u64, 2, 3, 4, 5, 6, 7, 8];
        let i_deltas = vec![1i64, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(extract_lsbs_u64(&u_deltas), extract_lsbs_i64(&i_deltas));
    }

    // -----------------------------------------------------------------------
    // pack_bits_into_bytes tests
    // -----------------------------------------------------------------------

    #[test]
    fn pack_bits_empty() {
        let bits: Vec<u8> = vec![];
        let bytes = pack_bits_into_bytes(&bits);
        assert!(bytes.is_empty());
    }

    #[test]
    fn pack_bits_full_byte() {
        let bits = vec![1, 0, 1, 0, 1, 0, 1, 0];
        let bytes = pack_bits_into_bytes(&bits);
        assert_eq!(bytes, vec![0b10101010]);
    }

    // -----------------------------------------------------------------------
    // mach_time tests
    // -----------------------------------------------------------------------

    #[test]
    fn mach_time_is_monotonic() {
        let t1 = mach_time();
        let t2 = mach_time();
        assert!(t2 >= t1);
    }

    // -----------------------------------------------------------------------
    // pack_nibbles tests
    // -----------------------------------------------------------------------

    #[test]
    fn pack_nibbles_basic() {
        let nibbles = vec![0x0A_u8, 0x0B];
        let bytes = pack_nibbles(nibbles.into_iter(), 10);
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0xAB);
    }

    #[test]
    fn pack_nibbles_empty() {
        let bytes = pack_nibbles(std::iter::empty(), 10);
        assert!(bytes.is_empty());
    }

    #[test]
    fn pack_nibbles_odd_count() {
        let nibbles = vec![0x0C_u8, 0x0D, 0x0E];
        let bytes = pack_nibbles(nibbles.into_iter(), 10);
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0], 0xCD);
        assert_eq!(bytes[1], 0xE0); // odd nibble shifted left
    }

    #[test]
    fn pack_nibbles_respects_max() {
        let nibbles = vec![0x01_u8, 0x02, 0x03, 0x04, 0x05, 0x06];
        let bytes = pack_nibbles(nibbles.into_iter(), 2);
        assert_eq!(bytes.len(), 2);
    }

    // -----------------------------------------------------------------------
    // extract_delta_bytes_i64 tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_delta_bytes_empty() {
        let bytes = extract_delta_bytes_i64(&[], 10);
        assert!(bytes.is_empty());
    }

    #[test]
    fn extract_delta_bytes_single_delta() {
        let deltas = vec![0x0102030405060708i64];
        let bytes = extract_delta_bytes_i64(&deltas, 8);
        // LE bytes of the delta
        assert_eq!(bytes, 0x0102030405060708i64.to_le_bytes().to_vec());
    }

    #[test]
    fn extract_delta_bytes_truncated() {
        let deltas = vec![0x0102030405060708i64];
        let bytes = extract_delta_bytes_i64(&deltas, 4);
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes, &0x0102030405060708i64.to_le_bytes()[..4]);
    }

    #[test]
    fn extract_delta_bytes_with_xor_mixing() {
        // Two deltas -> also produces XOR'd delta bytes for extra output
        let deltas = vec![100i64, 200];
        let bytes = extract_delta_bytes_i64(&deltas, 24);
        // 2 deltas * 8 bytes = 16 raw LE bytes + 1 XOR'd delta * 8 bytes = 24 total
        assert_eq!(bytes.len(), 24);
    }

    #[test]
    fn extract_delta_bytes_respects_n_samples() {
        let deltas: Vec<i64> = (1..=100).collect();
        let bytes = extract_delta_bytes_i64(&deltas, 50);
        assert_eq!(bytes.len(), 50);
    }
}
