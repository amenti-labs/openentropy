//! Shared CoreAudio FFI bindings used by audio-based entropy sources.
//!
//! Provides the minimal CoreAudio bindings needed to query audio device
//! properties. Used by `audio_pll_timing` to avoid duplicating FFI code.

#[cfg(target_os = "macos")]
pub use inner::*;

#[cfg(target_os = "macos")]
mod inner {
    #[repr(C)]
    pub struct AudioObjectPropertyAddress {
        pub m_selector: u32,
        pub m_scope: u32,
        pub m_element: u32,
    }

    pub const AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    pub const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = 0x644F7574; // 'dOut'
    pub const AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: u32 = 0x6E737274; // 'nsrt'
    pub const AUDIO_DEVICE_PROPERTY_ACTUAL_SAMPLE_RATE: u32 = 0x61737274; // 'asrt'
    pub const AUDIO_DEVICE_PROPERTY_LATENCY: u32 = 0x6C746E63; // 'ltnc'
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

    /// Query a device property and return the elapsed duration.
    ///
    /// Used by `audio_pll_timing` to measure timing jitter from property queries.
    pub fn query_device_property_timed(
        device: u32,
        selector: u32,
        scope: u32,
    ) -> std::time::Duration {
        let addr = AudioObjectPropertyAddress {
            m_selector: selector,
            m_scope: scope,
            m_element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut data = [0u8; 8];
        let mut size: u32 = 8;

        let t0 = std::time::Instant::now();
        // SAFETY: AudioObjectGetPropertyData reads a property from a valid audio device.
        // `data` is an 8-byte stack buffer sufficient for all queried properties
        // (f64 sample rate or u32 latency).
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
