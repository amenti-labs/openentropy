//! USB IORegistry query timing — crystal oscillator phase noise.
//!
//! USB host controllers use a crystal oscillator for the 1 kHz SOF signal.
//! By rapidly querying USB device properties via IOKit/IORegistry, we measure
//! timing jitter from the USB bus arbitration and clock domain crossing.
//!
//! The crystal has thermally-driven phase noise from:
//! - Mechanical vibrations of quartz lattice (phonon noise)
//! - Load capacitance thermal noise
//! - Oscillator circuit Johnson-Nyquist noise
//!
//! PoC measured H∞ ≈ 3.7 bits/byte for USB device property queries.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

static USB_TIMING_INFO: SourceInfo = SourceInfo {
    name: "usb_timing",
    description: "USB IORegistry query timing jitter from crystal oscillator phase noise",
    physics: "Rapidly queries USB device properties via IOKit. Each query traverses the \
              USB host controller\u{2019}s IORegistry tree, crossing the USB crystal oscillator / \
              CPU clock domain boundary. The USB crystal has thermally-driven phase noise \
              from quartz lattice phonon excitations, load capacitance Johnson-Nyquist noise, \
              and oscillator circuit thermal fluctuations. Timing jitter also includes USB \
              bus arbitration contention. \
              PoC measured H\u{221e} \u{2248} 3.7 bits/byte.",
    category: SourceCategory::Frontier,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 1500.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from USB IORegistry queries.
pub struct USBTimingSource;

/// IOKit FFI for USB device enumeration and property reads.
#[cfg(target_os = "macos")]
mod iokit {
    use std::ffi::c_void;

    // IOKit types
    pub type IOReturn = i32;
    pub type MachPort = u32;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        pub fn IOServiceGetMatchingServices(
            main_port: MachPort,
            matching: *const c_void,
            existing: *mut u32,
        ) -> IOReturn;

        pub fn IOServiceMatching(name: *const u8) -> *const c_void;

        pub fn IOIteratorNext(iterator: u32) -> u32;

        pub fn IOObjectRelease(object: u32) -> IOReturn;

        pub fn IORegistryEntryCreateCFProperty(
            entry: u32,
            key: *const c_void,
            allocator: *const c_void,
            options: u32,
        ) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFRelease(cf: *const c_void);

        pub fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *const c_void;
    }

    /// kIOMainPortDefault is 0 on modern macOS.
    pub const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;

    pub const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    pub fn cfstr(s: &[u8]) -> *const c_void {
        unsafe { CFStringCreateWithCString(std::ptr::null(), s.as_ptr() as *const i8, K_CF_STRING_ENCODING_UTF8) }
    }

    /// Find USB devices and return their IOKit service handles.
    pub fn find_usb_devices() -> Vec<u32> {
        let mut devices = Vec::new();
        let matching = unsafe {
            IOServiceMatching(c"IOUSBHostDevice".as_ptr() as *const u8)
        };
        if matching.is_null() {
            return devices;
        }

        let mut iter: u32 = 0;
        let kr = unsafe { IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iter) };
        if kr != 0 || iter == 0 {
            return devices;
        }

        loop {
            let service = unsafe { IOIteratorNext(iter) };
            if service == 0 {
                break;
            }
            devices.push(service);
        }
        unsafe { IOObjectRelease(iter) };
        devices
    }

    /// Query a device property and return the elapsed time.
    pub fn query_device_property(device: u32, key: &[u8]) -> std::time::Duration {
        let cf_key = cfstr(key);
        let t0 = std::time::Instant::now();
        let prop = unsafe {
            IORegistryEntryCreateCFProperty(device, cf_key, std::ptr::null(), 0)
        };
        let elapsed = t0.elapsed();
        if !prop.is_null() {
            unsafe { CFRelease(prop) };
        }
        unsafe { CFRelease(cf_key) };
        elapsed
    }
}

impl EntropySource for USBTimingSource {
    fn info(&self) -> &SourceInfo {
        &USB_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            !iokit::find_usb_devices().is_empty()
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
            let devices = iokit::find_usb_devices();
            if devices.is_empty() {
                return Vec::new();
            }

            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            let property_keys: &[&[u8]] = &[
                b"sessionID\0",
                b"USB Address\0",
            ];

            for i in 0..raw_count {
                let device = devices[i % devices.len()];
                let key = property_keys[i % property_keys.len()];
                let elapsed = iokit::query_device_property(device, key);
                timings.push(elapsed.as_nanos() as u64);
            }

            // Release device handles.
            for device in &devices {
                unsafe { iokit::IOObjectRelease(*device) };
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
        let src = USBTimingSource;
        assert_eq!(src.info().name, "usb_timing");
        assert!(matches!(src.info().category, SourceCategory::Frontier));
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires USB devices
    fn collects_bytes() {
        let src = USBTimingSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
