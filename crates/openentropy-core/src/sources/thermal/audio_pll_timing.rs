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
#[cfg(target_os = "macos")]
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
    entropy_rate_estimate: 3.0,
    composite: false,
    is_fast: true,
};

/// Entropy source that harvests PLL phase noise from audio subsystem queries.
pub struct AudioPLLTimingSource;

impl EntropySource for AudioPLLTimingSource {
    fn info(&self) -> &SourceInfo {
        &AUDIO_PLL_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            super::coreaudio_ffi::get_default_output_device() != 0
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
            Vec::new()
        }

        #[cfg(target_os = "macos")]
        {
            use super::coreaudio_ffi;

            let device = coreaudio_ffi::get_default_output_device();
            if device == 0 {
                return Vec::new();
            }

            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            // Cycle through different property queries to exercise different
            // code paths in the audio subsystem, each crossing the PLL boundary.
            let selectors = [
                (
                    coreaudio_ffi::AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE,
                    coreaudio_ffi::AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                ),
                (
                    coreaudio_ffi::AUDIO_DEVICE_PROPERTY_LATENCY,
                    coreaudio_ffi::AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
                ),
                (
                    coreaudio_ffi::AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE,
                    coreaudio_ffi::AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                ),
            ];

            for i in 0..raw_count {
                let (sel, scope) = selectors[i % selectors.len()];
                let elapsed = coreaudio_ffi::query_device_property_timed(device, sel, scope);
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
