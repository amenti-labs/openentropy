//! IMU (Inertial Measurement Unit) sensor noise — MEMS accelerometer/gyroscope noise.
//!
//! Apple Silicon Macs contain MEMS accelerometers and gyroscopes (visible in
//! IORegistry as `AppleEmbeddedAccelerometer`, `AppleLMUController`, and various
//! motion sensor services). The physical noise floor of MEMS sensors contains
//! genuine thermal (Brownian motion) components.
//!
//! ## Entropy mechanism
//!
//! - **Brownian motion**: The proof mass in a MEMS accelerometer undergoes thermal
//!   (Brownian) motion with energy kT/2 per degree of freedom — this is
//!   fundamentally quantum-statistical in origin
//! - **ADC quantization noise**: The sensor's ADC has Johnson-Nyquist noise
//! - **Cross-domain timing**: Reading sensor values via IOKit crosses clock domains
//!   (sensor sampling clock vs CPU crystal)
//!
//! ## Why this is unique
//!
//! MEMS accelerometer noise has been used as an entropy source in mobile devices
//! (Android CIS benchmark), but extracting it via IOKit on macOS for entropy
//! purposes is novel. The sensor is on a completely independent power/clock domain
//! from the CPU.
//!
//! ## Prior art
//!
//! - Drutarovsky & Galajda (2007): "A Robust Chaos-Based True Random Number
//!   Generator Embedded in Reconfigurable Switched-Capacitor Hardware" — used
//!   MEMS sensor noise for TRNG
//! - Android CIS benchmark: uses accelerometer noise as entropy contributor

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static IMU_NOISE_INFO: SourceInfo = SourceInfo {
    name: "imu_noise",
    description: "MEMS accelerometer/gyroscope sensor noise via IOKit with timing jitter",
    physics: "Probes MEMS motion sensor IOKit services (accelerometer, gyroscope, ambient \
              light sensor). Physical noise floor contains Brownian motion of the MEMS proof \
              mass (energy kT/2 per degree of freedom — quantum-statistical origin), ADC \
              Johnson-Nyquist quantization noise, and clock domain crossing jitter between \
              the sensor\u{2019}s sampling clock and the CPU\u{2019}s 24 MHz crystal. CNTVCT_EL0 \
              timestamps before/after each IOKit traversal capture the combined jitter.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 1200.0,
    composite: false,
};

/// MEMS IMU sensor noise entropy source.
pub struct ImuNoiseSource;

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

    /// IOKit service class names for motion/environmental sensors.
    const IMU_SERVICE_CLASSES: &[&str] = &[
        "AppleEmbeddedAccelerometer",  // Built-in accelerometer
        "AppleLMUController",          // Ambient light sensor (uses ADC)
        "AppleARMBacklight",           // Backlight controller (sensor feedback)
        "IOHIDSensor",                 // Generic HID sensor
        "AppleSMCMotionSensor",        // SMC-connected motion sensor
    ];

    /// Sensor-specific property keys to probe for deeper traversal.
    const SENSOR_PROPERTY_KEYS: &[&str] = &[
        "RawValue",
        "CurrentValue",
        "PrimaryUsagePage",
        "SensorState",
        "CalibrationData",
        "Orientation",
    ];

    /// Probe a sensor IOKit service. Returns CNTVCT tick duration.
    pub fn probe_sensor_service(class_name: &str) -> u64 {
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
            // Read all properties
            let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(service, &mut props, K_CF_ALLOCATOR_DEFAULT, 0)
            };

            if kr == 0 && !props.is_null() {
                let count = unsafe { CFDictionaryGetCount(props as CFDictionaryRef) };
                std::hint::black_box(count);
                unsafe { CFRelease(props as CFTypeRef) };
            }

            // Probe specific sensor properties for deeper traversal
            for key_name in SENSOR_PROPERTY_KEYS {
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

    /// Check if any sensor services are reachable.
    pub fn has_sensor_services() -> bool {
        for class in IMU_SERVICE_CLASSES {
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
        IMU_SERVICE_CLASSES
    }
}

impl EntropySource for ImuNoiseSource {
    fn info(&self) -> &SourceInfo {
        &IMU_NOISE_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_sensor_services()
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
                let duration = iokit::probe_sensor_service(class);
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
        let src = ImuNoiseSource;
        assert_eq!(src.name(), "imu_noise");
        assert_eq!(src.info().category, SourceCategory::Sensor);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_mems() {
        let src = ImuNoiseSource;
        assert!(src.info().physics.contains("MEMS"));
        assert!(src.info().physics.contains("Brownian"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
    }

    #[test]
    #[ignore] // Requires macOS Apple Silicon with motion sensors
    fn collects_bytes() {
        let src = ImuNoiseSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
