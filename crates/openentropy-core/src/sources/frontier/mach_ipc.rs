//! Mach IPC timing — entropy from complex Mach messages with OOL descriptors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Configuration for Mach IPC entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::MachIPCConfig;
/// let config = MachIPCConfig {
///     num_ports: 16,               // more ports = more contention
///     ool_size: 8192,              // larger OOL = more VM work
///     use_complex_messages: true,  // OOL messages (recommended)
/// };
/// ```
#[derive(Debug, Clone)]
pub struct MachIPCConfig {
    /// Number of Mach ports to round-robin across.
    ///
    /// More ports create more namespace contention (splay tree operations)
    /// and varied queue depths. Each port is allocated with both receive
    /// and send rights.
    ///
    /// **Range:** 1-64 (clamped to ≥1). **Default:** `8`
    pub num_ports: usize,

    /// Size of out-of-line (OOL) memory descriptors in bytes.
    ///
    /// OOL descriptors force the kernel to perform `vm_map_copyin`/`copyout`
    /// operations, exercising page table updates and physical page allocation.
    /// Larger sizes mean more VM work per message.
    ///
    /// **Range:** 1-65536. **Default:** `4096` (one page)
    pub ool_size: usize,

    /// Use complex messages with OOL descriptors (`true`) or simple port
    /// allocate/deallocate (`false`, legacy behavior).
    ///
    /// Complex messages traverse deeper kernel paths and produce significantly
    /// higher timing variance than simple port operations.
    ///
    /// **Default:** `true`
    pub use_complex_messages: bool,
}

impl Default for MachIPCConfig {
    fn default() -> Self {
        Self {
            num_ports: 8,
            ool_size: 4096,
            use_complex_messages: true,
        }
    }
}

/// Harvests timing jitter from Mach IPC using complex OOL messages.
///
/// # What it measures
/// Nanosecond timing of `mach_msg()` sends with out-of-line memory descriptors,
/// round-robined across a pool of Mach ports.
///
/// # Why it's entropic
/// Complex Mach messages with OOL descriptors traverse deep kernel paths:
/// - **OOL VM remapping** — `vm_map_copyin`/`vm_map_copyout` exercises page
///   tables, physical page allocation, and TLB updates
/// - **Port namespace contention** — round-robin across ports exercises the
///   splay tree with timing dependent on tree depth and rebalancing
/// - **Per-port lock contention** — `ipc_mqueue_send` acquires per-port locks
/// - **Receiver thread wakeup** — cross-thread scheduling decisions affected
///   by ALL runnable threads
///
/// # What makes it unique
/// Mach IPC is unique to XNU/macOS. Unlike higher-level IPC (pipes, sockets),
/// Mach messages go through XNU's `ipc_mqueue` subsystem with entirely different
/// locking and scheduling paths. OOL descriptors add VM operations that no
/// other entropy source exercises.
///
/// # Configuration
/// See [`MachIPCConfig`] for tunable parameters. Key options:
/// - `use_complex_messages`: OOL messages vs simple port ops (recommended: `true`)
/// - `num_ports`: controls namespace contention level
/// - `ool_size`: controls VM remapping workload per message
#[derive(Default)]
pub struct MachIPCSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
    pub config: MachIPCConfig,
}

static MACH_IPC_INFO: SourceInfo = SourceInfo {
    name: "mach_ipc",
    description: "Mach port complex OOL message and VM remapping timing jitter",
    physics: "Sends complex Mach messages with out-of-line (OOL) memory descriptors via \
              mach_msg(), round-robining across multiple ports. OOL descriptors force kernel \
              VM remapping (vm_map_copyin/copyout) which exercises page table operations. \
              Round-robin across ports with varied queue depths creates namespace contention. \
              Timing captures: OOL VM remap latency, port namespace splay tree operations, \
              per-port lock contention, and cross-core scheduling nondeterminism.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
    composite: false,
};

