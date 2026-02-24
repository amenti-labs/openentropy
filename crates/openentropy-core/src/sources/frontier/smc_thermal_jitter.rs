//! SMC (System Management Controller) thermal sensor timing entropy.
//!
//! The SMC is an independent co-processor on Apple Silicon that manages thermal
//! sensors, fan control, battery management, and power state. Every key read
//! crosses the CPU → Mach IPC → SMC co-processor boundary. Round-trip latency
//! for the CPU die temperature sensor (TCXC) varies with SMC workload, thermal
//! sensor polling cadence, and co-processor power state.
//!
//! ## Physics
//!
//! The SMC has its own processor, RAM, and clock domain. When we request a
//! thermal sensor reading:
//!
//! 1. Mach IPC carries the request from userspace to the kernel `AppleSMC` driver
//! 2. The driver schedules the read on the SMC bus (a synchronous serial bus)
//! 3. The SMC co-processor reads the sensor, performs ADC conversion, and responds
//! 4. The result travels back through kernel → Mach IPC → userspace
//!
//! Latency varies because the SMC continuously polls ~50 sensors at different
//! rates. Our request arrives at a random phase relative to the SMC's internal
//! polling cycle, introducing timing jitter of ~20% CV. On TCXC (CPU die temp),
//! measurements show CV > 20% and range spanning 2083–10625 hardware ticks.
//!
//! Unlike CPU-internal sources, SMC timing captures the thermal and power
//! management state of the entire SoC package — a physical noise source that
//! has no equivalent on non-Apple platforms.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;
#[cfg(target_os = "macos")]
use crate::sources::helpers::mach_time;

static SMC_THERMAL_JITTER_INFO: SourceInfo = SourceInfo {
    name: "smc_thermal_jitter",
    description: "SMC co-processor thermal sensor IPC round-trip timing",
    physics: "Times IOKit SMC key reads (TCXC = CPU die temp sensor) that cross the \
              CPU \u{2192} kernel \u{2192} SMC co-processor hardware boundary. The SMC has its own \
              clock domain and continuously polls ~50 sensors on an internal schedule. \
              Our read arrives at a random phase relative to that schedule, creating \
              timing jitter tied to the SMC\u{2019}s internal state, current thermal sensor \
              values, and co-processor power state. Empirically: CV >20%, range 2x-5x \
              around mean, no equivalent on non-Apple platforms.",
    category: SourceCategory::Thermal,
    platform: Platform::MacOS,
    requirements: &[Requirement::IOKit],
    entropy_rate_estimate: 800.0,
    composite: false,
};

/// Entropy source that harvests timing jitter from SMC co-processor reads.
pub struct SMCThermalJitterSource;

/// IOKit SMC key-read implementation (macOS only).
#[cfg(target_os = "macos")]
mod smc {
    use std::ffi::c_void;

    pub type IOReturn = i32;
    pub type MachPort = u32;

