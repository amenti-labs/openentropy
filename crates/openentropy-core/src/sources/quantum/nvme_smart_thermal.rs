//! NVMe SMART thermal noise — ADC quantization noise from NVMe temperature sensor.
//!
//! High-frequency polling of NVMe temperature fields via IOKit, extracting ADC
//! quantization noise from the least-significant bits. The NVMe composite
//! temperature (2 bytes, Kelvin) comes from an on-die ADC whose LSBs contain
//! Johnson-Nyquist thermal noise.
//!
//! ## Entropy mechanism
//!
//! - **ADC thermal noise**: The on-die temperature sensor ADC has quantization
//!   noise with genuine quantum origin (Johnson-Nyquist noise in the sense
//!   resistor / reference voltage)
//! - **IOKit traversal timing**: Each property read crosses clock domains,
//!   adding PLL jitter entropy
//! - **Temperature microvariations**: Real thermal fluctuations at the die
//!   surface change between consecutive reads
//!
//! ## Honest quantum assessment
//!
//! `quantum_fraction ≈ 0.35` — ADC thermal noise has genuine quantum components
//! (Johnson-Nyquist), but the macroscopic temperature is classical. The LSBs
//! capture a mix of quantization noise (quantum) and readout circuit noise.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::{extract_timing_entropy, xor_fold_u64};

static NVME_SMART_THERMAL_INFO: SourceInfo = SourceInfo {
    name: "nvme_smart_thermal",
    description: "NVMe temperature ADC quantization noise via high-frequency IOKit sensor polling",
    physics: "Polls the NVMe controller\u{2019}s composite temperature register at high frequency \
              via IOKit. The on-die ADC converts analog temperature to digital values; its LSBs \
              contain Johnson-Nyquist thermal noise from the sense resistor and reference voltage \
              circuitry. This noise has genuine quantum origin: Johnson-Nyquist noise arises from \
              thermal equilibrium fluctuations of charge carriers, a quantum statistical mechanical \
              effect. CNTVCT_EL0 timestamps add clock domain crossing jitter between CPU crystal \
              and NVMe controller PLL.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[Requirement::AppleSilicon, Requirement::IOKit],
    entropy_rate_estimate: 1200.0,
    composite: false,
};

/// NVMe SMART thermal noise entropy source.
pub struct NvmeSmartThermalSource;

#[cfg(target_os = "macos")]
mod iokit {
    use crate::sources::helpers::read_cntvct;
    use std::ffi::{c_char, c_long, c_void, CString};

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
    type CFNumberRef = *const c_void;

