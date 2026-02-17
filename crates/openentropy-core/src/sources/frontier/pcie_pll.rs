//! PCIe PHY PLL timing — clock domain crossing jitter from PCIe subsystem.
//!
//! Apple Silicon Macs have multiple independent PLLs for the PCIe/Thunderbolt
//! physical layer (visible in IORegistry as CIO3PLL, AUSPLL, ACIOPHY_PLL, etc.).
//! These PLLs are electrically independent from the CPU crystal, audio PLL, RTC,
//! and display PLL.
//!
//! ## Entropy mechanism
//!
//! We probe the PCIe subsystem via IOKit, reading properties from Thunderbolt/
//! PCIe IOService entries. Each IOKit property read crosses from the CPU clock
//! domain into the PCIe PHY's clock domain (or at minimum traverses IOKit's
//! internal locking and serialization which is driven by PCIe bus timing).
//!
//! The PCIe PHY PLLs have thermal noise from:
//! - VCO transistor Johnson-Nyquist noise
//! - Spread-spectrum clocking (SSC) modulation if enabled
//! - Lane-to-lane skew from manufacturing variation
//! - Reference clock phase noise
//!
//! ## Why this is unique
//!
//! - **Fourth independent oscillator domain**: separate from CPU, audio, RTC, display
//! - **Multiple PLLs**: each PCIe lane has its own clock recovery PLL
//! - **Spread-spectrum clocking**: PCIe often uses SSC which intentionally modulates
//!   the clock, adding an extra noise dimension
//! - **Deep kernel path**: IOKit traversal exercises more kernel subsystems than
//!   simple syscalls

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static PCIE_PLL_INFO: SourceInfo = SourceInfo {
    name: "pcie_pll",
    description: "PCIe PHY PLL jitter from IOKit property reads across PCIe clock domains",
    physics: "Reads IOKit properties from PCIe/Thunderbolt IOService entries, forcing \
              clock domain crossings into the PCIe PHY\u{2019}s independent PLL oscillators \
              (CIO3PLL, AUSPLL, etc.). These PLLs are electrically separate from the \
              CPU crystal, audio PLL, RTC crystal, and display PLL. Phase noise arises \
              from VCO thermal noise, spread-spectrum clocking modulation, and lane skew. \
              CNTVCT_EL0 timestamps before/after each IOKit call capture the beat between \
              CPU crystal and PCIe clock domain.",
    category: SourceCategory::Thermal,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

/// PCIe PHY PLL timing jitter entropy source.
pub struct PciePllSource;

/// IOKit FFI for reading PCIe/Thunderbolt service properties.
#[cfg(target_os = "macos")]
mod iokit {
    use crate::sources::helpers::read_cntvct;
    use std::ffi::{CString, c_char, c_void};

    type IOReturn = i32;

    #[allow(non_camel_case_types)]
    type mach_port_t = u32;
    #[allow(non_camel_case_types)]
    type io_iterator_t = u32;
    #[allow(non_camel_case_types)]
    type io_object_t = u32;
    #[allow(non_camel_case_types)]
    type io_registry_entry_t = u32;

    // CFTypeRef aliases
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFMutableDictionaryRef = *mut c_void;
    type CFDictionaryRef = *const c_void;

    const K_IO_MAIN_PORT_DEFAULT: mach_port_t = 0;
    const K_CF_ALLOCATOR_DEFAULT: CFAllocatorRef = std::ptr::null();
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    #[link(name = "IOKit", kind = "framework")]
    #[allow(clashing_extern_declarations)]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
        fn IOServiceGetMatchingServices(
            main_port: mach_port_t,
            matching: CFDictionaryRef,
            existing: *mut io_iterator_t,
        ) -> IOReturn;
        fn IOIteratorNext(iterator: io_iterator_t) -> io_object_t;
        fn IORegistryEntryCreateCFProperties(
            entry: io_registry_entry_t,
            properties: *mut CFMutableDictionaryRef,
            allocator: CFAllocatorRef,
            options: u32,
        ) -> IOReturn;
        fn IOObjectRelease(object: io_object_t) -> IOReturn;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFDictionaryGetCount(dict: CFDictionaryRef) -> isize;
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    }

    /// IOKit service class names that touch the PCIe/Thunderbolt clock domains.
    /// Each represents a different clock domain or subsystem path.
    const PCIE_SERVICE_CLASSES: &[&str] = &[
        "AppleThunderboltHAL",
        "IOPCIDevice",
        "IOThunderboltController",
        "IONVMeController",
        "AppleUSBHostController",
    ];

    /// Probe an IOKit service matching the given class name.
    /// Returns the time (in CNTVCT ticks) spent traversing IOKit.
    #[cfg(target_os = "macos")]
    pub fn probe_service(class_name: &str) -> u64 {
        let c_name = match CString::new(class_name) {
            Ok(s) => s,
            Err(_) => return 0,
        };

        let counter_before = read_cntvct();

        // SAFETY: IOServiceMatching creates a matching dictionary from a class name.
        // The returned dictionary is consumed by IOServiceGetMatchingServices.
        let matching = unsafe { IOServiceMatching(c_name.as_ptr()) };
        if matching.is_null() {
            return read_cntvct().wrapping_sub(counter_before);
        }

        let mut iterator: io_iterator_t = 0;
        // SAFETY: IOServiceGetMatchingServices consumes the matching dict
        // and writes a valid iterator to `iterator`.
        let kr = unsafe {
            IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iterator)
        };

        if kr != 0 {
            return read_cntvct().wrapping_sub(counter_before);
        }

        // Get first matching service.
        // SAFETY: IOIteratorNext returns the next service or 0 if exhausted.
        let service = unsafe { IOIteratorNext(iterator) };

        if service != 0 {
            let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
            // SAFETY: IORegistryEntryCreateCFProperties reads all properties
            // from a valid IOService entry. `props` receives a retained CF dict.
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(service, &mut props, K_CF_ALLOCATOR_DEFAULT, 0)
            };

            if kr == 0 && !props.is_null() {
                // Read property count to force traversal of the dictionary.
                // SAFETY: CFDictionaryGetCount on a valid dict.
                let count = unsafe { CFDictionaryGetCount(props as CFDictionaryRef) };
                std::hint::black_box(count);

                // Try to read a specific property to go deeper into the subsystem.
                let key_name = CString::new("IOPCIExpressLinkStatus").unwrap_or_default();
                // SAFETY: CFStringCreateWithCString and CFDictionaryGetValue are
                // read-only CF operations on valid objects.
                unsafe {
                    let key = CFStringCreateWithCString(
                        K_CF_ALLOCATOR_DEFAULT,
                        key_name.as_ptr(),
                        K_CF_STRING_ENCODING_UTF8,
                    );
                    if !key.is_null() {
                        let val = CFDictionaryGetValue(props as CFDictionaryRef, key);
                        std::hint::black_box(val);
                        CFRelease(key);
                    }
                    CFRelease(props as CFTypeRef);
                }
            }

            // SAFETY: IOObjectRelease releases a valid IOKit object.
            unsafe {
                IOObjectRelease(service);
            }
        }

        // Release iterator.
        // SAFETY: IOObjectRelease releases a valid iterator.
        unsafe {
            IOObjectRelease(iterator);
        }

        read_cntvct().wrapping_sub(counter_before)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn probe_service(_class_name: &str) -> u64 {
        0
    }

    /// Check if any PCIe services are reachable.
    pub fn has_pcie_services() -> bool {
        for class in PCIE_SERVICE_CLASSES {
            let c_name = match CString::new(*class) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // SAFETY: IOServiceMatching + IOServiceGetMatchingServices are read-only lookups.
            unsafe {
                let matching = IOServiceMatching(c_name.as_ptr());
                if matching.is_null() {
                    continue;
                }
                let mut iter: io_iterator_t = 0;
                let kr = IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iter);
                if kr == 0 {
                    let svc = IOIteratorNext(iter);
                    IOObjectRelease(iter);
                    if svc != 0 {
                        IOObjectRelease(svc);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn service_classes() -> &'static [&'static str] {
        PCIE_SERVICE_CLASSES
    }
}

impl EntropySource for PciePllSource {
    fn info(&self) -> &SourceInfo {
        &PCIE_PLL_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_pcie_services()
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
            let classes = iokit::service_classes();
            if classes.is_empty() {
                return Vec::new();
            }

            let raw_count = n_samples * 4 + 64;
            let mut beats: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                let class = classes[i % classes.len()];
                let duration = iokit::probe_service(class);
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
        let src = PciePllSource;
        assert_eq!(src.name(), "pcie_pll");
        assert_eq!(src.info().category, SourceCategory::Thermal);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_pcie() {
        let src = PciePllSource;
        assert!(src.info().physics.contains("PCIe"));
        assert!(src.info().physics.contains("PLL"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn collects_bytes() {
        let src = PciePllSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