    // IOKit framework symbols.
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        pub fn IOServiceGetMatchingService(
            main_port: MachPort,
            matching: *const c_void,
        ) -> u32;
        pub fn IOServiceMatching(name: *const i8) -> *mut c_void;
        pub fn IOServiceOpen(
            service: u32,
            owning_task: u32,
            kind: u32,
            connect: *mut u32,
        ) -> IOReturn;
        pub fn IOConnectCallStructMethod(
            connection: u32,
            selector: u32,
            input: *const c_void,
            input_size: usize,
            output: *mut c_void,
            output_size: *mut usize,
        ) -> IOReturn;
        pub fn IOServiceClose(connect: u32) -> IOReturn;
        pub fn IOObjectRelease(obj: u32) -> IOReturn;
    }

    // mach_task_self() returns the task port, needed for IOServiceOpen.
    #[link(name = "c")]
    unsafe extern "C" {
        pub fn mach_task_self() -> u32;
    }

    // kIOMainPortDefault = 0 on macOS 12+.
    pub const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;

    /// SMC kernel index for struct method call.
    pub const KERNEL_INDEX_SMC: u32 = 2;
    /// SMC command: read bytes.
    pub const SMC_CMD_READ_BYTES: u8 = 5;

    /// Packed 4-byte SMC key encoding.
    #[inline]
    pub fn encode_key(k: &[u8; 4]) -> u32 {
        ((k[0] as u32) << 24) | ((k[1] as u32) << 16) | ((k[2] as u32) << 8) | (k[3] as u32)
    }

    /// Minimal SMC parameter struct (must match kernel ABI).
    #[repr(C)]
    pub struct SMCParamStruct {
        pub key: u32,
        pub vers: [u8; 6],
        pub p_limit_data: [u8; 12],
        pub key_info: [u8; 9],
        pub result: u8,
        pub status: u8,
        pub data8: u8,
        pub data32: u32,
        pub bytes: [u8; 32],
    }

    impl SMCParamStruct {
        pub fn new_read(key: u32) -> Self {
            let mut s = Self {
                key,
                vers: [0; 6],
                p_limit_data: [0; 12],
                key_info: [0; 9],
                result: 0,
                status: 0,
                data8: SMC_CMD_READ_BYTES,
                data32: 0,
                bytes: [0; 32],
            };
            // Explicitly set the command byte.
            s.data8 = SMC_CMD_READ_BYTES;
            s
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::smc::*;
    use super::*;

    /// Open a connection to the AppleSMC IOKit service.
    ///
    /// Returns the io_connect_t on success.
    fn open_smc() -> Option<u32> {
        // SAFETY: IOServiceMatching takes a static C string and returns a CF dict
        // that IOServiceGetMatchingService consumes.
        let service_name = b"AppleSMC\0";
        let svc = unsafe {
            IOServiceGetMatchingService(
                K_IO_MAIN_PORT_DEFAULT,
                IOServiceMatching(service_name.as_ptr() as *const i8),
            )
        };
        if svc == 0 {
            return None;
        }

        let mut conn: u32 = 0;
        // SAFETY: svc is a valid service object; mach_task_self() is always valid.
        let kr = unsafe { IOServiceOpen(svc, mach_task_self(), 0, &mut conn) };
        unsafe { IOObjectRelease(svc) };

        if kr == 0 { Some(conn) } else { None }
    }

    impl EntropySource for SMCThermalJitterSource {
        fn info(&self) -> &SourceInfo {
            &SMC_THERMAL_JITTER_INFO
        }

        fn is_available(&self) -> bool {
            // Try opening and immediately closing the SMC service.
            if let Some(conn) = open_smc() {
                unsafe { IOServiceClose(conn) };
                true
            } else {
                false
            }
        }

        fn collect(&self, n_samples: usize) -> Vec<u8> {
            let conn = match open_smc() {
                Some(c) => c,
                None => return Vec::new(),
            };

            // TCXC = CPU die composite temperature sensor.
            // It has the highest timing variance among SMC keys because
            // the die temperature ADC requires more conversion cycles.
            let tcxc_key = encode_key(b"TCXC");

            // Cycle through multiple keys to diversify SMC bus states.
            // Each key hits a different sensor on the SMC's internal bus,
            // producing timing variance from different ADC conversion times.
            let keys = [
                encode_key(b"TCXC"), // CPU die composite
                encode_key(b"TC0E"), // CPU E-core cluster 0
                encode_key(b"TC0F"), // CPU P-core cluster 0
                encode_key(b"PSTR"), // Power state register (fast read, low latency outlier)
            ];
            let _ = tcxc_key;

            // 8× oversampling: each timing value contributes <1 bit of entropy.
            let raw_count = n_samples * 8 + 64;
            let mut timings = Vec::with_capacity(raw_count);

            // Warm up: let SMC settle into normal polling rhythm.
            for i in 0..16 {
                let mut inp = SMCParamStruct::new_read(keys[i % keys.len()]);
                let mut out = SMCParamStruct::new_read(0);
                let mut out_size = std::mem::size_of::<SMCParamStruct>();
                unsafe {
                    IOConnectCallStructMethod(
                        conn,
                        KERNEL_INDEX_SMC,
                        &inp as *const SMCParamStruct as *const _,
                        std::mem::size_of::<SMCParamStruct>(),
                        &mut out as *mut SMCParamStruct as *mut _,
                        &mut out_size,
                    )
                };
                // Use result to prevent dead-code elimination.
                let _ = out.bytes[0];
                inp.data8 = SMC_CMD_READ_BYTES;
            }

            for i in 0..raw_count {
                let key = keys[i % keys.len()];
                let inp = SMCParamStruct::new_read(key);
                let mut out = SMCParamStruct::new_read(0);
                let mut out_size = std::mem::size_of::<SMCParamStruct>();

                let t0 = mach_time();
                let kr = unsafe {
                    IOConnectCallStructMethod(
                        conn,
                        KERNEL_INDEX_SMC,
                        &inp as *const SMCParamStruct as *const _,
                        std::mem::size_of::<SMCParamStruct>(),
                        &mut out as *mut SMCParamStruct as *mut _,
                        &mut out_size,
                    )
                };
                let t1 = mach_time();

                if kr == 0 {
                    // Mix in the sensor value itself: the ADC reading is a real
                    // physical measurement (temperature in °C × 64, FP16 format).
                    let sensor_val = u16::from_be_bytes([out.bytes[0], out.bytes[1]]) as u64;
                    let delta = t1.wrapping_sub(t0) ^ (sensor_val << 3);
                    timings.push(delta);
                }
            }

            unsafe { IOServiceClose(conn) };

            extract_timing_entropy(&timings, n_samples)
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl EntropySource for SMCThermalJitterSource {
    fn info(&self) -> &SourceInfo {
        &SMC_THERMAL_JITTER_INFO
    }

    fn is_available(&self) -> bool {
        false
    }

    fn collect(&self, _n_samples: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = SMCThermalJitterSource;
        assert_eq!(src.info().name, "smc_thermal_jitter");
        assert!(matches!(src.info().category, SourceCategory::Thermal));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available_reflects_hardware() {
        // Just verify it returns without panic — true if SMC is accessible.
        let _ = SMCThermalJitterSource.is_available();
    }

    #[test]
    #[ignore] // Requires AppleSMC IOKit service
    fn collects_bytes() {
        let src = SMCThermalJitterSource;
        if !src.is_available() {
            return;
        }
        let data = src.collect(32);
        assert!(!data.is_empty());
        let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(unique.len() > 1, "expected byte variation from SMC timing");
    }
}