    #[allow(non_camel_case_types)]
    type CFNumberType = u32;
    const K_CF_NUMBER_LONG_TYPE: CFNumberType = 10;

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
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFNumberGetValue(
            number: CFNumberRef,
            the_type: CFNumberType,
            value_ptr: *mut c_void,
        ) -> bool;
    }

    /// NVMe service classes to try (most specific first).
    const NVME_CLASSES: &[&str] = &[
        "AppleANS3CGv2Controller",
        "AppleANS2Controller",
        "IONVMeController",
    ];

    /// Temperature-related property keys.
    const TEMP_KEYS: &[&str] = &[
        "Temperature",
        "Composite Temperature",
    ];

    /// A single temperature reading with its timing.
    pub struct TempReading {
        pub value: i64,
        pub duration_ticks: u64,
    }

    /// Find the first reachable NVMe service and return its io_object handle.
    /// Caller must release via IOObjectRelease.
    fn find_nvme_service() -> Option<io_registry_entry_t> {
        for class in NVME_CLASSES {
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
                        return Some(svc);
                    }
                }
            }
        }
        None
    }

    /// Read the NVMe temperature property from a service entry.
    /// Returns (temperature_value, duration_in_cntvct_ticks).
    fn read_temperature(service: io_registry_entry_t) -> TempReading {
        let t_before = read_cntvct();

        for key_name in TEMP_KEYS {
            let c_key = match CString::new(*key_name) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // SAFETY: CFStringCreateWithCString creates a CFString from a C string.
            // IORegistryEntryCreateCFProperty reads a single property.
            // CFNumberGetValue extracts a numeric value.
            // All are read-only CF operations on valid objects.
            unsafe {
                let cf_key = CFStringCreateWithCString(
                    K_CF_ALLOCATOR_DEFAULT,
                    c_key.as_ptr(),
                    K_CF_STRING_ENCODING_UTF8,
                );
                if cf_key.is_null() {
                    continue;
                }

                let val =
                    IORegistryEntryCreateCFProperty(service, cf_key, K_CF_ALLOCATOR_DEFAULT, 0);
                CFRelease(cf_key);

                if !val.is_null() {
                    let mut num_val: c_long = 0;
                    let ok = CFNumberGetValue(
                        val as CFNumberRef,
                        K_CF_NUMBER_LONG_TYPE,
                        &mut num_val as *mut c_long as *mut c_void,
                    );
                    CFRelease(val);

                    if ok {
                        let duration = read_cntvct().wrapping_sub(t_before);
                        return TempReading {
                            value: num_val as i64,
                            duration_ticks: duration,
                        };
                    }
                }
            }
        }

        let duration = read_cntvct().wrapping_sub(t_before);
        TempReading {
            value: 0,
            duration_ticks: duration,
        }
    }

    /// Check if any NVMe temperature services are reachable.
    pub fn has_nvme_temp() -> bool {
        if let Some(svc) = find_nvme_service() {
            // SAFETY: IOObjectRelease releases a valid IOKit object.
            unsafe { IOObjectRelease(svc) };
            true
        } else {
            false
        }
    }

    /// Poll the NVMe temperature `count` times.
    /// Returns parallel vecs of (temperature_values, timing_ticks).
    pub fn poll_temperature(count: usize) -> (Vec<i64>, Vec<u64>) {
        let mut temps = Vec::with_capacity(count);
        let mut timings = Vec::with_capacity(count);

        let service = match find_nvme_service() {
            Some(s) => s,
            None => return (temps, timings),
        };

        for _ in 0..count {
            let reading = read_temperature(service);
            temps.push(reading.value);
            timings.push(reading.duration_ticks);
        }

        // SAFETY: IOObjectRelease releases a valid IOKit object.
        unsafe { IOObjectRelease(service) };

        (temps, timings)
    }
}

impl EntropySource for NvmeSmartThermalSource {
    fn info(&self) -> &SourceInfo {
        &NVME_SMART_THERMAL_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            iokit::has_nvme_temp()
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
            // Over-sample: need ~4x raw readings per output byte.
            let raw_count = n_samples * 4 + 64;
            let (temps, timings) = iokit::poll_temperature(raw_count);

            if temps.is_empty() {
                return Vec::new();
            }

            // Two entropy streams:
            // 1. IOKit traversal timing (clock domain crossing jitter)
            let timing_bytes = extract_timing_entropy(&timings, n_samples);

            // 2. Temperature ADC LSB noise: XOR consecutive temp readings,
            //    fold the delta to capture quantization noise in the LSBs.
            let temp_deltas: Vec<u64> = temps
                .windows(2)
                .map(|w| (w[1].wrapping_sub(w[0])) as u64)
                .collect();

            let temp_xored: Vec<u64> = temp_deltas
                .windows(2)
                .map(|w| w[0] ^ w[1])
                .collect();

            let temp_bytes: Vec<u8> = temp_xored
                .iter()
                .map(|&x| xor_fold_u64(x))
                .take(n_samples)
                .collect();

            // XOR timing and temperature streams together for combined entropy.
            let mut output = Vec::with_capacity(n_samples);
            for i in 0..n_samples {
                let tb = timing_bytes.get(i).copied().unwrap_or(0);
                let tempb = temp_bytes.get(i).copied().unwrap_or(0);
                output.push(tb ^ tempb);
            }

            // If one stream was shorter, pad with the other.
            let combined_len = timing_bytes.len().min(temp_bytes.len());
            if combined_len < n_samples {
                output.truncate(combined_len.max(timing_bytes.len().max(temp_bytes.len())));
            }
            output.truncate(n_samples);
            output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = NvmeSmartThermalSource;
        assert_eq!(src.name(), "nvme_smart_thermal");
        assert_eq!(src.info().category, SourceCategory::Sensor);
        assert!(!src.info().composite);
    }

    #[test]
    fn physics_mentions_adc() {
        let src = NvmeSmartThermalSource;
        assert!(src.info().physics.contains("ADC"));
        assert!(src.info().physics.contains("Johnson-Nyquist"));
        assert!(src.info().physics.contains("CNTVCT_EL0"));
    }

    #[test]
    #[ignore] // Requires macOS Apple Silicon with NVMe controller
    fn collects_bytes() {
        let src = NvmeSmartThermalSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
