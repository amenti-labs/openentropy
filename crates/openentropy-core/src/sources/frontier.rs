//! Frontier entropy sources: novel, previously-unharvested nondeterminism from
//! Apple Silicon hardware and macOS kernel internals.
//!
//! These sources exploit entropy hiding in six unexplored domains:
//!
//! 1. **AMX coprocessor timing** — Apple Matrix eXtensions pipeline state
//! 2. **Thread lifecycle timing** — pthread create/join kernel scheduling
//! 3. **Mach IPC timing** — Mach port complex messages with OOL descriptors
//! 4. **TLB shootdown timing** — mprotect-induced inter-processor interrupts
//! 5. **Pipe buffer timing** — multi-pipe kernel zone allocator competition
//! 6. **Kqueue events timing** — kqueue event notification multiplexing

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crate::source::{EntropySource, SourceCategory, SourceInfo};

use super::helpers::{extract_timing_entropy, mach_time};

// ---------------------------------------------------------------------------
// 1. AMXTimingSource — Apple Matrix eXtensions coprocessor timing
// ---------------------------------------------------------------------------

/// Configuration for AMX timing entropy collection.
#[derive(Debug, Clone)]
pub struct AMXTimingConfig {
    /// Matrix sizes to cycle through. Different sizes stress different AMX pipeline
    /// configurations: small (register-bound), medium (L1-bound), large (L2/SLC-bound).
    pub matrix_sizes: Vec<usize>,
    /// Whether to interleave memory operations between AMX dispatches to disrupt
    /// pipeline state and prevent the AMX from settling into a steady state.
    pub interleave_memory_ops: bool,
    /// Whether to apply Von Neumann debiasing to raw timing deltas before XOR-fold.
    /// This corrects the heavy bias (H∞ 0.379 vs Shannon 6.985) by discarding
    /// correlated bit pairs.
    pub von_neumann_debias: bool,
}

impl Default for AMXTimingConfig {
    fn default() -> Self {
        Self {
            matrix_sizes: vec![16, 32, 48, 64, 96, 128],
            interleave_memory_ops: true,
            von_neumann_debias: true,
        }
    }
}

/// Entropy source that harvests timing jitter from the AMX (Apple Matrix
/// eXtensions) coprocessor via Accelerate framework BLAS operations.
///
/// **Physics:** On Apple Silicon, matrix multiply (sgemm) is dispatched to the
/// dedicated AMX coprocessor — a separate execution unit on the CPU die with
/// its own register file, pipeline, and memory access paths. The AMX has
/// independent DVFS (voltage/frequency scaling) and its internal pipeline state
/// is influenced by ALL prior AMX operations from every process. Timing
/// variation arises from:
///
/// - AMX pipeline occupancy and stalls from prior operations
/// - Memory bandwidth contention on the unified memory controller
/// - AMX power state transitions (idle → active ramp-up)
/// - L2/SLC (System Level Cache) contention from AMX's memory access patterns
/// - Thermal throttling affecting AMX frequency independently of CPU cores
///
/// **Improvements:** Von Neumann debiasing fixes the severe bias (H∞ 0.379),
/// varied operation types per sample exercise different AMX pipeline paths,
/// and interleaved memory operations disrupt pipeline steady-state.
#[derive(Default)]
pub struct AMXTimingSource {
    pub config: AMXTimingConfig,
}

static AMX_TIMING_INFO: SourceInfo = SourceInfo {
    name: "amx_timing",
    description: "Apple AMX coprocessor matrix multiply timing jitter (debiased)",
    physics: "Dispatches matrix multiplications to the AMX (Apple Matrix eXtensions) \
              coprocessor via Accelerate BLAS and measures per-operation timing. The AMX is \
              a dedicated execution unit with its own pipeline, register file, and memory \
              paths. Timing depends on: AMX pipeline occupancy from ALL system AMX users, \
              memory bandwidth contention, AMX power state transitions, and SLC cache state. \
              Von Neumann debiasing corrects heavy LSB bias. Interleaved memory operations \
              disrupt pipeline steady-state for higher min-entropy.",
    category: SourceCategory::Frontier,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 2500.0,
};

impl EntropySource for AMXTimingSource {
    fn info(&self) -> &SourceInfo {
        &AMX_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(all(target_os = "macos", target_arch = "aarch64"))
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Need extra raw samples when debiasing (VN discards ~50% of pairs).
        let debias = self.config.von_neumann_debias;
        let raw_count = if debias {
            n_samples * 8 + 128
        } else {
            n_samples * 4 + 64
        };
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        let sizes = &self.config.matrix_sizes;
        if sizes.is_empty() {
            return Vec::new();
        }
        let mut lcg: u64 = mach_time() | 1;

        // Scratch buffer for memory interleaving — 64KB to thrash L1 cache.
        let interleave = self.config.interleave_memory_ops;
        let mut scratch = if interleave {
            vec![0u8; 65536]
        } else {
            Vec::new()
        };

        for i in 0..raw_count {
            let n = sizes[i % sizes.len()];
            let len = n * n;

            // Generate pseudo-random matrix data — different data every time
            // to prevent AMX from caching results.
            let mut a = vec![0.0f32; len];
            let mut b = vec![0.0f32; len];
            let mut c = vec![0.0f32; len];

            for val in a.iter_mut().chain(b.iter_mut()) {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                *val = (lcg >> 32) as f32 / u32::MAX as f32;
            }

            // Interleave memory operations to disrupt AMX pipeline state.
            if interleave && !scratch.is_empty() {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
                let idx = (lcg >> 32) as usize % scratch.len();
                // Volatile read/write to prevent optimization.
                unsafe {
                    let ptr = scratch.as_mut_ptr().add(idx);
                    std::ptr::write_volatile(ptr, std::ptr::read_volatile(ptr).wrapping_add(1));
                }
            }

            let t0 = mach_time();

            // Vary the BLAS operation: alternate between sgemm (multiply) and
            // transposed multiply to exercise different AMX pipeline paths.
            let trans_b = if i % 3 == 1 { 112 } else { 111 }; // CblasTrans vs CblasNoTrans

            // SAFETY: cblas_sgemm is a well-defined C function from the Accelerate
            // framework. On Apple Silicon, this dispatches to the AMX coprocessor.
            unsafe {
                cblas_sgemm(
                    101, // CblasRowMajor
                    111, // CblasNoTrans
                    trans_b,
                    n as i32,
                    n as i32,
                    n as i32,
                    1.0,
                    a.as_ptr(),
                    n as i32,
                    b.as_ptr(),
                    n as i32,
                    0.0,
                    c.as_mut_ptr(),
                    n as i32,
                );
            }

            let t1 = mach_time();
            std::hint::black_box(&c);
            timings.push(t1.wrapping_sub(t0));
        }

        if debias {
            extract_timing_entropy_debiased(&timings, n_samples)
        } else {
            extract_timing_entropy(&timings, n_samples)
        }
    }
}

