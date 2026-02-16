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
    // SAFETY: mach_absolute_time() is a stable macOS API that returns the
    // current value of the system absolute time counter. Always safe to call.
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

/// Execute a command and return its `Output` if it succeeds.
fn run_command_output(program: &str, args: &[&str]) -> Option<std::process::Output> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(output)
}

/// Run a subprocess command and return its stdout as a `String`.
///
/// Returns `None` if the command fails to execute or exits with a non-zero
/// status. This is the shared helper for sources that shell out to system
/// utilities (sysctl, vm_stat, ps, ioreg, mdls, etc.).
pub fn run_command(program: &str, args: &[&str]) -> Option<String> {
    let output = run_command_output(program, args)?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run a subprocess command and return its raw stdout bytes.
///
/// Returns `None` if the command fails to execute or exits with a non-zero
/// status.
pub fn run_command_raw(program: &str, args: &[&str]) -> Option<Vec<u8>> {
    run_command_output(program, args).map(|o| o.stdout)
}

// ---------------------------------------------------------------------------
// ARM counter (CNTVCT_EL0)
// ---------------------------------------------------------------------------

/// Read the ARM generic timer counter (CNTVCT_EL0) directly.
///
/// Returns the raw hardware counter driven by the CPU's 24 MHz crystal oscillator,
/// independent of any OS abstraction layer. Used by frontier sources that measure
/// clock domain crossings against independent PLLs (audio, display, PCIe).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[inline(always)]
pub fn read_cntvct() -> u64 {
    let val: u64;
    // SAFETY: CNTVCT_EL0 is always readable from EL0 on Apple Silicon.
    // Read-only system register, no side effects.
    unsafe {
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) val, options(nostack, nomem));
    }
    val
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[inline(always)]
pub fn read_cntvct() -> u64 {
    0
}

// ---------------------------------------------------------------------------
// XOR-fold
// ---------------------------------------------------------------------------

/// XOR-fold all 8 bytes of a `u64` into a single byte.
///
/// Preserves entropy from every byte position instead of discarding the
/// upper 7 bytes. Used by timing-based sources where entropy is spread
/// across multiple byte positions.
#[inline]
pub fn xor_fold_u64(v: u64) -> u8 {
    let b = v.to_le_bytes();
    b[0] ^ b[1] ^ b[2] ^ b[3] ^ b[4] ^ b[5] ^ b[6] ^ b[7]
}

// ---------------------------------------------------------------------------
// Timing entropy extraction
// ---------------------------------------------------------------------------

/// Extract entropy bytes from a slice of raw timestamps.
///
/// Computes consecutive deltas, XORs adjacent deltas for mixing, then
/// XOR-folds each 8-byte value into one output byte. Returns at most
/// `n_samples` bytes.
///
/// Requires at least 4 input timings to produce any output (2 deltas
/// needed for the XOR mixing step).
pub fn extract_timing_entropy(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 2 {
        return Vec::new();
    }

    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // XOR consecutive deltas for mixing (not conditioning — just combines adjacent values)
    let xored: Vec<u64> = deltas.windows(2).map(|w| w[0] ^ w[1]).collect();

    // XOR-fold all 8 bytes of each value into one byte
    let mut raw: Vec<u8> = xored.iter().map(|&x| xor_fold_u64(x)).collect();
    raw.truncate(n_samples);
    raw
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

    // -----------------------------------------------------------------------
    // xor_fold_u64 tests
    // -----------------------------------------------------------------------

    #[test]
    fn xor_fold_u64_zero() {
        assert_eq!(xor_fold_u64(0), 0);
    }

    #[test]
    fn xor_fold_u64_identical_bytes() {
        // All bytes the same: XOR-fold of 8 identical bytes = 0 (even count)
        assert_eq!(xor_fold_u64(0x0101010101010101), 0);
    }

    #[test]
    fn xor_fold_u64_single_byte() {
        assert_eq!(xor_fold_u64(0xFF), 0xFF);
    }

    #[test]
    fn xor_fold_u64_two_bytes() {
        // 0xAA ^ 0xBB = 0x11
        assert_eq!(xor_fold_u64(0xBB_00_00_00_00_00_00_AA), 0xAA ^ 0xBB);
    }

    #[test]
    fn xor_fold_u64_max() {
        // All 0xFF bytes: XOR of 8 identical = 0 (even count)
        assert_eq!(xor_fold_u64(u64::MAX), 0);
    }

    // -----------------------------------------------------------------------
    // extract_timing_entropy tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_timing_entropy_basic() {
        let timings = vec![100, 110, 105, 120, 108, 130, 112, 125];
        let result = extract_timing_entropy(&timings, 4);
        assert!(!result.is_empty());
        assert!(result.len() <= 4);
    }

    #[test]
    fn extract_timing_entropy_too_few_samples() {
        assert!(extract_timing_entropy(&[], 10).is_empty());
        assert!(extract_timing_entropy(&[42], 10).is_empty());
    }

    #[test]
    fn extract_timing_entropy_exactly_two_timings() {
        // 2 timings → 1 delta → 0 XOR'd pairs → empty
        let result = extract_timing_entropy(&[100, 200], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_timing_entropy_exactly_three_timings() {
        // 3 timings → 2 deltas → 1 XOR'd pair → 1 byte
        let result = extract_timing_entropy(&[100, 200, 150], 10);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn extract_timing_entropy_truncates_to_n_samples() {
        let timings: Vec<u64> = (0..100).collect();
        let result = extract_timing_entropy(&timings, 5);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn extract_timing_entropy_constant_timings() {
        // Constant timings → all deltas are 0 → XOR of 0s = 0 → all zeros
        let timings = vec![42u64; 20];
        let result = extract_timing_entropy(&timings, 10);
        assert!(result.iter().all(|&b| b == 0));
    }

    // -----------------------------------------------------------------------
    // run_command / run_command_raw tests
    // -----------------------------------------------------------------------

    #[test]
    fn run_command_echo() {
        let out = run_command("echo", &["hello"]);
        assert!(out.is_some());
        assert_eq!(out.unwrap().trim(), "hello");
    }

    #[test]
    fn run_command_nonexistent() {
        let out = run_command("/nonexistent/binary", &[]);
        assert!(out.is_none());
    }

    #[test]
    fn run_command_raw_echo() {
        let out = run_command_raw("echo", &["bytes"]);
        assert!(out.is_some());
        assert!(out.unwrap().starts_with(b"bytes"));
    }

    #[test]
    fn run_command_failing_status() {
        // `false` always exits with status 1
        let out = run_command("false", &[]);
        assert!(out.is_none());
    }

    #[test]
    fn run_command_raw_failing_status() {
        let out = run_command_raw("false", &[]);
        assert!(out.is_none());
    }

    #[test]
    fn run_command_empty_output() {
        // `true` exits 0 with no output
        let out = run_command("true", &[]);
        assert!(out.is_some());
        assert!(out.unwrap().is_empty());
    }

    #[test]
    fn command_exists_true() {
        assert!(command_exists("echo"));
    }

    #[test]
    fn command_exists_false() {
        assert!(!command_exists("nonexistent_binary_xyz_12345"));
    }
}
