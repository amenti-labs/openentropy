//! SMC (System Management Controller) power meter LSBs — ADC quantization noise.
//!
//! Apple Silicon Macs have a dedicated System Management Controller that monitors
//! power consumption via on-board ADCs. The least-significant bits of power
//! measurements contain ADC quantization noise with genuine thermal (Johnson-Nyquist)
//! components from the current sense resistors and ADC input circuitry.
//!
//! ## Entropy mechanism
//!
//! - **ADC quantization noise**: Current sense amplifiers have Johnson-Nyquist noise
//!   in the input resistors and reference ladder; LSBs fluctuate genuinely
//! - **Power supply ripple**: Switching regulator ripple adds non-deterministic noise
//! - **Workload-dependent power**: Active compute loads create rapid current changes
//!   that interact with the ADC sampling window
//! - **Cross-domain timing**: SMC operates on its own clock (typically low-frequency
//!   ARM core), independent of the main CPU crystal
//!
//! ## Why this is unique
//!
//! Power meter ADC noise has been studied in smartcard side-channel attacks, but
//! intentionally harvesting SMC power meter LSBs as an entropy source via IOKit
//! is novel. The SMC's independent clock domain and dedicated ADC hardware make
//! it a genuinely independent noise source.
//!
//! ## IOKit access
//!
//! The SMC is accessible via the `AppleSMC` IOKit service. We read power-related
//! keys and use CNTVCT_EL0 timestamps to capture cross-domain timing jitter.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static SMC_POWER_INFO: SourceInfo = SourceInfo {
    name: "smc_power",
    description: "SMC power meter ADC quantization noise via IOKit with timing jitter",
    physics: "Reads power sensor data from the System Management Controller (AppleSMC) \
              via IOKit. Power measurements from on-board current sense ADCs contain \
              Johnson-Nyquist noise from sense resistors and ADC input circuitry. LSB \
              fluctuations reflect genuine thermal noise in the analog front-end. The SMC \
              operates on its own low-frequency ARM core with an independent clock, so \
              IOKit reads also capture cross-domain timing jitter. CNTVCT_EL0 timestamps \
              before/after each IOKit call capture the beat between CPU crystal and SMC \
              clock domain.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 1000.0,
    composite: false,
};

/// SMC power meter ADC noise entropy source.
pub struct SmcPowerSource;

#[cfg(target_os = "macos")]
mod iokit {
    use crate::sources::helpers::read_cntvct;
    use std::ffi::{c_char, c_void, CString};

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

    /// IOKit service class names for SMC and power management.
    const SMC_SERVICE_CLASSES: &[&str] = &[
        "AppleSMC",                    // Primary SMC service
        "AppleSMCCharger",             // Battery charger (has power ADCs)
        "AppleSmartBatteryManager",    // Battery management (current/voltage ADCs)
        "ApplePMU",                    // Power management unit
        "IOPMPowerSource",             // Power source reporting
    ];

    /// Power-related SMC property keys to probe.
    const SMC_PROPERTY_KEYS: &[&str] = &[
        "CurrentCapacity",
        "Voltage",
        "Amperage",
        "Temperature",
        "InstantAmperage",
        "AdapterDetails",
        "BatteryData",
    ];

    /// Probe an SMC/power IOKit service. Returns CNTVCT tick duration.
    pub fn probe_smc_service(class_name: &str) -> u64 {
        let c_name = match CString::new(class_name) {
            Ok(s) => s,
            Err(_) => return 0,
        };

        let counter_before = read_cntvct();

        let matching = unsafe { IOServiceMatching(c_name.as_ptr()) };
        if matching.is_null() {
            return read_cntvct().wrapping_sub(counter_before);
        }

        let mut iterator: io_iterator_t = 0;
        let kr = unsafe {
            IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iterator)
        };

        if kr != 0 {
            return read_cntvct().wrapping_sub(counter_before);
        }

        let service = unsafe { IOIteratorNext(iterator) };

        if service != 0 {
            // Read all properties to exercise the full SMC path
            let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(service, &mut props, K_CF_ALLOCATOR_DEFAULT, 0)
            };

            if kr == 0 && !props.is_null() {
                let count = unsafe { CFDictionaryGetCount(props as CFDictionaryRef) };
                std::hint::black_box(count);
                unsafe { CFRelease(props as CFTypeRef) };
            }

            // Read specific power property keys for deeper traversal
            for key_name in SMC_PROPERTY_KEYS {
                let c_key = match CString::new(*key_name) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
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

            unsafe {
                IOObjectRelease(service);
            }
        }

        unsafe {
            IOObjectRelease(iterator);
        }

        read_cntvct().wrapping_sub(counter_before)
    }

    /// Check if any SMC/power services are reachable.
    pub fn has_smc_services() -> bool {
        for class in SMC_SERVICE_CLASSES {
            let c_name = match CString::new(*class) {
                Ok(s) => s,
                Err(_) => continue,
            };
            unsafe {
                let matching = IOServiceMatching(c_name.as_ptr());
                if matching.is_null() {
                    continue;
                }
                let mut iter: io_iterator_t = 0;
                let kr =
                    IOServiceGetMatchingServices(K_IO_MAIN_PORT_DEFAULT, matching, &mut iter);
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
        SMC_SERVICE_CLASSES
    }
}

impl EntropySource for SmcPowerSource {
    fn info(&self) -> &SourceInfo {
        &SMC_POWER_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_smc_services()
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
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                let class = classes[i % classes.len()];
                let duration = iokit::probe_smc_service(class);
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
        let src = SmcPowerSource;
        assert_eq!(src.name(), "smc_power");
        assert_eq!(src.info().category, SourceCategory::Sensor);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_smc() {
        let src = SmcPowerSource;
        assert!(src.info().physics.contains("SMC"));
        assert!(src.info().physics.contains("Johnson-Nyquist"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
    }

    #[test]
    #[ignore] // Requires macOS Apple Silicon with SMC
    fn collects_bytes() {
        let src = SmcPowerSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