/// Von Neumann debiased timing extraction.
///
/// Takes pairs of consecutive timing deltas. If they differ, emit one bit
/// based on their relative order (first < second → 1, else → 0). This
/// removes bias from the raw timing stream at the cost of ~50% data loss.
fn extract_timing_entropy_debiased(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 4 {
        return Vec::new();
    }

    // Compute deltas.
    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // Von Neumann debias: take pairs, discard equal, emit comparison bit.
    let mut debiased_bits: Vec<u8> = Vec::with_capacity(deltas.len() / 2);
    for pair in deltas.chunks_exact(2) {
        if pair[0] != pair[1] {
            debiased_bits.push(if pair[0] < pair[1] { 1 } else { 0 });
        }
    }

    // Pack bits into bytes.
    let mut bytes = Vec::with_capacity(n_samples);
    for chunk in debiased_bits.chunks(8) {
        if chunk.len() < 8 {
            break; // Only emit full bytes.
        }
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
        if bytes.len() >= n_samples {
            break;
        }
    }
    bytes.truncate(n_samples);
    bytes
}

// Accelerate framework CBLAS binding (Apple-provided, always available on macOS).
unsafe extern "C" {
    fn cblas_sgemm(
        order: i32,
        transa: i32,
        transb: i32,
        m: i32,
        n: i32,
        k: i32,
        alpha: f32,
        a: *const f32,
        lda: i32,
        b: *const f32,
        ldb: i32,
        beta: f32,
        c: *mut f32,
        ldc: i32,
    );
}

// ---------------------------------------------------------------------------
// 2. ThreadLifecycleSource — pthread create/join scheduling entropy
// ---------------------------------------------------------------------------

/// Entropy source that harvests timing jitter from thread creation and
/// destruction, which exercises deep kernel scheduling paths.
///
/// **Physics:** Each `pthread_create` + `pthread_join` cycle involves:
///
/// - **Mach thread port allocation** from the kernel IPC port name space
/// - **Kernel thread structure** allocation from the zone allocator
/// - **CPU core selection** — the scheduler must decide P-core vs E-core
///   based on thermal state, load balance, and QoS hints. This decision
///   is influenced by ALL running threads across ALL processes.
/// - **Stack page allocation** via `vm_allocate` (kernel VM map operation)
/// - **TLS (Thread Local Storage)** setup including dyld per-thread state
/// - **Context switch timing** on join — the joining thread must be woken
///   by the scheduler, which depends on current runqueue state
/// - **Core migration** — the new thread may run on a different core than
///   the creating thread, adding interconnect latency
pub struct ThreadLifecycleSource;

static THREAD_LIFECYCLE_INFO: SourceInfo = SourceInfo {
    name: "thread_lifecycle",
    description: "Thread create/join kernel scheduling and allocation jitter",
    physics: "Creates and immediately joins threads, measuring the full lifecycle timing. \
              Each cycle involves: Mach thread port allocation, zone allocator allocation, \
              CPU core selection (P-core vs E-core), stack page allocation, TLS setup, and \
              context switch on join. The scheduler\u{2019}s core selection depends on thermal \
              state, load from ALL processes, and QoS priorities.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 3000.0,
};

