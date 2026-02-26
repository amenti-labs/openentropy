//! All entropy source implementations, organized by category.
//!
//! ## Source Categories
//!
//! - **Timing**: Clock jitter, DRAM row buffer, page fault timing
//! - **Scheduling**: Sleep jitter, thread lifecycle, timer coalescing
//! - **System**: Kernel counters, process state, IORegistry
//! - **Network**: DNS timing, TCP handshake, WiFi RSSI
//! - **IO**: Disk latency, NVMe sensors, fsync journaling
//! - **Sensor**: Camera noise, audio ADC noise, Bluetooth RSSI
//! - **Microarch**: Branch prediction, TLB shootdown, AMX timing
//! - **IPC**: Mach ports, pipes, kqueue, keychain
//! - **Thermal**: PLL clock domain crossings (audio, display, PCIe)
//! - **GPU**: Metal divergence, IOSurface crossing, NL inference
//! - **Signal**: Compression timing, hash timing, Spotlight

pub mod helpers;

pub mod gpu;
pub mod io;
pub mod ipc;
pub mod microarch;
pub mod network;
pub mod scheduling;
pub mod sensor;
pub mod signal;
pub mod system;
pub mod thermal;
pub mod timing;

use crate::source::EntropySource;

/// All entropy source constructors, composed from category modules.
pub fn all_sources() -> Vec<Box<dyn EntropySource>> {
    let mut v = Vec::with_capacity(64);
    v.extend(timing::sources());
    v.extend(scheduling::sources());
    v.extend(system::sources());
    v.extend(network::sources());
    v.extend(io::sources());
    v.extend(sensor::sources());
    v.extend(microarch::sources());
    v.extend(ipc::sources());
    v.extend(thermal::sources());
    v.extend(gpu::sources());
    v.extend(signal::sources());
    v
}
