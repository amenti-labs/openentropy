//! Two-oscillator beat frequency — CPU counter vs audio PLL crystal.
//!
//! Measures the phase difference between two physically independent oscillators:
//! - **CNTVCT_EL0**: ARM generic timer counter, driven by the CPU's 24 MHz crystal
//! - **Audio PLL**: the audio subsystem's independent crystal oscillator, probed
//!   via CoreAudio property queries that force clock domain crossings
//!
//! The entropy arises from independent thermal noise (Johnson-Nyquist) in each
//! crystal oscillator's sustaining circuit, causing uncorrelated phase drift
//! between the two clock domains. This two-oscillator beat technique is used in
//! some hardware random number generators for anomaly detection research (note:
//! the original PEAR lab REGs used noise diodes, not oscillator beats).
//!
//! ## Mechanism
//!
//! Each sample reads CNTVCT_EL0 immediately before and after a CoreAudio property
//! query (actual sample rate, latency) that forces synchronization with the audio
//! PLL clock domain. The query duration in raw counter ticks is modulated by the
//! instantaneous phase relationship between the CPU crystal and the audio PLL.
//! XORing the raw counter value with this PLL-modulated duration produces a beat
//! that encodes the phase difference between the two independent oscillators.
//!
//! ## Why this matters for anomaly detection research
//!
//! - **Clean physical signal**: thermal noise in crystal oscillators is as close to
//!   quantum-origin randomness as consumer hardware provides
//! - **High sample rate**: thousands of phase-difference samples per second
//! - **Well-characterized physics**: crystal oscillator phase noise (Allan variance,
//!   flicker FM, white PM) is thoroughly documented in metrology literature
//! - **Low min-entropy is a feature**: a source with H∞ ~1–3 bits/byte is easier
//!   to detect statistical anomalies in than one at 7.9 — useful for anomaly
//!   detection experiments
//!
//! ## Previous version
//!
//! An earlier `counter_beat` was removed because it XOR'd CNTVCT_EL0 with
//! `mach_absolute_time()` — which on Apple Silicon is the *same* counter, not an
//! independent oscillator. This version fixes that by using the audio PLL as the
//! genuinely independent second clock domain, validated by `audio_pll_timing`'s

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::read_cntvct;
#[cfg(target_os = "macos")]
use crate::sources::helpers::xor_fold_u64;

static COUNTER_BEAT_INFO: SourceInfo = SourceInfo {
    name: "counter_beat",
    description: "Two-oscillator beat frequency: CPU counter (CNTVCT_EL0) vs audio PLL crystal",
    physics: "Reads the ARM generic timer counter (CNTVCT_EL0, driven by a 24 MHz crystal) \
              immediately before and after a CoreAudio property query that forces \
              synchronization with the audio PLL clock domain. The query duration in raw \
              counter ticks is modulated by the instantaneous phase relationship between \
              the CPU crystal and the independent audio PLL crystal. XORing the counter \
              value with this PLL-modulated duration produces a two-oscillator beat that \
              encodes the phase difference between two independent oscillators. \
              Entropy arises from independent \
              Johnson-Nyquist thermal noise in each crystal's sustaining amplifier. \
              The raw physical signal is preserved for statistical analysis.
              raw physical signal for statistical analysis.",
    category: SourceCategory::Thermal,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::AudioUnit],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

/// Two-oscillator beat frequency entropy source.
///
/// Captures the instantaneous phase difference between the CPU's ARM counter
/// and the audio PLL clock — two physically independent crystal oscillators
/// with uncorrelated thermal noise.
pub struct CounterBeatSource;

/// CoreAudio FFI for audio PLL clock domain crossing.
#[cfg(target_os = "macos")]
mod coreaudio {
    #[repr(C)]
    pub struct AudioObjectPropertyAddress {
        pub m_selector: u32,
        pub m_scope: u32,
        pub m_element: u32,
    }

