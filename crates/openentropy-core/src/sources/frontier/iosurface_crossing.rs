//! IOSurface GPU/CPU memory domain crossing — multi-clock-domain coherence entropy.
//!
//! IOSurface is shared memory between GPU and CPU. Writing from one domain and
//! reading from another crosses multiple clock boundaries:
//!   CPU → fabric → GPU memory controller → GPU cache
//!
//! Each domain transition adds independent timing noise from cache coherence
//! traffic, fabric arbitration, and cross-clock-domain synchronization.
//!
//! Uses direct IOSurface framework FFI — no external process spawning.
//! Each create/lock/write/unlock/destroy cycle completes in microseconds.
//!

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
#[cfg(target_os = "macos")]
use crate::sources::helpers::extract_timing_entropy;
#[cfg(target_os = "macos")]
use crate::sources::helpers::mach_time;

static IOSURFACE_CROSSING_INFO: SourceInfo = SourceInfo {
    name: "iosurface_crossing",
    description: "IOSurface GPU/CPU memory domain crossing coherence jitter",
    physics: "Times the round-trip latency of IOSurface create/lock/write/unlock/destroy \
              cycles that cross multiple clock domain boundaries: CPU \u{2192} system fabric \
              \u{2192} GPU memory controller \u{2192} GPU cache \u{2192} back. Each boundary adds \
              independent timing noise from cache coherence protocol arbitration, fabric \
              interconnect scheduling, and cross-clock-domain synchronizer metastability. \
              The combined multi-domain crossing creates high entropy from physically \
              independent noise sources.",
    category: SourceCategory::GPU,
    platform: Platform::MacOS,
    requirements: &[Requirement::IOSurface],
    entropy_rate_estimate: 3000.0,
    composite: false,
};

/// Entropy source from GPU/CPU memory domain crossing timing.
pub struct IOSurfaceCrossingSource;

/// IOSurface framework FFI (macOS only).
#[cfg(target_os = "macos")]
mod iosurface {
    use std::ffi::c_void;

    // CFDictionary/CFNumber/CFString types (all opaque pointers).
    type CFDictionaryRef = *const c_void;
    type CFMutableDictionaryRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CFNumberRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    type IOSurfaceRef = *mut c_void;

    // IOSurface lock options.
    const K_IOSURFACE_LOCK_READ_ONLY: u32 = 1;

