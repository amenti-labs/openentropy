//! SMC high-variance key timing entropy — thermistor ADC and fuel gauge I2C bus.
//!
//! The Apple System Management Controller (SMC) manages hundreds of hardware
//! keys representing sensor readings, power states, and system configuration.
//! Empirical measurement of 10 SMC keys reveals a bimodal CV distribution:
//!
//! - **Standard keys (8/10)**: CV=6–14% — cached or polled-register reads
//! - **Two outliers**: TC0P=63.9%, B0RM=66.0% — **8× higher variance**
//!
//! ## Physics of the Two Outlier Keys
//!
//! **TC0P — CPU Proximity NTC Thermistor:**
//! Unlike TCXC (die-embedded digital sensor), TC0P reads a discrete NTC
//! (Negative Temperature Coefficient) thermistor mounted on the PCB near the
//! CPU package. The SMC reads it via its onboard 12-bit ADC:
//!
//! 1. ADC must start a conversion, apply the RC settling time, then sample
//! 2. Conversion time varies with thermistor resistance (a function of die temp)
//! 3. Our SMC IPC request arrives at a random phase of the ADC cycle
//! 4. Phase misalignment adds 0 to ~30,000 ticks depending on where we land
//!
//! TC0P: mean=4723 ticks, CV=63.9%, range=[4059,33893]
//!
//! **B0RM — Battery Remaining Capacity (mAh):**
//! The battery fuel gauge IC (a separate microcontroller on the SMBus/I2C bus,
//! e.g., Texas Instruments bq28z610 or Maxim MAX17057) maintains coulomb counts,
//! cell voltage, and capacity state for Li-ion cells.
//!
//! On Mac mini (no battery): The SMC polls the I2C battery bus for a device
//! that doesn't exist. The I2C transaction times out after a stretch period:
//! - Fast path (~3200 ticks): SMC returns cached "not present" value immediately
//! - Slow path (~35000 ticks): SMC initiates I2C bus poll, waits for timeout
//!
//! On MacBook: B0RM reads live from the fuel gauge ADC — real electrochemical
//! noise from Li-ion cell voltage measurement across the coulomb counter shunt.
//!
//! B0RM: mean=3594 ticks, CV=66.0%, range=[3226,35826]
//!
//! ## Relationship to Power Side-Channel Research
//!
//! Chawla et al. (2023) demonstrated that SMC power meter keys (e.g., `PHPC`,
//! `PPBR`, `PPLN`) can be read to recover AES encryption keys via software-
//! based power analysis on M1/M2 — no physical measurement required. Their
//! attack exploits the *values* returned by SMC power keys.
//!
//! This source exploits the *timing* of SMC IPC calls to TC0P and B0RM —
//! a different and orthogonal channel. The high CV of TC0P reflects ADC phase
//! jitter (thermal noise) rather than data-dependent power consumption.
//!
//! ## Security Note: B0RM on Desktop Macs
//!
//! On Mac mini and Mac Pro (no battery), the B0RM key causes the SMC to poll
//! an I2C bus for a fuel gauge IC that is not present. This timeout-driven
//! bimodal behavior is reproducible across reboots. It is unclear why Apple's
//! firmware polls a battery bus on battery-free hardware; the most likely
//! explanation is shared firmware with MacBook. The I2C bus timeout creates a
//! measurable covert timing channel into the SMC's I2C bus state machine.
//!
//! ## References
//!
//! - Chawla et al., "Uncovering Software-Based Power Side-Channel Attacks on
//!   Apple M1/M2 Systems", arXiv:2306.16391 [cs.CR], 2023.
//!   <https://arxiv.org/abs/2306.16391>

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};

#[cfg(target_os = "macos")]
use crate::sources::helpers::mach_time;
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;

static SMC_HIGHVAR_TIMING_INFO: SourceInfo = SourceInfo {
    name: "smc_highvar_timing",
    description: "SMC thermistor ADC + fuel gauge I2C bus — CV=64–66%, 8× outliers",
    physics: "Targets two SMC keys with 8× higher CV than all others: TC0P (CPU proximity \
              NTC thermistor, analog ADC conversion phase-alignment jitter, mean=4723 ticks, \
              CV=63.9%, range=[4059,33893]) and B0RM (battery fuel gauge IC over SMBus/I2C, \
              bimodal fast-cache vs slow I2C-timeout, mean=3594, CV=66.0%, \
              range=[3226,35826]). On MacBook: B0RM reads live Li-ion electrochemical \
              coulomb-counter noise. On Mac mini: I2C bus timeout randomness from SMC \
              polling absent battery device. Both keys interleaved to mix analog ADC \
              entropy with I2C bus entropy. Standard SMC keys (TCXC, TG0P, PSTR, etc.) \
              all show CV=6\u{2013}14% — these two are structural outliers.",
    category: SourceCategory::Sensor,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2.5,
    composite: false,
    is_fast: false,
};

/// Entropy source from high-variance SMC key timing (TC0P + B0RM).
pub struct SMCHighVarTimingSource;

// Inline IOKit declarations mirrored from smc_thermal_jitter.rs
mod smc_hv {
    use std::ffi::c_void;