    pub const AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    pub const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = 0x644F7574; // 'dOut'
    pub const AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE: u32 = 0x61737264; // 'asrd'
    pub const AUDIO_DEVICE_PROPERTY_LATENCY: u32 = 0x6C746E63; // 'ltnc'
    pub const AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: u32 = 0x6E737274; // 'nsrt'
    pub const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676C6F62; // 'glob'
    pub const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
    pub const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = 0x6F757470; // 'outp'

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        pub fn AudioObjectGetPropertyData(
            object_id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_data_size: u32,
            qualifier_data: *const std::ffi::c_void,
            data_size: *mut u32,
            data: *mut std::ffi::c_void,
        ) -> i32;
    }

    /// Get the default output audio device ID, or 0 if none.
    pub fn get_default_output_device() -> u32 {
        let addr = AudioObjectPropertyAddress {
            m_selector: AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            m_scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut device: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        // SAFETY: AudioObjectGetPropertyData reads a property from the system
        // audio object. We pass valid pointers with correct sizes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut device as *mut u32 as *mut std::ffi::c_void,
            )
        };
        if status == 0 { device } else { 0 }
    }

    /// Force a clock domain crossing by querying an audio device property.
    ///
    /// Returns the raw bytes read from the audio subsystem (we don't care about
    /// the value — the point is forcing the CPU to synchronize with the audio PLL).
    pub fn query_audio_property(device: u32, selector: u32, scope: u32) {
        let addr = AudioObjectPropertyAddress {
            m_selector: selector,
            m_scope: scope,
            m_element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut data = [0u8; 8];
        let mut size: u32 = 8;
        // SAFETY: AudioObjectGetPropertyData reads a property from a valid audio device.
        // `data` is an 8-byte stack buffer sufficient for all queried properties.
        unsafe {
            AudioObjectGetPropertyData(
                device,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                data.as_mut_ptr() as *mut std::ffi::c_void,
            );
        }
        // Prevent the compiler from optimizing away the query.
        std::hint::black_box(data);
    }
}

impl EntropySource for CounterBeatSource {
    fn info(&self) -> &SourceInfo {
        &COUNTER_BEAT_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            coreaudio::get_default_output_device() != 0
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
            Vec::new()
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let device = coreaudio::get_default_output_device();
            if device == 0 {
                return Vec::new();
            }

            // Cycle through different audio property queries to exercise
            // different code paths crossing the PLL clock domain boundary.
            let selectors = [
                (
                    coreaudio::AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE,
                    coreaudio::AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                ),
                (
                    coreaudio::AUDIO_DEVICE_PROPERTY_LATENCY,
                    coreaudio::AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
                ),
                (
                    coreaudio::AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE,
                    coreaudio::AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                ),
            ];

            // Over-collect: delta + XOR + fold reduces count.
            let raw_count = n_samples * 4 + 64;
            let mut beats: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                let (sel, scope) = selectors[i % selectors.len()];

                // Read CNTVCT_EL0 immediately before the audio PLL crossing.
                let counter_before = read_cntvct();

                // Force a clock domain crossing into the audio PLL.
                coreaudio::query_audio_property(device, sel, scope);

                // Read CNTVCT_EL0 immediately after.
                let counter_after = read_cntvct();

                // The beat: XOR the raw counter value (CPU oscillator phase)
                // with the PLL-modulated duration (audio oscillator phase).
                // The duration encodes how long the CPU had to wait for the
                // audio PLL to respond — modulated by the instantaneous phase
                // relationship between the two independent crystals.
                let pll_duration = counter_after.wrapping_sub(counter_before);
                let beat = counter_before ^ pll_duration;
                beats.push(beat);
            }

            if beats.len() < 4 {
                return Vec::new();
            }

            // Extract entropy: consecutive beat differences capture the phase
            // drift rate, then XOR adjacent deltas and fold to bytes.
            let deltas: Vec<u64> = beats.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();

            let xored: Vec<u64> = deltas.windows(2).map(|w| w[0] ^ w[1]).collect();

            let mut output: Vec<u8> = xored.iter().map(|&x| xor_fold_u64(x)).collect();
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
        let src = CounterBeatSource;
        assert_eq!(src.name(), "counter_beat");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_two_oscillators() {
        let src = CounterBeatSource;
        assert!(src.info().physics.contains("CNTVCT_EL0"));
        assert!(src.info().physics.contains("two-oscillator"));
        assert!(src.info().physics.contains("phase difference"));
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn cntvct_is_nonzero() {
        let v = read_cntvct();
        assert!(v > 0);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[ignore] // Requires audio hardware
    fn collects_bytes() {
        let src = CounterBeatSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
