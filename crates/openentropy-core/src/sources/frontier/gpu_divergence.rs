//! GPU shader thread divergence — intra-warp nondeterminism entropy.
//!
//! GPU threads (SIMD groups) should execute in lockstep but don't due to:
//! - Warp divergence from conditional branches
//! - Memory coalescing failures
//! - Thermal effects on GPU clock frequency
//! - L2 cache bank conflicts
//!
//! We dispatch a Metal compute shader where threads race to atomically
//! increment a counter. The execution order captures GPU scheduling
//! nondeterminism that is genuinely novel as an entropy source.
//!
//! Uses direct Metal framework FFI via Objective-C runtime — no external
//! process spawning. Each dispatch completes in microseconds.
//!


use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time, xor_fold_u64};

static GPU_DIVERGENCE_INFO: SourceInfo = SourceInfo {
    name: "gpu_divergence",
    description: "GPU shader thread execution order divergence entropy",
    physics: "Dispatches Metal compute shaders where parallel threads race to atomically \
              increment a shared counter. The execution order captures GPU scheduling \
              nondeterminism from: SIMD group divergence on conditional branches, memory \
              coalescing failures, L2 cache bank conflicts, thermal-dependent GPU clock \
              frequency variation, and warp scheduler arbitration. Each dispatch produces \
              a different execution ordering due to physical nondeterminism in the GPU.",
    category: SourceCategory::GPU,
    platform: Platform::MacOS,
    requirements: &[Requirement::Metal],
    entropy_rate_estimate: 6000.0,
    composite: false,
};

/// Entropy source that harvests thread execution order divergence from Metal GPU.
pub struct GPUDivergenceSource;

/// Metal framework FFI via Objective-C runtime (macOS only).
#[cfg(target_os = "macos")]
mod metal {
    use std::ffi::{CString, c_void};

    // Objective-C runtime types.
    type Id = *mut c_void;
    type Sel = *mut c_void;
    type Class = *mut c_void;

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const i8) -> Class;
        fn sel_registerName(name: *const i8) -> Sel;
        fn objc_msgSend(receiver: Id, sel: Sel, ...) -> Id;
    }

    // Metal framework link — ensures the framework is loaded.
    #[link(name = "Metal", kind = "framework")]
    unsafe extern "C" {
        fn MTLCreateSystemDefaultDevice() -> Id;
    }

    /// Number of GPU threads per dispatch.
    const THREADS: u32 = 256;

    /// Metal shader source: threads race to atomically increment a counter.
    /// The `order` output captures the nondeterministic execution ordering.
    const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;