    pub type IOReturn = i32;
    pub type MachPort = u32;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        pub fn IOServiceGetMatchingService(main_port: MachPort, matching: *const c_void) -> u32;
        pub fn IOServiceMatching(name: *const i8) -> *mut c_void;
        pub fn IOServiceOpen(service: u32, task: u32, kind: u32, connect: *mut u32) -> IOReturn;
        pub fn IOConnectCallStructMethod(
            connection: u32, selector: u32,
            input: *const c_void, input_size: usize,
            output: *mut c_void, output_size: *mut usize,
        ) -> IOReturn;
        pub fn IOServiceClose(connect: u32) -> IOReturn;
        pub fn IOObjectRelease(obj: u32) -> IOReturn;
    }

    #[link(name = "c")]
    unsafe extern "C" {
        pub fn mach_task_self() -> u32;
    }

    pub const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;
    pub const SMC_CMD_READ_BYTES: u8 = 5;

    pub fn encode_key(k: &[u8; 4]) -> u32 {
        ((k[0] as u32) << 24) | ((k[1] as u32) << 16) | ((k[2] as u32) << 8) | (k[3] as u32)
    }

    #[repr(C)]
    pub struct SMCParam {
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

    impl SMCParam {
        pub fn new(key: u32) -> Self {
            Self {
                key,
                vers: [0; 6],
                p_limit_data: [0; 12],
                key_info: [0; 9],
                result: 0,
                status: 0,
                data8: SMC_CMD_READ_BYTES,
                data32: 0,
                bytes: [0; 32],
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl EntropySource for SMCHighVarTimingSource {
    fn info(&self) -> &SourceInfo {
        &SMC_HIGHVAR_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        use smc_hv::*;
        let svc = unsafe {
            IOServiceGetMatchingService(
                K_IO_MAIN_PORT_DEFAULT,
                IOServiceMatching(c"AppleSMC".as_ptr()),
            )
        };
        if svc != 0 { unsafe { IOObjectRelease(svc) }; true } else { false }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        use smc_hv::*;
        use std::ffi::c_void;

        let svc = unsafe {
            IOServiceGetMatchingService(
                K_IO_MAIN_PORT_DEFAULT,
                IOServiceMatching(c"AppleSMC".as_ptr()),
            )
        };
        if svc == 0 { return Vec::new(); }

        let mut conn: u32 = 0;
        let kr = unsafe { IOServiceOpen(svc, mach_task_self(), 0, &mut conn) };
        unsafe { IOObjectRelease(svc) };
        if kr != 0 { return Vec::new(); }

        let tc0p = encode_key(b"TC0P");
        let b0rm = encode_key(b"B0RM");
        let raw = n_samples * 3 + 32;
        let mut timings = Vec::with_capacity(raw * 2);

        // Warm up — SMC IPC takes time to settle on first call
        for _ in 0..4 {
            let inp = SMCParam::new(tc0p);
            let mut out = SMCParam::new(tc0p);
            let mut out_sz = std::mem::size_of::<SMCParam>();
            unsafe {
                IOConnectCallStructMethod(conn, 5,
                    &inp as *const _ as *const c_void, std::mem::size_of::<SMCParam>(),
                    &mut out as *mut _ as *mut c_void, &mut out_sz);
            }
            let _ = inp.result;
        }

        for _ in 0..raw {
            // TC0P — thermistor ADC phase jitter
            let mut inp = SMCParam::new(tc0p);
            let mut out_tc = SMCParam::new(tc0p);
            let mut out_sz = std::mem::size_of::<SMCParam>();
            let t0 = mach_time();
            unsafe {
                IOConnectCallStructMethod(conn, 5,
                    &inp as *const _ as *const c_void, std::mem::size_of::<SMCParam>(),
                    &mut out_tc as *mut _ as *mut c_void, &mut out_sz);
            }
            let t_tc = mach_time().wrapping_sub(t0);

            // B0RM — fuel gauge I2C bus timing
            inp = SMCParam::new(b0rm);
            let mut out_b0 = SMCParam::new(b0rm);
            out_sz = std::mem::size_of::<SMCParam>();
            let t1 = mach_time();
            unsafe {
                IOConnectCallStructMethod(conn, 5,
                    &inp as *const _ as *const c_void, std::mem::size_of::<SMCParam>(),
                    &mut out_b0 as *mut _ as *mut c_void, &mut out_sz);
            }
            let t_b0 = mach_time().wrapping_sub(t1);

            // Reject extreme outliers (>50ms = suspend/resume)
            if t_tc < 1_200_000 { timings.push(t_tc); }
            if t_b0 < 1_200_000 { timings.push(t_b0); }
        }

        unsafe { IOServiceClose(conn) };
        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(not(target_os = "macos"))]
impl EntropySource for SMCHighVarTimingSource {
    fn info(&self) -> &SourceInfo { &SMC_HIGHVAR_TIMING_INFO }
    fn is_available(&self) -> bool { false }
    fn collect(&self, _: usize) -> Vec<u8> { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = SMCHighVarTimingSource;
        assert_eq!(src.info().name, "smc_highvar_timing");
        assert!(matches!(src.info().category, SourceCategory::Sensor));
        assert_eq!(src.info().platform, Platform::MacOS);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available_on_macos_with_smc() {
        // Most Macs have an SMC
        let _ = SMCHighVarTimingSource.is_available(); // just don't panic
    }

    #[test]
    #[ignore]
    fn collects_bimodal_smc_outliers() {
        let data = SMCHighVarTimingSource.collect(32);
        assert!(!data.is_empty());
    }
}