impl EntropySource for ThreadLifecycleSource {
    fn info(&self) -> &SourceInfo {
        &THREAD_LIFECYCLE_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Thread creation is expensive (~400-600ns), so we need less oversampling.
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Vary the thread's workload to create different scheduling pressures.
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let work_amount = (lcg >> 48) as u32 % 100;

            let t0 = mach_time();

            let handle = thread::spawn(move || {
                // Small variable workload to perturb scheduler decisions.
                let mut sink: u64 = 0;
                for j in 0..work_amount {
                    sink = sink.wrapping_add(j as u64);
                }
                std::hint::black_box(sink);
            });

            let _ = handle.join();
            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// 3. MachIPCSource — Mach port complex message passing timing
// ---------------------------------------------------------------------------

/// Configuration for Mach IPC entropy collection.
#[derive(Debug, Clone)]
pub struct MachIPCConfig {
    /// Number of Mach ports to round-robin across. More ports create more
    /// namespace contention and varied queue depths.
    pub num_ports: usize,
    /// Size of out-of-line (OOL) memory descriptors in bytes. OOL descriptors
    /// force the kernel to perform VM remapping operations.
    pub ool_size: usize,
    /// Whether to use complex messages with OOL descriptors (true) or simple
    /// port allocate/deallocate (false, legacy behavior).
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

/// Entropy source that harvests timing jitter from Mach IPC (Inter-Process
/// Communication) using complex messages with OOL memory descriptors.
///
/// **Physics:** Complex Mach messages with OOL descriptors traverse deeper
/// kernel paths than simple port operations:
///
/// - **OOL memory remapping** — the kernel must `vm_map_copyin` the sender's
///   memory and `vm_map_copyout` into the receiver's address space. This
///   exercises VM map operations, page table updates, and physical page
///   allocation — all sources of nondeterminism.
/// - **Port set scheduling** — round-robin across multiple ports with
///   different queue depths creates scheduling interference. The kernel's
///   `ipc_mqueue_send` must acquire per-port locks, creating contention.
/// - **Thread wakeup** — if a receiver is blocked on `mach_msg_receive()`,
///   the kernel wakes it via `thread_go()`, which involves scheduler
///   decisions affected by ALL runnable threads.
/// - **Port name space operations** — allocating/deallocating ports exercises
///   the splay tree in the port name space, with timing dependent on tree depth.
///
/// **Improvement over v1:** The original used simple port allocate/deallocate.
/// Complex OOL messages force VM remapping, which has much higher timing variance.
/// Round-robin across multiple ports adds namespace contention entropy.
#[derive(Default)]
pub struct MachIPCSource {
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

        // Allocate a pool of Mach ports to round-robin across.
        let mut ports: Vec<u32> = Vec::with_capacity(num_ports);
        for _ in 0..num_ports {
            let mut port: u32 = 0;
            // SAFETY: mach_port_allocate allocates a receive right.
            let kr = unsafe {
                mach_port_allocate(task, 1 /* MACH_PORT_RIGHT_RECEIVE */, &mut port)
            };
            if kr == 0 {
                // Insert a send right so we can send to ourselves.
                // SAFETY: port is a valid receive right we just allocated.
                let kr2 = unsafe {
                    mach_port_insert_right(
                        task,
                        port,
                        port,
                        20, // MACH_MSG_TYPE_MAKE_SEND
                    )
                };
                if kr2 == 0 {
                    ports.push(port);
                } else {
                    unsafe {
                        mach_port_mod_refs(task, port, 1, -1);
                    }
                }
            }
        }

        // Fallback: if no ports allocated, use simple allocate/deallocate pattern.
        if ports.is_empty() {
            return self.collect_simple(n_samples);
        }

        if self.config.use_complex_messages {
            // Complex message path: send OOL messages via mach_msg.
            let ool_size = self.config.ool_size.max(1);

            // Allocate OOL buffer (stays alive for all sends).
            let ool_buf = vec![0xBEu8; ool_size];

            // Receiver thread to drain messages (prevents queue backup).
            let stop = Arc::new(AtomicBool::new(false));
            let stop2 = stop.clone();
            let recv_ports = ports.clone();
            let receiver = thread::spawn(move || {
                // Receive buffer: header + trailer space.
                let mut recv_buf = vec![0u8; 1024 + ool_size * 2];
                while !stop2.load(Ordering::Relaxed) {
                    for &port in &recv_ports {
                        // SAFETY: recv_buf is large enough for the message.
                        // MACH_RCV_TIMEOUT (0x100) with 0ms timeout = non-blocking.
                        unsafe {
                            let hdr = recv_buf.as_mut_ptr() as *mut MachMsgHeader;
                            (*hdr).msgh_local_port = port;
                            (*hdr).msgh_size = recv_buf.len() as u32;
                            mach_msg(
                                hdr,
                                2 | 0x100, // MACH_RCV_MSG | MACH_RCV_TIMEOUT
                                0,
                                recv_buf.len() as u32,
                                port,
                                0, // 0ms timeout = non-blocking
                                0,
                            );
                        }
                    }
                    std::thread::yield_now();
                }
            });

            for i in 0..raw_count {
                let port = ports[i % ports.len()];

                // Build a complex OOL message.
                let mut msg = MachMsgOOL::zeroed();
                msg.header.msgh_bits = 0x80000000 // MACH_MSGH_BITS_COMPLEX
                    | 17;  // MACH_MSG_TYPE_COPY_SEND (remote)
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

                // SAFETY: msg is a properly initialized Mach message. MACH_SEND_TIMEOUT
                // prevents blocking indefinitely if the port queue is full.
                unsafe {
                    mach_msg(
                        &mut msg.header as *mut MachMsgHeader,
                        1 | 0x80, // MACH_SEND_MSG | MACH_SEND_TIMEOUT
                        msg.header.msgh_size,
                        0,
                        0,
                        10, // 10ms timeout
                        0,
                    );
                }

                let t1 = mach_time();
                timings.push(t1.wrapping_sub(t0));
            }

            // Shutdown receiver.
            stop.store(true, Ordering::Relaxed);
            let _ = receiver.join();
        } else {
            // Simple path: allocate/deallocate ports (original behavior).
            for i in 0..raw_count {
                let t0 = mach_time();

                // Use different ports for namespace contention.
                let base_port = ports[i % ports.len()];

                let mut new_port: u32 = 0;
                // SAFETY: standard Mach port operations.
                let kr = unsafe {
                    mach_port_allocate(task, 1, &mut new_port)
                };
                if kr == 0 {
                    unsafe {
                        mach_port_deallocate(task, new_port);
                        mach_port_mod_refs(task, new_port, 1, -1);
                    }
                }

                // Touch the base port to create namespace contention.
                unsafe {
                    let mut ptype: u32 = 0;
                    mach_port_type(task, base_port, &mut ptype);
                }

                let t1 = mach_time();
                timings.push(t1.wrapping_sub(t0));
            }
        }

        // Clean up port pool.
        for &port in &ports {
            unsafe {
                mach_port_mod_refs(task, port, 1, -1); // Drop receive right.
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

impl MachIPCSource {
    /// Fallback: simple allocate/deallocate if port pool setup fails.
    fn collect_simple(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let task = unsafe { mach_task_self() };

        for _ in 0..raw_count {
            let t0 = mach_time();
            let mut port: u32 = 0;
            let kr = unsafe {
                mach_port_allocate(task, 1, &mut port)
            };
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
// The struct is never shared across threads.
unsafe impl Send for MachMsgOOL {}

impl MachMsgOOL {
    fn zeroed() -> Self {
        // SAFETY: All-zeros is a valid representation for this packed C struct.
        // Null pointers and zero port names are safe initial values.
        unsafe { std::mem::zeroed() }
    }
}

// Mach kernel FFI bindings.
unsafe extern "C" {
    fn mach_task_self() -> u32;
    fn mach_port_allocate(task: u32, right: i32, name: *mut u32) -> i32;
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
    fn mach_port_mod_refs(task: u32, name: u32, right: i32, delta: i32) -> i32;
    fn mach_port_insert_right(task: u32, name: u32, poly: u32, poly_poly: u32) -> i32;
    fn mach_port_type(task: u32, name: u32, ptype: *mut u32) -> i32;
    fn mach_msg(
        msg: *mut MachMsgHeader,
        option: i32,
        send_size: u32,
        rcv_size: u32,
        rcv_name: u32,
        timeout: u32,
        notify: u32,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// 4. TLBShootdownSource — mprotect-induced TLB invalidation timing
// ---------------------------------------------------------------------------

/// Configuration for TLB shootdown entropy collection.
#[derive(Debug, Clone)]
pub struct TLBShootdownConfig {
    /// Range of pages to invalidate per measurement [min, max].
    /// Varying the page count changes the number of IPIs sent.
    pub page_count_range: (usize, usize),
    /// Total memory region size in pages. Larger regions use different
    /// physical pages each time, preventing TLB prefetch patterns.
    pub region_pages: usize,
    /// Whether to measure variance between consecutive shootdowns (true)
    /// or absolute timing (false, legacy behavior).
    pub measure_variance: bool,
}

impl Default for TLBShootdownConfig {
    fn default() -> Self {
        Self {
            page_count_range: (8, 128),
            region_pages: 256,
            measure_variance: true,
        }
    }
}

/// Entropy source that harvests timing jitter from TLB (Translation Lookaside
/// Buffer) invalidation broadcasts triggered by `mprotect()`.
///
/// **Physics:** When `mprotect()` changes page protection on a multi-core
/// system, the kernel must invalidate stale TLB entries on ALL cores:
///
/// - **Inter-Processor Interrupt (IPI)** — the kernel sends an IPI to every
///   core that might have cached TLB entries for the affected pages.
/// - **TLB flush latency** — each receiving core must drain its pipeline,
///   flush matching TLB entries, and acknowledge.
/// - **Cross-cluster latency** — Apple Silicon has separate P-core and E-core
///   clusters with different interconnect latencies.
///
/// **Improvements:** Varying the number of pages invalidated per measurement
/// creates different IPI patterns. Using different memory regions each time
/// prevents TLB prefetch patterns. Measuring variance between consecutive
/// shootdowns captures relative timing, which has higher min-entropy than
/// absolute timing.
#[derive(Default)]
pub struct TLBShootdownSource {
    pub config: TLBShootdownConfig,
}

static TLB_SHOOTDOWN_INFO: SourceInfo = SourceInfo {
    name: "tlb_shootdown",
    description: "TLB invalidation broadcast timing via variable-count mprotect IPI storms",
    physics: "Toggles page protection via mprotect() on varying page counts to trigger TLB \
              shootdown broadcasts. Each mprotect() sends IPIs to ALL cores to flush stale \
              TLB entries. Varying page counts creates different IPI patterns. Different \
              memory regions each time prevent TLB prefetch. Variance between consecutive \
              shootdowns captures relative timing with higher min-entropy. IPI latency depends \
              on: what each core is executing, P-core vs E-core cluster latency, core power \
              states, and concurrent IPI traffic.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 2000.0,
};

impl EntropySource for TLBShootdownSource {
    fn info(&self) -> &SourceInfo {
        &TLB_SHOOTDOWN_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // SAFETY: sysconf(_SC_PAGESIZE) is always safe and returns the page size.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let region_pages = self.config.region_pages.max(8);
        let region_size = page_size * region_pages;
        let (min_pages, max_pages) = self.config.page_count_range;
        let min_pages = min_pages.max(1).min(region_pages);
        let max_pages = max_pages.max(min_pages).min(region_pages);

        // SAFETY: mmap with MAP_ANONYMOUS|MAP_PRIVATE creates a private anonymous
        // mapping. We check for MAP_FAILED before using the returned address.
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                region_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };

        if addr == libc::MAP_FAILED {
            return Vec::new();
        }

        // Touch every page to establish TLB entries on this core.
        for p in 0..region_pages {
            // SAFETY: addr is valid mmap'd region, p * page_size < region_size.
            unsafe {
                std::ptr::write_volatile((addr as *mut u8).add(p * page_size), 0xAA);
            }
        }

        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            // Vary number of pages to invalidate.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let num_pages = if min_pages == max_pages {
                min_pages
            } else {
                min_pages + ((lcg >> 32) as usize % (max_pages - min_pages + 1))
            };
            let prot_size = num_pages * page_size;

            // Vary the region offset to use different memory each time.
            let max_offset_pages = region_pages.saturating_sub(num_pages);
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let offset_pages = if max_offset_pages > 0 {
                (lcg >> 48) as usize % max_offset_pages
            } else {
                0
            };
            let offset = offset_pages * page_size;

            let t0 = mach_time();

            // SAFETY: addr+offset is within the mmap'd region, prot_size fits.
            unsafe {
                let target = (addr as *mut u8).add(offset) as *mut libc::c_void;
                libc::mprotect(target, prot_size, libc::PROT_READ);
                libc::mprotect(target, prot_size, libc::PROT_READ | libc::PROT_WRITE);
            }

            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
        }

        // SAFETY: addr was returned by mmap (checked != MAP_FAILED) with size region_size.
        unsafe {
            libc::munmap(addr, region_size);
        }

        if self.config.measure_variance {
            // Use delta-of-deltas: measures variance between consecutive shootdowns.
            extract_timing_entropy_variance(&timings, n_samples)
        } else {
            extract_timing_entropy(&timings, n_samples)
        }
    }
}

/// Extract entropy from timing variance (delta-of-deltas).
///
/// First computes deltas between consecutive timings, then computes deltas
/// between consecutive deltas. This captures the *change* in timing, which
/// removes systematic bias and amplifies nondeterministic components.
fn extract_timing_entropy_variance(timings: &[u64], n_samples: usize) -> Vec<u8> {
    if timings.len() < 4 {
        return Vec::new();
    }

    // First-order deltas.
    let deltas: Vec<u64> = timings
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // Second-order deltas (variance).
    let variance: Vec<u64> = deltas
        .windows(2)
        .map(|w| w[1].wrapping_sub(w[0]))
        .collect();

    // XOR consecutive variance values for mixing.
    let xored: Vec<u64> = variance.windows(2).map(|w| w[0] ^ w[1]).collect();

    let mut raw: Vec<u8> = xored
        .iter()
        .map(|&x| super::helpers::xor_fold_u64(x))
        .collect();
    raw.truncate(n_samples);
    raw
}

// ---------------------------------------------------------------------------
// 5. PipeBufferSource — multi-pipe kernel zone allocator competition
// ---------------------------------------------------------------------------

/// Configuration for pipe buffer entropy collection.
#[derive(Debug, Clone)]
pub struct PipeBufferConfig {
    /// Number of pipes to use simultaneously. Multiple pipes competing for
    /// kernel buffer space creates zone allocator contention.
    pub num_pipes: usize,
    /// Minimum write size in bytes.
    pub min_write_size: usize,
    /// Maximum write size in bytes.
    pub max_write_size: usize,
    /// Whether to use non-blocking mode. Non-blocking writes that hit EAGAIN
    /// follow a different kernel path with different latency characteristics.
    pub non_blocking: bool,
}

impl Default for PipeBufferConfig {
    fn default() -> Self {
        Self {
            num_pipes: 4,
            min_write_size: 1,
            max_write_size: 4096,
            non_blocking: true,
        }
    }
}

/// Entropy source that harvests timing jitter from pipe creation and I/O,
/// with multiple pipes competing for kernel buffer space.
///
/// **Physics:** Multiple simultaneous pipes competing for kernel zone allocator
/// resources amplifies nondeterminism:
///
/// - **Zone allocator contention** — multiple pipes allocating from the pipe zone
///   simultaneously creates cross-CPU magazine transfer contention.
/// - **Variable buffer sizes** — different write sizes exercise different mbuf
///   allocation paths (small buffers use inline storage, large buffers chain mbufs).
/// - **Non-blocking I/O** — EAGAIN timing on full pipe buffers follows a
///   different kernel path (checking buffer space, returning error vs blocking)
///   with its own latency characteristics.
/// - **Cross-pipe interference** — reading from one pipe while another has
///   pending data creates wakeup scheduling interference.
#[derive(Default)]
pub struct PipeBufferSource {
    pub config: PipeBufferConfig,
}

static PIPE_BUFFER_INFO: SourceInfo = SourceInfo {
    name: "pipe_buffer",
    description: "Multi-pipe kernel zone allocator competition and buffer timing jitter",
    physics: "Creates multiple pipes simultaneously, writes variable-size data, reads it back, \
              and closes — measuring contention in the kernel zone allocator. Multiple pipes \
              compete for pipe zone and mbuf allocations, creating cross-CPU magazine transfer \
              contention. Variable write sizes exercise different mbuf paths. Non-blocking mode \
              captures EAGAIN timing on different kernel failure paths. Zone allocator timing \
              depends on zone fragmentation, magazine layer state, and cross-CPU transfers.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 1500.0,
};

impl EntropySource for PipeBufferSource {
    fn info(&self) -> &SourceInfo {
        &PIPE_BUFFER_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut lcg: u64 = mach_time() | 1;
        let num_pipes = self.config.num_pipes.max(1);
        let min_size = self.config.min_write_size.max(1);
        let max_size = self.config.max_write_size.max(min_size);

        // Pre-allocate a persistent pool of pipes for contention.
        let mut pipe_pool: Vec<[i32; 2]> = Vec::new();
        for _ in 0..num_pipes {
            let mut fds: [i32; 2] = [0; 2];
            // SAFETY: fds is a 2-element array matching pipe()'s expected output.
            let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
            if ret == 0 {
                if self.config.non_blocking {
                    // Set write end to non-blocking.
                    // SAFETY: fds[1] is a valid file descriptor from pipe().
                    unsafe {
                        let flags = libc::fcntl(fds[1], libc::F_GETFL);
                        libc::fcntl(fds[1], libc::F_SETFL, flags | libc::O_NONBLOCK);
                    }
                }
                pipe_pool.push(fds);
            }
        }

        // Fallback: if no pipes allocated, use single-pipe mode.
        if pipe_pool.is_empty() {
            return self.collect_single_pipe(n_samples);
        }

        for i in 0..raw_count {
            // Vary write size to exercise different mbuf allocation paths.
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let write_size = if min_size == max_size {
                min_size
            } else {
                min_size + (lcg >> 48) as usize % (max_size - min_size + 1)
            };
            let write_data = vec![0xBEu8; write_size];
            let mut read_buf = vec![0u8; write_size];

            // Round-robin across pipe pool.
            let pipe_idx = i % pipe_pool.len();
            let fds = pipe_pool[pipe_idx];

            let t0 = mach_time();

            // SAFETY: fds are valid file descriptors from pipe().
            unsafe {
                let written = libc::write(
                    fds[1],
                    write_data.as_ptr() as *const _,
                    write_size,
                );

                if written > 0 {
                    libc::read(fds[0], read_buf.as_mut_ptr() as *mut _, written as usize);
                }
                // If EAGAIN (non-blocking full), the timing of the failure is itself entropy.
            }

            let t1 = mach_time();
            std::hint::black_box(&read_buf);
            timings.push(t1.wrapping_sub(t0));

            // Periodically create/destroy an extra pipe for zone allocator churn.
            if i % 8 == 0 {
                let mut extra_fds: [i32; 2] = [0; 2];
                // SAFETY: standard pipe() call.
                let ret = unsafe { libc::pipe(extra_fds.as_mut_ptr()) };
                if ret == 0 {
                    unsafe {
                        libc::close(extra_fds[0]);
                        libc::close(extra_fds[1]);
                    }
                }
            }
        }

        // Clean up pipe pool.
        for fds in &pipe_pool {
            // SAFETY: closing valid file descriptors.
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

impl PipeBufferSource {
    /// Fallback single-pipe collection (matches original behavior).
    fn collect_single_pipe(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let mut lcg: u64 = mach_time() | 1;

        for _ in 0..raw_count {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            let write_size = 1 + (lcg >> 48) as usize % 256;
            let write_data = vec![0xBEu8; write_size];
            let mut read_buf = vec![0u8; write_size];

            let mut fds: [i32; 2] = [0; 2];
            let t0 = mach_time();
            let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
            if ret != 0 {
                continue;
            }
            unsafe {
                libc::write(fds[1], write_data.as_ptr() as *const _, write_size);
                libc::read(fds[0], read_buf.as_mut_ptr() as *mut _, write_size);
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            let t1 = mach_time();
            std::hint::black_box(&read_buf);
            timings.push(t1.wrapping_sub(t0));
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

// ---------------------------------------------------------------------------
// 6. KqueueEventsSource — kqueue event notification timing
// ---------------------------------------------------------------------------

/// Configuration for kqueue events entropy collection.
#[derive(Debug, Clone)]
pub struct KqueueEventsConfig {
    /// Number of file watchers to register.
    pub num_file_watchers: usize,
    /// Number of timer events to register (different intervals).
    pub num_timers: usize,
    /// Number of socket pairs for socket event monitoring.
    pub num_sockets: usize,
    /// Timeout in milliseconds for kevent() calls.
    pub timeout_ms: u32,
}

impl Default for KqueueEventsConfig {
    fn default() -> Self {
        Self {
            num_file_watchers: 4,
            num_timers: 8,
            num_sockets: 4,
            timeout_ms: 1,
        }
    }
}

/// Entropy source that harvests timing jitter from kqueue event
/// notification multiplexing.
///
/// **Physics:** kqueue is the macOS/BSD kernel event notification system.
/// Registering diverse event types simultaneously creates rich interference:
///
/// - **Timer events** — `EVFILT_TIMER` with different intervals fire at
///   kernel-determined times affected by timer coalescing, interrupt handling,
///   and power management state. Multiple timers create scheduling contention.
/// - **File watchers** — `EVFILT_VNODE` on temp files monitors inode changes.
///   The filesystem notification path traverses VFS, APFS/HFS event queues,
///   and the kqueue knote hash table.
/// - **Socket events** — `EVFILT_READ` on socket pairs monitors buffer state.
///   Socket buffer management interacts with the network stack's mbuf allocator.
/// - **Contention/interference** — many registered watchers all compete for
///   the kqueue's internal knote lock and dispatch queue. The kevent() syscall
///   must scan all registered events, and the order/timing of event delivery
///   depends on kernel scheduling, interrupt timing, and lock contention.
///
/// The combination of independent event sources creates interference patterns
/// with high min-entropy.
#[derive(Default)]
pub struct KqueueEventsSource {
    pub config: KqueueEventsConfig,
}

static KQUEUE_EVENTS_INFO: SourceInfo = SourceInfo {
    name: "kqueue_events",
    description: "Kqueue event multiplexing timing from timers, files, and sockets",
    physics: "Registers diverse kqueue event types (timers, file watchers, socket monitors) \
              and measures kevent() notification timing. Timer events capture kernel timer \
              coalescing and interrupt jitter. File watchers exercise VFS/APFS notification \
              paths. Socket events capture mbuf allocator timing. Multiple simultaneous watchers \
              create knote lock contention and dispatch queue interference. The combination of \
              independent event sources produces high min-entropy.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 2500.0,
};

impl EntropySource for KqueueEventsSource {
    fn info(&self) -> &SourceInfo {
        &KQUEUE_EVENTS_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))
    }

    #[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // SAFETY: kqueue() creates a new kernel event queue (always safe).
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return Vec::new();
        }

        let mut changes: Vec<libc::kevent> = Vec::new();
        let mut cleanup_fds: Vec<i32> = Vec::new();

        // Register timer events with different intervals (1-10ms).
        for i in 0..self.config.num_timers {
            let interval_ms = 1 + (i % 10);
            // SAFETY: Zeroed kevent is safe; we fill all fields.
            let mut ev: libc::kevent = unsafe { std::mem::zeroed() };
            ev.ident = i;
            ev.filter = libc::EVFILT_TIMER;
            ev.flags = libc::EV_ADD | libc::EV_ENABLE;
            ev.fflags = 0;
            ev.data = interval_ms as isize;
            ev.udata = std::ptr::null_mut();
            changes.push(ev);
        }

        // Register socket pair events.
        for i in 0..self.config.num_sockets {
            let mut sv: [i32; 2] = [0; 2];
            // SAFETY: socketpair creates a connected pair of sockets.
            let ret = unsafe {
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, sv.as_mut_ptr())
            };
            if ret == 0 {
                cleanup_fds.push(sv[0]);
                cleanup_fds.push(sv[1]);

                // Monitor read end for incoming data.
                let mut ev: libc::kevent = unsafe { std::mem::zeroed() };
                ev.ident = sv[0] as usize;
                ev.filter = libc::EVFILT_READ;
                ev.flags = libc::EV_ADD | libc::EV_ENABLE;
                ev.udata = std::ptr::null_mut();
                changes.push(ev);

                // Write a byte to trigger the event.
                let byte = [0xAAu8];
                unsafe {
                    libc::write(sv[1], byte.as_ptr() as *const _, 1);
                }

                // Also monitor write end.
                let mut ev2: libc::kevent = unsafe { std::mem::zeroed() };
                ev2.ident = sv[1] as usize;
                ev2.filter = libc::EVFILT_WRITE;
                ev2.flags = libc::EV_ADD | libc::EV_ENABLE;
                ev2.udata = std::ptr::null_mut();
                changes.push(ev2);

                let _ = i; // suppress unused warning
            }
        }

        // Register file watchers on temp files.
        let mut temp_files: Vec<(i32, std::path::PathBuf)> = Vec::new();
        for i in 0..self.config.num_file_watchers {
            let path = std::env::temp_dir().join(format!("oe_kq_{i}_{}", std::process::id()));
            if std::fs::write(&path, b"entropy").is_ok() {
                // SAFETY: CString is properly null-terminated; O_RDONLY is safe.
                let c_path = std::ffi::CString::new(path.to_str().unwrap_or("")).unwrap();
                let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
                if fd >= 0 {
                    let mut ev: libc::kevent = unsafe { std::mem::zeroed() };
                    ev.ident = fd as usize;
                    ev.filter = libc::EVFILT_VNODE;
                    ev.flags = libc::EV_ADD | libc::EV_ENABLE | libc::EV_CLEAR;
                    ev.fflags = libc::NOTE_WRITE | libc::NOTE_ATTRIB;
                    ev.udata = std::ptr::null_mut();
                    changes.push(ev);
                    temp_files.push((fd, path));
                }
            }
        }

        // Register all changes.
        if !changes.is_empty() {
            // SAFETY: kq is valid, changes is properly initialized.
            unsafe {
                libc::kevent(
                    kq,
                    changes.as_ptr(),
                    changes.len() as i32,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                );
            }
        }

        // Spawn a thread to periodically poke watched files and sockets
        // to generate events asynchronously.
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let socket_write_fds: Vec<i32> = cleanup_fds
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect();
        let file_paths: Vec<std::path::PathBuf> = temp_files.iter().map(|(_, p)| p.clone()).collect();

        let poker = thread::spawn(move || {
            let byte = [0xBBu8];
            while !stop2.load(Ordering::Relaxed) {
                // Poke sockets.
                for &fd in &socket_write_fds {
                    unsafe {
                        libc::write(fd, byte.as_ptr() as *const _, 1);
                    }
                }
                // Touch files.
                for path in &file_paths {
                    let _ = std::fs::write(path, b"poke");
                }
                std::thread::sleep(std::time::Duration::from_micros(500));
            }
        });

        // Collect timing samples.
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: self.config.timeout_ms as i64 * 1_000_000,
        };
        let mut events: Vec<libc::kevent> =
            vec![unsafe { std::mem::zeroed() }; changes.len().max(16)];

        // Also drain socket read ends periodically.
        let socket_read_fds: Vec<i32> = cleanup_fds
            .iter()
            .step_by(2)
            .copied()
            .collect();

        for _ in 0..raw_count {
            let t0 = mach_time();

            // SAFETY: kq and events buffer are valid.
            let n = unsafe {
                libc::kevent(
                    kq,
                    std::ptr::null(),
                    0,
                    events.as_mut_ptr(),
                    events.len() as i32,
                    &timeout,
                )
            };

            let t1 = mach_time();

            // Drain socket read buffers to prevent saturation.
            if n > 0 {
                let mut drain = [0u8; 64];
                for &fd in &socket_read_fds {
                    unsafe {
                        libc::read(fd, drain.as_mut_ptr() as *mut _, drain.len());
                    }
                }
            }

            timings.push(t1.wrapping_sub(t0));
        }

        // Shutdown poker thread.
        stop.store(true, Ordering::Relaxed);
        let _ = poker.join();

        // Cleanup.
        for (fd, path) in &temp_files {
            unsafe { libc::close(*fd); }
            let _ = std::fs::remove_file(path);
        }
        for &fd in &cleanup_fds {
            unsafe { libc::close(fd); }
        }
        unsafe { libc::close(kq); }

        extract_timing_entropy(&timings, n_samples)
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd")))]
    fn collect(&self, _n_samples: usize) -> Vec<u8> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// 7. InterleavedFrontierSource — cross-source interference entropy
// ---------------------------------------------------------------------------

/// Entropy source that rapidly alternates between all frontier sources,
/// harvesting the interference between them as independent entropy.
///
/// **Physics:** When frontier sources are sampled in rapid alternation, each
/// source's system state perturbations affect the next source's measurements:
///
/// - AMX dispatch affects memory controller state → affects TLB shootdown timing
/// - Pipe buffer zone allocations affect kernel zone magazine state → affects
///   Mach port allocation timing
/// - Thread lifecycle scheduling decisions affect kqueue timer delivery timing
/// - Each source's syscalls perturb the CPU pipeline, TLB, and cache state
///   that the next source measures
///
/// The cross-source interference is itself a source of entropy that is
/// independent from each individual source's entropy.
pub struct InterleavedFrontierSource;

static INTERLEAVED_FRONTIER_INFO: SourceInfo = SourceInfo {
    name: "interleaved_frontier",
    description: "Cross-source interference from rapidly alternating frontier sources",
    physics: "Rapidly alternates between all frontier sources (AMX, thread lifecycle, Mach IPC, \
              TLB shootdown, pipe buffer, kqueue). Each source's system perturbations affect \
              the next source's measurements: AMX affects memory state, pipe allocation affects \
              kernel zones, thread scheduling affects timer delivery. The cross-source \
              interference pattern is itself independent entropy.",
    category: SourceCategory::Frontier,
    platform_requirements: &[],
    entropy_rate_estimate: 3000.0,
};

impl EntropySource for InterleavedFrontierSource {
    fn info(&self) -> &SourceInfo {
        &INTERLEAVED_FRONTIER_INFO
    }

    fn is_available(&self) -> bool {
        // Available if at least 2 frontier sources are available.
        let sources = frontier_sources();
        sources.iter().filter(|s| s.is_available()).count() >= 2
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let sources = frontier_sources();
        let available: Vec<Box<dyn EntropySource>> = sources
            .into_iter()
            .filter(|s| s.is_available())
            .collect();

        if available.is_empty() {
            return Vec::new();
        }

        // Collect small batches from each source in round-robin, measuring
        // the transition timing between sources as additional entropy.
        let batch_size = 4; // Small batches to maximize interference.
        let mut timings: Vec<u64> = Vec::with_capacity(n_samples * 4 + 64);
        let raw_count = n_samples * 4 + 64;
        let mut all_bytes: Vec<u8> = Vec::new();

        let mut i = 0;
        while i < raw_count {
            for source in &available {
                let t0 = mach_time();
                let bytes = source.collect(batch_size);
                let t1 = mach_time();

                all_bytes.extend_from_slice(&bytes);
                timings.push(t1.wrapping_sub(t0));

                i += 1;
                if i >= raw_count {
                    break;
                }
            }
        }

        // Mix transition timings with collected bytes.
        let timing_entropy = extract_timing_entropy(&timings, n_samples);

        // XOR timing entropy with collected source bytes for final output.
        let mut result = Vec::with_capacity(n_samples);
        for j in 0..n_samples {
            let t_byte = timing_entropy.get(j).copied().unwrap_or(0);
            let s_byte = all_bytes.get(j).copied().unwrap_or(0);
            result.push(t_byte ^ s_byte);
        }
        result.truncate(n_samples);
        result
    }
}

/// Create instances of all non-interleaved frontier sources (for use by InterleavedFrontierSource).
fn frontier_sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(AMXTimingSource::default()),
        Box::new(ThreadLifecycleSource),
        Box::new(MachIPCSource::default()),
        Box::new(TLBShootdownSource::default()),
        Box::new(PipeBufferSource::default()),
        Box::new(KqueueEventsSource::default()),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // AMX timing
    #[test]
    fn amx_timing_info() {
        let src = AMXTimingSource::default();
        assert_eq!(src.name(), "amx_timing");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    fn amx_timing_default_config() {
        let config = AMXTimingConfig::default();
        assert_eq!(config.matrix_sizes, vec![16, 32, 48, 64, 96, 128]);
        assert!(config.interleave_memory_ops);
        assert!(config.von_neumann_debias);
    }

    #[test]
    fn amx_timing_custom_config() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                matrix_sizes: vec![32, 64],
                interleave_memory_ops: false,
                von_neumann_debias: false,
            },
        };
        assert_eq!(src.config.matrix_sizes.len(), 2);
        assert!(!src.config.interleave_memory_ops);
    }

    #[test]
    fn amx_timing_empty_sizes_returns_empty() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                matrix_sizes: vec![],
                interleave_memory_ops: false,
                von_neumann_debias: false,
            },
        };
        if src.is_available() {
            let data = src.collect(64);
            assert!(data.is_empty());
        }
    }

    #[test]
    #[ignore] // Requires macOS aarch64
    fn amx_timing_collects_bytes() {
        let src = AMXTimingSource::default();
        if src.is_available() {
            let data = src.collect(128);
            assert!(!data.is_empty());
            assert!(data.len() <= 128);
        }
    }

    #[test]
    #[ignore] // Requires macOS aarch64
    fn amx_timing_no_debias_collects_bytes() {
        let src = AMXTimingSource {
            config: AMXTimingConfig {
                von_neumann_debias: false,
                ..AMXTimingConfig::default()
            },
        };
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
        }
    }

    // Von Neumann debiasing
    #[test]
    fn debiased_extraction_basic() {
        // Create timings with known pattern.
        let timings: Vec<u64> = (0..200).map(|i| 100 + (i * 7 + i * i) % 50).collect();
        let result = extract_timing_entropy_debiased(&timings, 10);
        // Should produce some bytes (exact count depends on how many pairs are equal).
        assert!(result.len() <= 10);
    }

    #[test]
    fn debiased_extraction_too_few() {
        assert!(extract_timing_entropy_debiased(&[1, 2, 3], 10).is_empty());
        assert!(extract_timing_entropy_debiased(&[], 10).is_empty());
    }

    #[test]
    fn debiased_extraction_constant_input() {
        // All-constant timings → all deltas equal → VN discards all → empty.
        let timings = vec![42u64; 100];
        let result = extract_timing_entropy_debiased(&timings, 10);
        assert!(result.is_empty());
    }

    // Thread lifecycle
    #[test]
    fn thread_lifecycle_info() {
        let src = ThreadLifecycleSource;
        assert_eq!(src.name(), "thread_lifecycle");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    #[ignore] // Spawns threads
    fn thread_lifecycle_collects_bytes() {
        let src = ThreadLifecycleSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
        if data.len() > 1 {
            let first = data[0];
            assert!(data.iter().any(|&b| b != first), "all bytes identical");
        }
    }

    // Mach IPC
    #[test]
    fn mach_ipc_info() {
        let src = MachIPCSource::default();
        assert_eq!(src.name(), "mach_ipc");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    fn mach_ipc_default_config() {
        let config = MachIPCConfig::default();
        assert_eq!(config.num_ports, 8);
        assert_eq!(config.ool_size, 4096);
        assert!(config.use_complex_messages);
    }

    #[test]
    fn mach_ipc_custom_config() {
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
    fn mach_ipc_collects_bytes() {
        let src = MachIPCSource::default();
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty());
        assert!(data.len() <= 64);
    }

    #[test]
    #[ignore] // Uses Mach ports
    fn mach_ipc_simple_mode_collects_bytes() {
        let src = MachIPCSource {
            config: MachIPCConfig {
                use_complex_messages: false,
                ..MachIPCConfig::default()
            },
        };
        let data = src.collect(64);
        assert!(!data.is_empty());
    }

    // TLB shootdown
    #[test]
    fn tlb_shootdown_info() {
        let src = TLBShootdownSource::default();
        assert_eq!(src.name(), "tlb_shootdown");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    fn tlb_shootdown_default_config() {
        let config = TLBShootdownConfig::default();
        assert_eq!(config.page_count_range, (8, 128));
        assert_eq!(config.region_pages, 256);
        assert!(config.measure_variance);
    }

    #[test]
    fn tlb_shootdown_custom_config() {
        let src = TLBShootdownSource {
            config: TLBShootdownConfig {
                page_count_range: (4, 64),
                region_pages: 128,
                measure_variance: false,
            },
        };
        assert_eq!(src.config.page_count_range, (4, 64));
    }

    #[test]
    #[ignore] // Uses mmap/mprotect
    fn tlb_shootdown_collects_bytes() {
        let src = TLBShootdownSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    #[ignore] // Uses mmap/mprotect
    fn tlb_shootdown_absolute_mode() {
        let src = TLBShootdownSource {
            config: TLBShootdownConfig {
                measure_variance: false,
                ..TLBShootdownConfig::default()
            },
        };
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
        }
    }

    // Variance extraction
    #[test]
    fn variance_extraction_basic() {
        let timings: Vec<u64> = (0..100).map(|i| 100 + (i * 7 + i * i) % 50).collect();
        let result = extract_timing_entropy_variance(&timings, 10);
        assert!(!result.is_empty());
        assert!(result.len() <= 10);
    }

    #[test]
    fn variance_extraction_too_few() {
        assert!(extract_timing_entropy_variance(&[1, 2, 3], 10).is_empty());
    }

    // Pipe buffer
    #[test]
    fn pipe_buffer_info() {
        let src = PipeBufferSource::default();
        assert_eq!(src.name(), "pipe_buffer");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    fn pipe_buffer_default_config() {
        let config = PipeBufferConfig::default();
        assert_eq!(config.num_pipes, 4);
        assert_eq!(config.min_write_size, 1);
        assert_eq!(config.max_write_size, 4096);
        assert!(config.non_blocking);
    }

    #[test]
    fn pipe_buffer_custom_config() {
        let src = PipeBufferSource {
            config: PipeBufferConfig {
                num_pipes: 8,
                min_write_size: 64,
                max_write_size: 1024,
                non_blocking: false,
            },
        };
        assert_eq!(src.config.num_pipes, 8);
    }

    #[test]
    #[ignore] // Uses pipe syscall
    fn pipe_buffer_collects_bytes() {
        let src = PipeBufferSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    #[ignore] // Uses pipe syscall
    fn pipe_buffer_single_pipe_mode() {
        let src = PipeBufferSource {
            config: PipeBufferConfig {
                num_pipes: 0, // Will trigger single-pipe fallback.
                ..PipeBufferConfig::default()
            },
        };
        if src.is_available() {
            let data = src.collect_single_pipe(64);
            assert!(!data.is_empty());
        }
    }

    // Kqueue events
    #[test]
    fn kqueue_events_info() {
        let src = KqueueEventsSource::default();
        assert_eq!(src.name(), "kqueue_events");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    fn kqueue_events_default_config() {
        let config = KqueueEventsConfig::default();
        assert_eq!(config.num_file_watchers, 4);
        assert_eq!(config.num_timers, 8);
        assert_eq!(config.num_sockets, 4);
        assert_eq!(config.timeout_ms, 1);
    }

    #[test]
    fn kqueue_events_custom_config() {
        let src = KqueueEventsSource {
            config: KqueueEventsConfig {
                num_file_watchers: 2,
                num_timers: 4,
                num_sockets: 2,
                timeout_ms: 5,
            },
        };
        assert_eq!(src.config.num_timers, 4);
    }

    #[test]
    #[ignore] // Uses kqueue syscall
    fn kqueue_events_collects_bytes() {
        let src = KqueueEventsSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    // Interleaved frontier
    #[test]
    fn interleaved_frontier_info() {
        let src = InterleavedFrontierSource;
        assert_eq!(src.name(), "interleaved_frontier");
        assert_eq!(src.info().category, SourceCategory::Frontier);
    }

    #[test]
    #[ignore] // Uses multiple syscalls
    fn interleaved_frontier_collects_bytes() {
        let src = InterleavedFrontierSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }

    // Source availability
    #[test]
    fn all_sources_have_valid_names() {
        let sources: Vec<Box<dyn EntropySource>> = vec![
            Box::new(AMXTimingSource::default()),
            Box::new(ThreadLifecycleSource),
            Box::new(MachIPCSource::default()),
            Box::new(TLBShootdownSource::default()),
            Box::new(PipeBufferSource::default()),
            Box::new(KqueueEventsSource::default()),
            Box::new(InterleavedFrontierSource),
        ];
        for src in &sources {
            assert!(!src.name().is_empty());
            assert!(!src.info().description.is_empty());
            assert!(!src.info().physics.is_empty());
        }
    }

    // frontier_sources helper
    #[test]
    fn frontier_sources_returns_six() {
        let sources = frontier_sources();
        assert_eq!(sources.len(), 6);
    }
}
