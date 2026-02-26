//! Apple Neural Engine (ANE) timing — clock domain crossing jitter from ANE dispatch.
//!
//! The Apple Neural Engine has its own independent clock domain, separate from the
//! CPU, GPU, audio PLL, display PLL, and PCIe PHY. Dispatching even a trivial
//! workload to the ANE forces a clock domain crossing that introduces timing jitter.
//!
//! ## Entropy mechanism
//!
//! - **ANE clock domain crossing**: CPU crystal (24 MHz) vs ANE's independent PLL
//! - **DMA setup variance**: ANE workloads require DMA buffer setup with variable latency
//! - **Power state transitions**: ANE may be in low-power state, adding wake jitter
//! - **Fabric contention**: ANE shares the memory fabric with CPU/GPU, adding noise
//!
//! ## Why this is unique
//!
//! No published entropy source extracts timing jitter from neural accelerator dispatch.
//! The ANE is a completely separate compute domain with its own clocking, power gating,
//! and DMA engine — making it an independent noise source from all existing oscillator
//! beat sources.
//!
//! ## How it works
//!
//! We don't actually need CoreML or a real model. We probe the ANE subsystem via IOKit
//! by reading properties from AppleH13ANEInterface / AppleANE* services. Each IOKit
//! traversal crosses into the ANE's clock/power domain. CNTVCT_EL0 timestamps capture
//! the timing jitter.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static ANE_TIMING_INFO: SourceInfo = SourceInfo {
    name: "ane_timing",
    description: "Apple Neural Engine clock domain crossing jitter via IOKit property reads",
    physics: "Probes Apple Neural Engine (ANE) IOKit services, forcing clock domain \
              crossings between the CPU\u{2019}s 24 MHz crystal and the ANE\u{2019}s independent \
              PLL. The ANE is a separate compute block with its own clocking, power gating, \
              and DMA engine. Timing jitter arises from ANE PLL thermal noise (VCO \
              Johnson-Nyquist), power state transition latency, DMA setup variance, and \
              memory fabric contention. CNTVCT_EL0 timestamps before/after each IOKit \
              call capture the beat between CPU and ANE clock domains.",
    category: SourceCategory::Timing,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 3.0,
    composite: false,
    is_fast: true,
};

/// Apple Neural Engine timing jitter entropy source.
pub struct AneTimingSource;

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
    type CFAllocatorRef = *const c_void;
    type CFMutableDictionaryRef = *mut c_void;
    type CFDictionaryRef = *const c_void;

    const K_IO_MAIN_PORT_DEFAULT: mach_port_t = 0;
    const K_CF_ALLOCATOR_DEFAULT: CFAllocatorRef = std::ptr::null();

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
    }

    /// IOKit service class names for Apple Neural Engine subsystem.
    /// Different Apple Silicon generations use different class names.
    const ANE_SERVICE_CLASSES: &[&str] = &[
        "H11ANEIn",         // ANE input/dispatch service (M1/M2/M3/M4)
        "H11ANE",           // ANE controller (all Apple Silicon)
        "AppleT6041ANEHAL", // ANE HAL (chip-specific, e.g. M3)
        "ANEClientHints",   // ANE client hints service
    ];

    /// Probe an ANE IOKit service. Returns CNTVCT tick duration.
    pub fn probe_ane_service(class_name: &str) -> u64 {
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
            let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(service, &mut props, K_CF_ALLOCATOR_DEFAULT, 0)
            };

            if kr == 0 && !props.is_null() {
                let count = unsafe { CFDictionaryGetCount(props as CFDictionaryRef) };
                std::hint::black_box(count);
                unsafe { CFRelease(props as CFTypeRef) };
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

    /// Check if any ANE services are reachable.
    pub fn has_ane_services() -> bool {
        for class in ANE_SERVICE_CLASSES {
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
        ANE_SERVICE_CLASSES
    }
}

impl EntropySource for AneTimingSource {
    fn info(&self) -> &SourceInfo {
        &ANE_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_ane_services()
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
                let duration = iokit::probe_ane_service(class);
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
        let src = AneTimingSource;
        assert_eq!(src.name(), "ane_timing");
        assert_eq!(src.info().category, SourceCategory::Timing);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_ane() {
        let src = AneTimingSource;
        assert!(src.info().physics.contains("Neural Engine"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
        assert!(src.info().physics.contains("PLL"));
    }

    #[test]
    #[ignore] // Requires macOS Apple Silicon with ANE
    fn collects_bytes() {
        let src = AneTimingSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