impl EntropySource for MachIPCSource {
    fn info(&self) -> &SourceInfo {
        &MACH_IPC_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // SAFETY: mach_task_self() returns the current task port (always valid).
        let task = unsafe { mach_task_self() };
        let num_ports = self.config.num_ports.max(1);

        let mut ports: Vec<u32> = Vec::with_capacity(num_ports);
        for _ in 0..num_ports {
            let mut port: u32 = 0;
            // SAFETY: mach_port_allocate allocates a receive right.
            let kr = unsafe {
                mach_port_allocate(task, 1 /* MACH_PORT_RIGHT_RECEIVE */, &mut port)
            };
            if kr == 0 {
                // SAFETY: port is a valid receive right we just allocated.
                let kr2 = unsafe {
                    mach_port_insert_right(task, port, port, 20 /* MACH_MSG_TYPE_MAKE_SEND */)
                };
                if kr2 == 0 {
                    ports.push(port);
                } else {
                    unsafe { mach_port_mod_refs(task, port, 1, -1); }
                }
            }
        }

        if ports.is_empty() {
            return self.collect_simple(n_samples);
        }

        if self.config.use_complex_messages {
            let ool_size = self.config.ool_size.max(1);
            let ool_buf = vec![0xBEu8; ool_size];

            let stop = Arc::new(AtomicBool::new(false));
            let stop2 = stop.clone();
            let recv_ports = ports.clone();
            let receiver = thread::spawn(move || {
                let mut recv_buf = vec![0u8; 1024 + ool_size * 2];
                while !stop2.load(Ordering::Relaxed) {
                    for &port in &recv_ports {
                        // SAFETY: recv_buf is large enough. Non-blocking receive.
                        unsafe {
                            let hdr = recv_buf.as_mut_ptr() as *mut MachMsgHeader;
                            (*hdr).msgh_local_port = port;
                            (*hdr).msgh_size = recv_buf.len() as u32;
                            mach_msg(hdr, 2 | 0x100, 0, recv_buf.len() as u32, port, 0, 0);
                        }
                    }
                    std::thread::yield_now();
                }
            });

            for i in 0..raw_count {
                let port = ports[i % ports.len()];

                let mut msg = MachMsgOOL::zeroed();
                msg.header.msgh_bits = 0x80000000 | 17; // COMPLEX | COPY_SEND
                msg.header.msgh_size = std::mem::size_of::<MachMsgOOL>() as u32;
                msg.header.msgh_remote_port = port;
                msg.header.msgh_local_port = 0;
                msg.header.msgh_id = i as i32;
                msg.body.msgh_descriptor_count = 1;
                msg.ool.address = ool_buf.as_ptr() as *mut _;
                msg.ool.size = ool_size as u32;
                msg.ool.deallocate = 0;
                msg.ool.copy = 1; // MACH_MSG_VIRTUAL_COPY
                msg.ool.ool_type = 1; // MACH_MSG_OOL_DESCRIPTOR

                let t0 = mach_time();
                // SAFETY: msg is properly initialized. MACH_SEND_TIMEOUT prevents blocking.
                unsafe {
                    mach_msg(&mut msg.header, 1 | 0x80, msg.header.msgh_size, 0, 0, 10, 0);
                }
                let t1 = mach_time();
                timings.push(t1.wrapping_sub(t0));
            }

            stop.store(true, Ordering::Relaxed);
            let _ = receiver.join();
        } else {
            for i in 0..raw_count {
                let t0 = mach_time();
                let base_port = ports[i % ports.len()];

                let mut new_port: u32 = 0;
                // SAFETY: standard Mach port operations.
                let kr = unsafe { mach_port_allocate(task, 1, &mut new_port) };
                if kr == 0 {
                    unsafe {
                        mach_port_deallocate(task, new_port);
                        mach_port_mod_refs(task, new_port, 1, -1);
                    }
                }
                unsafe {
                    let mut ptype: u32 = 0;
                    mach_port_type(task, base_port, &mut ptype);
                }
                let t1 = mach_time();
                timings.push(t1.wrapping_sub(t0));
            }
        }

        for &port in &ports {
            unsafe { mach_port_mod_refs(task, port, 1, -1); }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

impl MachIPCSource {
    fn collect_simple(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let task = unsafe { mach_task_self() };

        for _ in 0..raw_count {
            let t0 = mach_time();
            let mut port: u32 = 0;
            let kr = unsafe { mach_port_allocate(task, 1, &mut port) };
            if kr == 0 {
                unsafe {
                    mach_port_deallocate(task, port);
                    mach_port_mod_refs(task, port, 1, -1);
                }
            }
            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }
        extract_timing_entropy(&timings, n_samples)
    }
}

// Mach message structures for complex OOL messages.
#[repr(C)]
struct MachMsgHeader {
    msgh_bits: u32,
    msgh_size: u32,
    msgh_remote_port: u32,
    msgh_local_port: u32,
    msgh_voucher_port: u32,
    msgh_id: i32,
}

#[repr(C)]
struct MachMsgBody {
    msgh_descriptor_count: u32,
}

#[repr(C)]
struct MachMsgOOLDescriptor {
    address: *mut u8,
    deallocate: u8,
    copy: u8,
    ool_type: u8,
    _pad: u8,
    size: u32,
}

#[repr(C)]
struct MachMsgOOL {
    header: MachMsgHeader,
    body: MachMsgBody,
    ool: MachMsgOOLDescriptor,
}

// SAFETY: MachMsgOOL contains a raw pointer (ool.address), but we only use it
// within a single thread's send operation where the pointed-to buffer is alive.
unsafe impl Send for MachMsgOOL {}

impl MachMsgOOL {
    fn zeroed() -> Self {
        // SAFETY: All-zeros is valid for this repr(C) struct.
        unsafe { std::mem::zeroed() }
    }
}

unsafe extern "C" {
    fn mach_task_self() -> u32;
    fn mach_port_allocate(task: u32, right: i32, name: *mut u32) -> i32;
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
    fn mach_port_mod_refs(task: u32, name: u32, right: i32, delta: i32) -> i32;
    fn mach_port_insert_right(task: u32, name: u32, poly: u32, poly_poly: u32) -> i32;
    fn mach_port_type(task: u32, name: u32, ptype: *mut u32) -> i32;
    fn mach_msg(
        msg: *mut MachMsgHeader, option: i32, send_size: u32,
        rcv_size: u32, rcv_name: u32, timeout: u32, notify: u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = MachIPCSource::default();
        assert_eq!(src.name(), "mach_ipc");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = MachIPCConfig::default();
        assert_eq!(config.num_ports, 8);
        assert_eq!(config.ool_size, 4096);
        assert!(config.use_complex_messages);
    }

    #[test]
    fn custom_config() {
        let src = MachIPCSource {
            config: MachIPCConfig {
                num_ports: 4,
                ool_size: 8192,
                use_complex_messages: false,
            },
        };
        assert_eq!(src.config.num_ports, 4);
        assert!(!src.config.use_complex_messages);
    }

    #[test]
    #[ignore] // Uses Mach ports
    fn collects_bytes() {
        let src = MachIPCSource::default();
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    #[ignore] // Uses Mach ports
    fn simple_mode_collects_bytes() {
        let src = MachIPCSource {
            config: MachIPCConfig {
                use_complex_messages: false,
                ..MachIPCConfig::default()
            },
        };
        assert!(!src.collect(64).is_empty());
    }
}
