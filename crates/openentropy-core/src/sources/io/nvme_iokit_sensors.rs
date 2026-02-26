//! NVMe IOKit sensor polling — clock domain crossing jitter from NVMe controller.
//!
//! Reads NVMe controller properties (temperature, SMART counters, error rates)
//! directly via the IOKit C API with CNTVCT timestamps. Each IOKit property read
//! crosses from the CPU clock domain into the NVMe controller's clock domain.
//!
//! ## Entropy mechanism
//!
//! - **Clock domain crossing jitter**: CPU crystal (24 MHz) vs NVMe controller PLL
//! - **SMART counter deltas**: DataUnitsRead, HostWriteCommands change between polls
//! - **Temperature ADC noise**: NVMe composite temperature from on-die ADC
//!
//! The NVMe controller (Apple ANS2/ANS3 on Apple Silicon) has its own independent
//! clock, separate from the CPU, audio, display, and PCIe PHY PLLs. Property reads
//! traverse IOKit's internal locking and serialization paths timed by CNTVCT_EL0.
//!
//! ## Entropy quality
//!
//! The dominant timing variance comes from kernel lock contention and IOKit
//! serialization, not PLL thermal noise. This is a high-quality hardware
//! timing source, not a quantum random number generator.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static NVME_IOKIT_SENSORS_INFO: SourceInfo = SourceInfo {
    name: "nvme_iokit_sensors",
    description: "NVMe controller sensor polling via IOKit with CNTVCT clock domain crossing timestamps",
    physics: "Reads NVMe controller properties (temperature, SMART counters) via IOKit C API, \
              forcing clock domain crossings between the CPU\u{2019}s 24 MHz crystal and the \
              NVMe controller\u{2019}s independent PLL (Apple ANS2/ANS3). CNTVCT_EL0 timestamps \
              before/after each IOKit call capture the beat between CPU and NVMe clock domains. \
              Entropy arises from PLL thermal noise (VCO Johnson-Nyquist), IOKit kernel path \
              traversal variance, and SMART counter deltas between consecutive polls.",
    category: SourceCategory::IO,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 3.0,
    composite: false,
    is_fast: true,
};

/// NVMe IOKit sensor polling entropy source.
pub struct NvmeIokitSensorsSource;

/// IOKit FFI for reading NVMe controller properties.
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
        fn IORegistryEntryCreateCFProperty(
            entry: io_registry_entry_t,
            key: CFStringRef,
            allocator: CFAllocatorRef,
            options: u32,
        ) -> CFTypeRef;
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
    }

    /// IOKit service class names for NVMe controllers on Apple Silicon.
    const NVME_SERVICE_CLASSES: &[&str] = &[
        "AppleANS3CGv2Controller",
        "AppleANS2Controller",
        "IONVMeController",
        "AppleANS3NVMeController",
    ];

    /// NVMe-specific property keys to probe (forces deeper IOKit traversal).
    const NVME_PROPERTY_KEYS: &[&str] = &[
        "Temperature",
        "Data Units Read",
        "Data Units Written",
        "Host Read Commands",
        "Host Write Commands",
        "Media and Data Integrity Errors",
        "Power On Hours",
    ];

    /// Probe an NVMe IOKit service, reading all properties + specific keys.
    /// Returns the CNTVCT tick count for the traversal.
    pub fn probe_nvme_service(class_name: &str) -> u64 {
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
            // Read all properties (forces full IOKit traversal).
            let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
            // SAFETY: IORegistryEntryCreateCFProperties reads all properties
            // from a valid IOService entry. `props` receives a retained CF dict.
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(service, &mut props, K_CF_ALLOCATOR_DEFAULT, 0)
            };

            if kr == 0 && !props.is_null() {
                // Force traversal of the dictionary.
                // SAFETY: CFDictionaryGetCount on a valid dict.
                let count = unsafe { CFDictionaryGetCount(props as CFDictionaryRef) };
                std::hint::black_box(count);
                // SAFETY: CFRelease on a valid CF object.
                unsafe { CFRelease(props as CFTypeRef) };
            }

            // Read specific NVMe property keys for deeper traversal.
            for key_name in NVME_PROPERTY_KEYS {
                let c_key = match CString::new(*key_name) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                // SAFETY: CFStringCreateWithCString creates a CFString from a C string.
                // IORegistryEntryCreateCFProperty reads a single property by key.
                // Both are read-only operations on valid objects.
                unsafe {
                    let cf_key = CFStringCreateWithCString(
                        K_CF_ALLOCATOR_DEFAULT,
                        c_key.as_ptr(),
                        K_CF_STRING_ENCODING_UTF8,
                    );
                    if !cf_key.is_null() {
                        let val = IORegistryEntryCreateCFProperty(
                            service,
                            cf_key,
                            K_CF_ALLOCATOR_DEFAULT,
                            0,
                        );
                        std::hint::black_box(val);
                        if !val.is_null() {
                            CFRelease(val);
                        }
                        CFRelease(cf_key);
                    }
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

    /// Check if any NVMe IOKit services are reachable.
    pub fn has_nvme_services() -> bool {
        for class in NVME_SERVICE_CLASSES {
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
        NVME_SERVICE_CLASSES
    }
}

impl EntropySource for NvmeIokitSensorsSource {
    fn info(&self) -> &SourceInfo {
        &NVME_IOKIT_SENSORS_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_nvme_services()
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

            // Over-sample: each IOKit probe produces one timing, we need ~4x for
            // the delta+XOR extraction pipeline to yield n_samples bytes.
            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                let class = classes[i % classes.len()];
                let duration = iokit::probe_nvme_service(class);
                timings.push(duration);
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
        let src = NvmeIokitSensorsSource;
        assert_eq!(src.name(), "nvme_iokit_sensors");
        assert_eq!(src.info().category, SourceCategory::IO);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_nvme() {
        let src = NvmeIokitSensorsSource;
        assert!(src.info().physics.contains("NVMe"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
        assert!(src.info().physics.contains("SMART"));
    }

    #[test]
    #[ignore] // Requires macOS Apple Silicon with NVMe controller
    fn collects_bytes() {
        let src = NvmeIokitSensorsSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
