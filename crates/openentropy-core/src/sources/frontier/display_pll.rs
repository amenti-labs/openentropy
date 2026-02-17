//! Display PLL clock jitter — pixel clock oscillator phase noise.
//!
//! The display subsystem on Apple Silicon uses an independent PLL to generate
//! the pixel clock (measured at ~533 MHz on Mac Mini M4). This PLL is
//! electrically separate from both the CPU's 24 MHz crystal and the audio PLL.
//!
//! ## Entropy mechanism
//!
//! We issue CoreGraphics display queries (`CGDisplayCopyDisplayMode`,
//! `CGDisplayModeGetRefreshRate`, pixel dimension lookups) that cross from the
//! CPU clock domain into display subsystem timing paths. By reading CNTVCT_EL0
//! (CPU crystal) before and after each query, we capture timing variation in the
//! display path influenced by the display PLL's phase noise.
//!
//! The display PLL has its own thermal noise sources:
//! - VCO transistor Johnson-Nyquist noise
//! - Charge pump shot noise
//! - Reference oscillator phase noise
//! - Display cable/connector timing margin noise
//!
//! ## Why this is unique
//!
//! - **Third independent oscillator**: neither CPU crystal nor audio PLL
//! - **High frequency PLL**: 533 MHz pixel clock means faster phase drift
//! - **No special permissions**: CoreVideo is a standard framework
//! - **Works headless**: Mac Mini always has a virtual display
//!
//! ## Tradeoff
//!
//! CoreGraphics query collection is slower than pure syscall-based sources.
//! We oversample and extract timing deltas to recover usable entropy density.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::{extract_timing_entropy, read_cntvct};

static DISPLAY_PLL_INFO: SourceInfo = SourceInfo {
    name: "display_pll",
    description: "Display PLL phase noise from pixel clock domain crossing",
    physics: "Queries CoreGraphics display properties (mode, refresh rate, color space) \
              that cross into the display PLL\u{2019}s clock domain (~533 MHz pixel clock). \
              The display PLL is an independent oscillator from both the CPU crystal \
              (24 MHz) and audio PLL (48 kHz). Phase noise arises from VCO transistor \
              Johnson-Nyquist noise and charge pump shot noise in the display PLL. \
              Reading CNTVCT_EL0 before and after each query captures the beat between \
              CPU crystal and display PLL.",
    category: SourceCategory::Thermal,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon],
    entropy_rate_estimate: 2500.0,
    composite: false,
};

/// Display PLL phase noise entropy source.
pub struct DisplayPllSource;

/// CoreGraphics FFI for display property queries.
#[cfg(target_os = "macos")]
mod coregraphics {
    use std::ffi::c_void;

    // CGDirectDisplayID is u32 on macOS.
    type CGDirectDisplayID = u32;
    // CGDisplayModeRef is an opaque pointer.
    type CGDisplayModeRef = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        fn CGDisplayCopyDisplayMode(display: CGDirectDisplayID) -> CGDisplayModeRef;
        fn CGDisplayModeGetRefreshRate(mode: CGDisplayModeRef) -> f64;
        fn CGDisplayModeGetPixelWidth(mode: CGDisplayModeRef) -> usize;
        fn CGDisplayModeGetPixelHeight(mode: CGDisplayModeRef) -> usize;
        fn CGDisplayModeRelease(mode: CGDisplayModeRef);
        fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> usize;
        fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> usize;
    }

    /// Check if a display is available.
    pub fn has_display() -> bool {
        // SAFETY: CGMainDisplayID returns 0 if no display is available.
        // It's a read-only query with no side effects.
        unsafe { CGMainDisplayID() != 0 }
    }

    /// Query display mode properties, forcing a clock domain crossing.
    /// Returns different property values based on `query_type` to exercise
    /// different code paths in the display subsystem.
    pub fn query_display_property(query_type: usize) -> u64 {
        // SAFETY: All CG functions here are read-only queries on the main display.
        // CGDisplayCopyDisplayMode returns a retained reference that we release.
        unsafe {
            let display = CGMainDisplayID();
            if display == 0 {
                return 0;
            }

            match query_type % 4 {
                0 => {
                    // Query display mode (refresh rate) — crosses into display PLL
                    let mode = CGDisplayCopyDisplayMode(display);
                    if mode.is_null() {
                        return 0;
                    }
                    let rate = CGDisplayModeGetRefreshRate(mode);
                    CGDisplayModeRelease(mode);
                    rate.to_bits()
                }
                1 => {
                    // Query pixel dimensions via display mode
                    let mode = CGDisplayCopyDisplayMode(display);
                    if mode.is_null() {
                        return 0;
                    }
                    let w = CGDisplayModeGetPixelWidth(mode);
                    let h = CGDisplayModeGetPixelHeight(mode);
                    CGDisplayModeRelease(mode);
                    (w as u64) ^ (h as u64).rotate_left(32)
                }
                2 => {
                    // Direct pixel query (different code path)
                    let w = CGDisplayPixelsWide(display);
                    let h = CGDisplayPixelsHigh(display);
                    (w as u64).wrapping_mul(h as u64)
                }
                _ => {
                    // Mode + refresh combined
                    let mode = CGDisplayCopyDisplayMode(display);
                    if mode.is_null() {
                        return 0;
                    }
                    let rate = CGDisplayModeGetRefreshRate(mode);
                    let w = CGDisplayModeGetPixelWidth(mode);
                    CGDisplayModeRelease(mode);
                    rate.to_bits() ^ (w as u64)
                }
            }
        }
    }
}

impl EntropySource for DisplayPllSource {
    fn info(&self) -> &SourceInfo {
        &DISPLAY_PLL_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            coregraphics::has_display()
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            false
        }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = n_samples;
            return Vec::new();
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let raw_count = n_samples * 4 + 64;
            let mut beats: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                // Read CPU crystal counter before display domain crossing.
                let counter_before = read_cntvct();

                // Force a clock domain crossing into the display PLL.
                let display_val = coregraphics::query_display_property(i);
                std::hint::black_box(display_val);

                // Read CPU crystal counter after display domain crossing.
                let counter_after = read_cntvct();

                // The duration in CNTVCT ticks captures the clock domain crossing
                // time, modulated by the display PLL's phase. We don't XOR with
                // counter_before (a monotonic counter creates NIST-detectable patterns).
                let duration = counter_after.wrapping_sub(counter_before);
                beats.push(duration);
            }

            extract_timing_entropy(&beats, n_samples)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = DisplayPllSource;
        assert_eq!(src.name(), "display_pll");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_display() {
        let src = DisplayPllSource;
        assert!(src.info().physics.contains("display PLL"));
        assert!(src.info().physics.contains("533 MHz"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn collects_bytes() {
        let src = DisplayPllSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