    #[link(name = "IOSurface", kind = "framework")]
    unsafe extern "C" {
        fn IOSurfaceCreate(properties: CFDictionaryRef) -> IOSurfaceRef;
        fn IOSurfaceLock(surface: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
        fn IOSurfaceUnlock(surface: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
        fn IOSurfaceGetBaseAddress(surface: IOSurfaceRef) -> *mut c_void;
        fn IOSurfaceGetAllocSize(surface: IOSurfaceRef) -> usize;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFAllocatorDefault: CFAllocatorRef;

        fn CFDictionaryCreateMutable(
            allocator: CFAllocatorRef,
            capacity: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFMutableDictionaryRef;

        fn CFDictionarySetValue(
            dict: CFMutableDictionaryRef,
            key: *const c_void,
            value: *const c_void,
        );

        fn CFNumberCreate(
            allocator: CFAllocatorRef,
            the_type: CFIndex,
            value_ptr: *const c_void,
        ) -> CFNumberRef;

        fn CFRelease(cf: CFTypeRef);

        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
    }

    // IOSurface property keys — linked from the IOSurface framework.
    #[link(name = "IOSurface", kind = "framework")]
    unsafe extern "C" {
        static kIOSurfaceWidth: CFStringRef;
        static kIOSurfaceHeight: CFStringRef;
        static kIOSurfaceBytesPerElement: CFStringRef;
        static kIOSurfaceBytesPerRow: CFStringRef;
        static kIOSurfaceAllocSize: CFStringRef;
        static kIOSurfacePixelFormat: CFStringRef;
    }

    // kCFNumberSInt32Type = 3
    const K_CF_NUMBER_SINT32_TYPE: CFIndex = 3;

    /// Perform one IOSurface create/lock/write/read/unlock/destroy cycle.
    /// Returns the high-resolution timing of the cycle, or None on failure.
    pub fn crossing_cycle(iteration: usize) -> Option<u64> {
        unsafe {
            // Build IOSurface properties dictionary.
            let dict = CFDictionaryCreateMutable(
                kCFAllocatorDefault,
                6,
                std::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),
                std::ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast(),
            );
            if dict.is_null() {
                return None;
            }

            let width: i32 = 64;
            let height: i32 = 64;
            let bpe: i32 = 4;
            let bpr: i32 = width * bpe;
            let alloc_size: i32 = bpr * height;
            let pixel_format: i32 = 0x42475241; // 'BGRA'

            set_dict_int(dict, kIOSurfaceWidth, width);
            set_dict_int(dict, kIOSurfaceHeight, height);
            set_dict_int(dict, kIOSurfaceBytesPerElement, bpe);
            set_dict_int(dict, kIOSurfaceBytesPerRow, bpr);
            set_dict_int(dict, kIOSurfaceAllocSize, alloc_size);
            set_dict_int(dict, kIOSurfacePixelFormat, pixel_format);

            // Create IOSurface (crosses into kernel / GPU memory controller).
            let surface = IOSurfaceCreate(dict as CFDictionaryRef);
            CFRelease(dict as CFTypeRef);
            if surface.is_null() {
                return None;
            }

            // Capture high-resolution timestamp around the lock/write/unlock cycle.
            // This crosses CPU→fabric→GPU memory controller clock domains.
            let t0 = super::mach_time();

            // Lock for write (crosses clock domains).
            let lock_result = IOSurfaceLock(surface, 0, std::ptr::null_mut());
            if lock_result != 0 {
                CFRelease(surface as CFTypeRef);
                return None;
            }

            // Write pattern to surface memory (CPU domain write).
            let base = IOSurfaceGetBaseAddress(surface);
            if !base.is_null() {
                let size = IOSurfaceGetAllocSize(surface);
                let slice = std::slice::from_raw_parts_mut(base as *mut u8, size);
                // Write a pattern that varies per iteration to prevent optimization.
                let pattern = (iteration as u8).wrapping_mul(0x37).wrapping_add(0xA5);
                for (j, byte) in slice.iter_mut().enumerate() {
                    *byte = pattern.wrapping_add(j as u8);
                }
                std::hint::black_box(&slice[0]);
            }

            // Unlock write (flushes CPU caches, crosses back).
            IOSurfaceUnlock(surface, 0, std::ptr::null_mut());

            let t1 = super::mach_time();

            // Lock for read (cross-domain coherence).
            IOSurfaceLock(surface, K_IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());

            // Read back (may hit different cache/memory path).
            if !base.is_null() {
                let size = IOSurfaceGetAllocSize(surface);
                let slice = std::slice::from_raw_parts(base as *const u8, size);
                std::hint::black_box(slice[iteration % size]);
            }

            IOSurfaceUnlock(surface, K_IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());

            let t2 = super::mach_time();

            // Destroy (returns memory to GPU memory controller pool).
            CFRelease(surface as CFTypeRef);

            // Combine write-cycle and read-cycle timings for maximum jitter capture.
            let write_timing = t1.wrapping_sub(t0);
            let read_timing = t2.wrapping_sub(t1);
            Some(write_timing ^ read_timing.rotate_left(32))
        }
    }

    /// Helper: set an integer value in a CFMutableDictionary.
    unsafe fn set_dict_int(dict: CFMutableDictionaryRef, key: CFStringRef, value: i32) {
        let num = unsafe {
            CFNumberCreate(
                kCFAllocatorDefault,
                K_CF_NUMBER_SINT32_TYPE,
                &value as *const i32 as *const c_void,
            )
        };
        if !num.is_null() {
            unsafe {
                CFDictionarySetValue(dict, key, num);
                CFRelease(num as CFTypeRef);
            }
        }
    }

    /// Check if IOSurface is available by trying to create one.
    pub fn is_available() -> bool {
        crossing_cycle(0).is_some()
    }
}

impl EntropySource for IOSurfaceCrossingSource {
    fn info(&self) -> &SourceInfo {
        &IOSURFACE_CROSSING_INFO
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            iosurface::is_available()
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
            Vec::new()
        }

        #[cfg(target_os = "macos")]
        {
            let raw_count = n_samples * 4 + 64;
            let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

            for i in 0..raw_count {
                // IOSurface cycle crosses CPU→fabric→GPU memory controller domains.
                // The returned value captures write/read timing jitter directly.
                if let Some(cycle_timing) = iosurface::crossing_cycle(i) {
                    timings.push(cycle_timing);
                }
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
        let src = IOSurfaceCrossingSource;
        assert_eq!(src.name(), "iosurface_crossing");
        assert_eq!(src.info().category, SourceCategory::GPU);
        assert!(!src.info().composite);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore] // Requires IOSurface framework
    fn collects_bytes() {
        let src = IOSurfaceCrossingSource;
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
            let unique: std::collections::HashSet<u8> = data.iter().copied().collect();
            assert!(unique.len() > 1, "Expected variation in collected bytes");
        }
    }
}