kernel void divergence(
    device atomic_uint *counter [[buffer(0)]],
    device uint *output [[buffer(1)]],
    uint tid [[thread_position_in_grid]]
) {
    // Data-dependent work to create divergence.
    uint val = tid;
    for (uint i = 0; i < 16; i++) {
        if (val & 1) { val = val * 3 + 1; }
        else { val = val >> 1; }
    }
    // Atomic increment — order captures scheduling nondeterminism.
    uint order = atomic_fetch_add_explicit(counter, 1, memory_order_relaxed);
    output[tid] = order ^ val;
}
"#;

    /// Opaque handle to a reusable Metal pipeline + buffers.
    pub struct MetalState {
        _device: Id,
        queue: Id,
        pipeline: Id,
        counter_buf: Id,
        output_buf: Id,
    }

    // SAFETY: Metal objects are reference-counted and thread-safe.
    // We only use them from a single thread within `collect()`.
    unsafe impl Send for MetalState {}

    /// Cast `objc_msgSend` to a concrete function pointer type.
    ///
    /// We must go through a raw pointer because `objc_msgSend` is a variadic
    /// extern fn which is a zero-sized type that cannot be transmuted directly.
    macro_rules! msg_send_fn {
        ($ty:ty) => {
            std::mem::transmute::<*const (), $ty>(objc_msgSend as *const ())
        };
    }

    impl MetalState {
        /// Try to initialize Metal device, compile shader, create buffers.
        pub fn new() -> Option<Self> {
            unsafe {
                // SAFETY: MTLCreateSystemDefaultDevice returns a retained Metal device
                // object or null if no GPU is available.
                let device = MTLCreateSystemDefaultDevice();
                if device.is_null() {
                    return None;
                }

                let queue = msg_send(device, "newCommandQueue");
                if queue.is_null() {
                    return None;
                }

                let pipeline = compile_shader(device)?;

                let counter_buf = new_buffer(device, 4); // 1 x uint32
                let output_buf = new_buffer(device, THREADS as u64 * 4); // THREADS x uint32
                if counter_buf.is_null() || output_buf.is_null() {
                    return None;
                }

                Some(MetalState {
                    _device: device,
                    queue,
                    pipeline,
                    counter_buf,
                    output_buf,
                })
            }
        }

        /// Dispatch one compute pass and return the output buffer contents.
        pub fn dispatch(&self) -> Option<Vec<u32>> {
            unsafe {
                // Zero the counter.
                // SAFETY: counter_buf is a shared MTLBuffer we created. `contents` returns
                // a valid pointer to the buffer's CPU-accessible memory.
                let counter_ptr = msg_send(self.counter_buf, "contents") as *mut u32;
                if counter_ptr.is_null() {
                    return None;
                }
                *counter_ptr = 0;

                let cmd_buf = msg_send(self.queue, "commandBuffer");
                if cmd_buf.is_null() {
                    return None;
                }

                let encoder = msg_send(cmd_buf, "computeCommandEncoder");
                if encoder.is_null() {
                    return None;
                }

                // encoder.setComputePipelineState_(pipeline)
                let sel_set_pipeline = sel("setComputePipelineState:");
                msg_send_fn!(unsafe extern "C" fn(Id, Sel, Id))(
                    encoder,
                    sel_set_pipeline,
                    self.pipeline,
                );

                // encoder.setBuffer_offset_atIndex_(counter_buf, 0, 0)
                set_buffer(encoder, self.counter_buf, 0, 0);
                // encoder.setBuffer_offset_atIndex_(output_buf, 0, 1)
                set_buffer(encoder, self.output_buf, 0, 1);

                dispatch_threads_1d(encoder, THREADS, THREADS.min(256));

                // End encoding, commit, wait.
                msg_send_fn!(unsafe extern "C" fn(Id, Sel))(encoder, sel("endEncoding"));
                msg_send_fn!(unsafe extern "C" fn(Id, Sel))(cmd_buf, sel("commit"));
                msg_send_fn!(unsafe extern "C" fn(Id, Sel))(cmd_buf, sel("waitUntilCompleted"));

                // Read output.
                let output_ptr = msg_send(self.output_buf, "contents") as *const u32;
                if output_ptr.is_null() {
                    return None;
                }
                let mut result = vec![0u32; THREADS as usize];
                std::ptr::copy_nonoverlapping(output_ptr, result.as_mut_ptr(), THREADS as usize);
                Some(result)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Objective-C runtime helpers
    // -----------------------------------------------------------------------

    unsafe fn sel(name: &str) -> Sel {
        let c_name = CString::new(name).expect("selector contains null byte");
        unsafe { sel_registerName(c_name.as_ptr()) }
    }

    unsafe fn msg_send(obj: Id, sel_name: &str) -> Id {
        unsafe {
            let s = sel(sel_name);
            msg_send_fn!(unsafe extern "C" fn(Id, Sel) -> Id)(obj, s)
        }
    }

    /// Create an NSString from a Rust &str.
    unsafe fn nsstring(s: &str) -> Id {
        unsafe {
            let cls = objc_getClass(c"NSString".as_ptr());
            let sel_alloc = sel("alloc");
            let sel_init = sel("initWithBytes:length:encoding:");

            let raw = msg_send_fn!(unsafe extern "C" fn(Id, Sel) -> Id)(cls as Id, sel_alloc);
            // NSUTF8StringEncoding = 4
            msg_send_fn!(unsafe extern "C" fn(Id, Sel, *const u8, u64, u64) -> Id)(
                raw,
                sel_init,
                s.as_ptr(),
                s.len() as u64,
                4,
            )
        }
    }

    /// Compile the Metal shader source and return a compute pipeline state.
    unsafe fn compile_shader(device: Id) -> Option<Id> {
        unsafe {
            let source = nsstring(SHADER_SOURCE);
            if source.is_null() {
                return None;
            }

            // device.newLibraryWithSource:options:error:
            let sel_lib = sel("newLibraryWithSource:options:error:");
            let mut error: Id = std::ptr::null_mut();
            let library = msg_send_fn!(unsafe extern "C" fn(Id, Sel, Id, Id, *mut Id) -> Id)(
                device,
                sel_lib,
                source,
                std::ptr::null_mut(), // default options
                &mut error,
            );
            if library.is_null() {
                return None;
            }

            // library.newFunctionWithName:("divergence")
            let func_name = nsstring("divergence");
            let sel_func = sel("newFunctionWithName:");
            let function =
                msg_send_fn!(unsafe extern "C" fn(Id, Sel, Id) -> Id)(library, sel_func, func_name);
            if function.is_null() {
                return None;
            }

            // device.newComputePipelineStateWithFunction:error:
            let sel_pipe = sel("newComputePipelineStateWithFunction:error:");
            let mut error2: Id = std::ptr::null_mut();
            let pipeline = msg_send_fn!(unsafe extern "C" fn(Id, Sel, Id, *mut Id) -> Id)(
                device,
                sel_pipe,
                function,
                &mut error2,
            );
            if pipeline.is_null() {
                return None;
            }

            Some(pipeline)
        }
    }

    /// Create a shared MTLBuffer of given size.
    unsafe fn new_buffer(device: Id, size: u64) -> Id {
        unsafe {
            let sel_buf = sel("newBufferWithLength:options:");
            // MTLResourceStorageModeShared = 0
            msg_send_fn!(unsafe extern "C" fn(Id, Sel, u64, u64) -> Id)(device, sel_buf, size, 0)
        }
    }

    /// Set a buffer on a compute command encoder.
    unsafe fn set_buffer(encoder: Id, buffer: Id, offset: u64, index: u64) {
        unsafe {
            let s = sel("setBuffer:offset:atIndex:");
            msg_send_fn!(unsafe extern "C" fn(Id, Sel, Id, u64, u64))(
                encoder, s, buffer, offset, index,
            );
        }
    }

    /// Dispatch 1D threads on a compute command encoder.
    unsafe fn dispatch_threads_1d(encoder: Id, total: u32, per_group: u32) {
        // MTLSize is a struct of 3 x NSUInteger (u64 on 64-bit).
        #[repr(C)]
        struct MTLSize {
            width: u64,
            height: u64,
            depth: u64,
        }

        let grid = MTLSize {
            width: total as u64,
            height: 1,
            depth: 1,
        };
        let group = MTLSize {
            width: per_group as u64,
            height: 1,
            depth: 1,
        };

        unsafe {
            let s = sel("dispatchThreads:threadsPerThreadgroup:");
            msg_send_fn!(unsafe extern "C" fn(Id, Sel, MTLSize, MTLSize))(encoder, s, grid, group);
        }
    }
}

impl EntropySource for GPUDivergenceSource {
    fn info(&self) -> &SourceInfo {
        &GPU_DIVERGENCE_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            metal::MetalState::new().is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = n_samples;
            return Vec::new();
        }

        #[cfg(target_os = "macos")]
        {
            let state = match metal::MetalState::new() {
                Some(s) => s,
                None => return Vec::new(),
            };

            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
            let mut gpu_entropy: Vec<u8> = Vec::with_capacity(raw_count);

            for _ in 0..raw_count {
                let t0 = mach_time();

                // GPU dispatch crosses CPU→GPU→CPU clock domains.
                let results = match state.dispatch() {
                    Some(r) => r,
                    None => continue,
                };

                let t1 = mach_time();
                timings.push(t1.wrapping_sub(t0));

                // XOR-fold all thread execution orders into one byte.
                // This captures GPU scheduling nondeterminism directly.
                let mut gpu_hash: u64 = 0;
                for (i, &val) in results.iter().enumerate() {
                    gpu_hash ^= (val as u64).rotate_left((i as u32) & 63);
                }
                gpu_entropy.push(xor_fold_u64(gpu_hash));
            }

            // Extract timing entropy from dispatch latencies.
            let timing_bytes = extract_timing_entropy(&timings, n_samples);

            // XOR GPU execution order entropy with dispatch timing entropy.
            // Both are genuine, independent entropy sources.
            let mut output: Vec<u8> = Vec::with_capacity(n_samples);
            for i in 0..n_samples.min(timing_bytes.len()).min(gpu_entropy.len()) {
                output.push(timing_bytes[i] ^ gpu_entropy[i]);
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
        let src = GPUDivergenceSource;
        assert_eq!(src.name(), "gpu_divergence");
        assert_eq!(src.info().category, SourceCategory::GPU);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires GPU
    fn collects_bytes() {
        let src = GPUDivergenceSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
            let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
            assert!(unique.len() > 1, "Expected variation in collected bytes");
        }
    }
}
