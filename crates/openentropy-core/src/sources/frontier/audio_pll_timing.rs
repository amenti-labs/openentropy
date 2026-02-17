//! Audio PLL clock jitter — phase noise from the audio subsystem oscillator.
//!
//! The audio subsystem has its own Phase-Locked Loop (PLL) generating sample
//! clocks. By rapidly querying CoreAudio device properties, we measure timing
//! jitter from crossing the audio/CPU clock domain boundary.
//!
//! The PLL phase noise arises from:
//! - Thermal noise in VCO transistors (Johnson-Nyquist)
//! - Shot noise in charge pump current
//! - Reference oscillator crystal phase noise
//!

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static AUDIO_PLL_TIMING_INFO: SourceInfo = SourceInfo {
    name: "audio_pll_timing",
    description: "Audio PLL clock jitter from CoreAudio device property queries",
    physics: "Rapidly queries CoreAudio device properties (sample rate, latency) that \
              cross the audio PLL / CPU clock domain boundary. The audio subsystem\u{2019}s \
              PLL has thermally-driven phase noise from VCO transistor Johnson-Nyquist \
              noise, charge pump shot noise, and crystal reference jitter. Each query \
              timing captures the instantaneous phase relationship between these \
              independent clock domains.",
    category: SourceCategory::Thermal,
    platform: Platform::MacOS,
    requirements: &[Requirement::AudioUnit],
    entropy_rate_estimate: 4000.0,
    composite: false,
};

/// Entropy source that harvests PLL phase noise from audio subsystem queries.
pub struct AudioPLLTimingSource;

/// CoreAudio FFI bindings (macOS only).
#[cfg(target_os = "macos")]
mod coreaudio {
    #[repr(C)]
    pub struct AudioObjectPropertyAddress {
        pub m_selector: u32,
        pub m_scope: u32,
        pub m_element: u32,
    }

    pub const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = 0x644F7574; // 'dOut'
    pub const AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: u32 = 0x6E737274; // 'nsrt'
    pub const AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE: u32 = 0x61737264; // 'asrd'
    pub const AUDIO_DEVICE_PROPERTY_LATENCY: u32 = 0x6C746E63; // 'ltnc'
    pub const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676C6F62; // 'glob'
    pub const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
    pub const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = 0x6F757470; // 'outp'
    pub const AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;

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
        // SAFETY: AudioObjectGetPropertyData is a CoreAudio API that reads a property
        // from the system audio object. We pass valid pointers to stack-allocated `size`
        // and `device` with correct sizes. The function writes at most `size` bytes.
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

    /// Query a device property and return the elapsed duration.
    pub fn query_device_property(device: u32, selector: u32, scope: u32) -> std::time::Duration {
        let addr = AudioObjectPropertyAddress {
            m_selector: selector,
            m_scope: scope,
            m_element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut data = [0u8; 8];
        let mut size: u32 = 8;

        let t0 = std::time::Instant::now();
        // SAFETY: AudioObjectGetPropertyData reads a property from a valid audio device.
        // `data` is an 8-byte stack buffer, and `size` is set to 8, which is sufficient
        // for all queried properties (f64 sample rate or u32 latency).
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
        t0.elapsed()
    }
}

impl EntropySource for AudioPLLTimingSource {
    fn info(&self) -> &SourceInfo {
        &AUDIO_PLL_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            coreaudio::get_default_output_device() != 0
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = n_samples;
            return Vec::new();
        }

        #[cfg(target_os = "macos")]
        {
            let device = coreaudio::get_default_output_device();
            if device == 0 {
                return Vec::new();
            }

            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            // Cycle through different property queries to exercise different
            // code paths in the audio subsystem, each crossing the PLL boundary.
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

            for i in 0..raw_count {
                let (sel, scope) = selectors[i % selectors.len()];
                let elapsed = coreaudio::query_device_property(device, sel, scope);
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
        let src = AudioPLLTimingSource;
        assert_eq!(src.name(), "audio_pll_timing");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires audio hardware
    fn collects_bytes() {
        let src = AudioPLLTimingSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
