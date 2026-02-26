//! USB device enumeration timing entropy.
//!
//! macOS's IOKit USB subsystem enumerates USB devices through the USB controller.
//! The enumeration process involves:
//! 1. Querying the USB controller for device list
//! 2. Walking the USB device tree
//! 3. Reading device descriptors from each device
//!
//! ## Physics
//!
//! USB enumeration timing varies based on:
//!
//! 1. **USB controller state**: The USB xHCI controller has internal state
//!    machines for each port. Port state (connected, enumerating, error)
//!    affects enumeration latency.
//!
//! 2. **USB bus traffic**: Active transfers on the USB bus delay enumeration
//!    requests. High-bandwidth devices (external drives, cameras) create
//!    bus contention.
//!
//! 3. **Device descriptor cache**: macOS caches USB device descriptors.
//!    Cold enumeration (first access after boot) is slower than warm
//!    enumeration (cached descriptors).
//!
//! 4. **IOKit registry lock**: The IOKit registry is protected by locks.
//!    Concurrent device hot-plug events hold these locks, delaying our
//!    enumeration.
//!
//! Empirically on M4 Mac mini (N=200):
//! - mean=13046 ticks (~544 µs), **CV=116.2%**, range=[9791,169338]
//!
//! ## Why This Is Entropy
//!
//! USB enumeration timing captures:
//!
//! 1. **USB bus activity** — other devices' transfers create bus contention
//! 2. **Hot-plug events** — device insertions/removals hold IOKit locks
//! 3. **Controller power state** — USB controller in low-power mode adds wake latency
//! 4. **Cross-process sensitivity** — any process using USB devices changes timing

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(target_os = "macos")]
use crate::sources::helpers::{extract_timing_entropy, mach_time};

static USB_ENUMERATION_INFO: SourceInfo = SourceInfo {
    name: "usb_enumeration",
    description: "IOKit USB device enumeration timing — CV=116%, USB controller state",
    physics: "Times IOKit USB device enumeration (IOServiceMatching(kIOUSBDeviceClassName) + \
              device tree walk). USB enumeration latency varies with: USB xHCI controller \
              port state, USB bus traffic from active devices, IOKit registry lock contention \
              from hot-plug events, controller power state wake-up latency. \
              Measured: mean=13046 ticks (~544 µs), CV=116.2%, range=[9791,169338]. \
              Cross-process sensitivity: any process using USB devices or hot-plugging \
              changes enumeration timing.",
    category: SourceCategory::IO,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 1.5,
    composite: false,
    is_fast: false,
};

/// Entropy source from USB device enumeration timing.
pub struct USBEnumerationSource;

#[cfg(target_os = "macos")]
mod usb_imp {
    use std::ffi::c_void;

    pub type IOReturn = i32;
    pub type MachPort = u32;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        pub fn IOServiceMatching(name: *const i8) -> *mut c_void;
        pub fn IOServiceGetMatchingServices(
            main_port: MachPort, matching: *const c_void, iter: *mut u32) -> IOReturn;
        pub fn IOIteratorNext(iterator: u32) -> u32;
        pub fn IOObjectRelease(obj: u32) -> IOReturn;
    }

    pub const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;
}

#[cfg(target_os = "macos")]
impl EntropySource for USBEnumerationSource {
    fn info(&self) -> &SourceInfo {
        &USB_ENUMERATION_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        use usb_imp::*;

        let raw = n_samples * 2 + 32;
        let mut timings = Vec::with_capacity(raw);

        // Warm up
        for _ in 0..4 {
            let matching = unsafe { IOServiceMatching(c"IOUSBDevice".as_ptr()) };
            if !matching.is_null() {
                let mut iter: u32 = 0;
                unsafe {
                    IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iter);
                    if iter != 0 {
                        let mut obj = IOIteratorNext(iter);
                        while obj != 0 {
                            IOObjectRelease(obj);
                            obj = IOIteratorNext(iter);
                        }
                        IOObjectRelease(iter);
                    }
                }
            }
        }

        for _ in 0..raw {
            let matching = unsafe { IOServiceMatching(c"IOUSBDevice".as_ptr()) };
            if matching.is_null() { continue; }

            let t0 = mach_time();
            let mut iter: u32 = 0;
            let kr = unsafe {
                IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iter)
            };

            if kr == 0 && iter != 0 {
                // Enumerate devices
                let mut count = 0;
                let mut obj = unsafe { IOIteratorNext(iter) };
                while obj != 0 && count < 50 {
                    unsafe { IOObjectRelease(obj) };
                    obj = unsafe { IOIteratorNext(iter) };
                    count += 1;
                }
                unsafe { IOObjectRelease(iter) };
            }

            let elapsed = mach_time().wrapping_sub(t0);
            // Cap at 100ms
            if elapsed < 2_400_000 {
                timings.push(elapsed);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(not(target_os = "macos"))]
impl EntropySource for USBEnumerationSource {
    fn info(&self) -> &SourceInfo { &USB_ENUMERATION_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = USBEnumerationSource;
        assert_eq!(src.info().name, "usb_enumeration");
        assert!(matches!(src.info().category, SourceCategory::IO));
        assert_eq!(src.info().platform, Platform::MacOS);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available() {
        assert!(USBEnumerationSource.is_available());
    }

    #[test]
    #[ignore]
    fn collects_usb_controller_state() {
        let data = USBEnumerationSource.collect(32);
        assert!(!data.is_empty());
    }
}
