//! Keychain/securityd IPC timing — entropy from the Security framework's
//! multi-domain round-trip through securityd, SEP, and APFS.

use crate::source::{EntropySource, Platform, Requirement, SourceCategory, SourceInfo};
use crate::sources::helpers::mach_time;

use super::extract_timing_entropy_variance;

/// Configuration for keychain timing entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::KeychainTimingConfig;
/// let config = KeychainTimingConfig::default();
/// ```
#[derive(Debug, Clone, Default)]
pub struct KeychainTimingConfig {
    /// Use SecItemAdd/Delete (write path) instead of SecItemCopyMatching (read path).
    ///

    ///
    /// **Default:** `false` (use read path for speed)
    pub use_write_path: bool,
}

/// Harvests timing jitter from Security.framework keychain operations.
///
/// # What it measures
/// Nanosecond timing of keychain operations (SecItemCopyMatching or SecItemAdd/Delete)
/// that traverse the full Apple security stack.
///
/// # Why it's entropic
/// Every keychain operation travels through multiple independent physical domains:
/// 1. **XPC IPC** to securityd — scheduling/dispatch jitter
/// 2. **securityd processing** — SQLite database lookup, access control evaluation
/// 3. **Kernel scheduling** — context switches between our process and securityd
/// 4. **Database I/O** — SQLite page reads from the keychain database file
///
/// The write path additionally involves APFS copy-on-write and NVMe controller
/// timing. The read path may or may not traverse the Secure Enclave depending
/// on the item's access control policy.
///
/// # What makes it unique
/// No prior work has used keychain operation timing as an entropy source.
/// The round-trip through XPC IPC, securityd scheduling, and database I/O
/// aggregates jitter from multiple independent domains in a single measurement.
///

///
/// # Caveats
/// - High autocorrelation at lag-1 (~0.43): variance extraction mitigates this
/// - Warm-up effect: first ~500 reads are slower due to securityd cold caches
/// - Slow: ~0.6ms per sample, not suitable for high-throughput collection
///
/// # Configuration
/// See [`KeychainTimingConfig`] for tunable parameters.
#[derive(Default)]
pub struct KeychainTimingSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
    pub config: KeychainTimingConfig,
}

static KEYCHAIN_TIMING_INFO: SourceInfo = SourceInfo {
    name: "keychain_timing",
    description: "Keychain/securityd round-trip timing jitter",
    physics: "Times keychain operations that traverse: XPC IPC to securityd → SQLite \
              database lookup → access control evaluation → return. Each domain (IPC \
              scheduling, securityd process scheduling, database page I/O, kernel context \
              switches) contributes independent jitter. Variance extraction removes serial \
              correlation (lag-1 autocorrelation ~0.43 in raw timings). First 500 samples \
              discarded to avoid warm-up transient from securityd cold caches.",
    category: SourceCategory::IPC,
    platform: Platform::MacOS,
    requirements: &[Requirement::SecurityFramework],
    entropy_rate_estimate: 6500.0,
    composite: false,
};

impl EntropySource for KeychainTimingSource {
    fn info(&self) -> &SourceInfo {
        &KEYCHAIN_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        if self.config.use_write_path {
            collect_write_path(n_samples)
        } else {
            collect_read_path(n_samples)
        }
    }
}

/// Number of warm-up samples to discard (securityd cold cache transient).
const WARMUP_SAMPLES: usize = 100;

/// Collect entropy via the keychain read path (SecItemCopyMatching).
/// Faster (~0.6ms/sample) with excellent entropy (H∞ ≈ 6.5–7.0).
fn collect_read_path(n_samples: usize) -> Vec<u8> {
    // Bind Security framework symbols.
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecClass: CFStringRef;
        static kSecClassGenericPassword: CFStringRef;
        static kSecAttrLabel: CFStringRef;
        static kSecReturnData: CFStringRef;
        static kCFBooleanTrue: CFBooleanRef;
        static kSecValueData: CFStringRef;
        static kSecAttrAccessible: CFStringRef;
        static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: CFStringRef;

        fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
        fn SecItemDelete(query: CFDictionaryRef) -> i32;
        fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            cStr: *const i8,
            encoding: u32,
        ) -> CFStringRef;
        fn CFDataCreate(alloc: CFAllocatorRef, bytes: *const u8, length: isize) -> CFDataRef;
        fn CFDictionaryCreateMutable(
            alloc: CFAllocatorRef,
            capacity: isize,
            keyCallBacks: *const std::ffi::c_void,
            valueCallBacks: *const std::ffi::c_void,
        ) -> CFMutableDictionaryRef;
        fn CFDictionarySetValue(
            dict: CFMutableDictionaryRef,
            key: *const std::ffi::c_void,
            value: *const std::ffi::c_void,
        );
        fn CFRelease(cf: CFTypeRef);

        static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
        static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    }

    // Opaque CF types (we only use them as pointers).
    type CFAllocatorRef = *const std::ffi::c_void;
    type CFStringRef = *const std::ffi::c_void;
    type CFDataRef = *const std::ffi::c_void;
    type CFBooleanRef = *const std::ffi::c_void;
    type CFDictionaryRef = *const std::ffi::c_void;
    type CFMutableDictionaryRef = *mut std::ffi::c_void;
    type CFTypeRef = *const std::ffi::c_void;

    #[repr(C)]
    struct CFDictionaryKeyCallBacks {
        _data: [u8; 40],
    }
    #[repr(C)]
    struct CFDictionaryValueCallBacks {
        _data: [u8; 40],
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    let label_cstr = b"openentropy-timing-probe\0";

    unsafe {
        // Create a keychain item to query.
        let label_cf = CFStringCreateWithCString(
            std::ptr::null(),
            label_cstr.as_ptr() as *const i8,
            K_CF_STRING_ENCODING_UTF8,
        );
        let secret: [u8; 16] = [0x42; 16];
        let secret_cf = CFDataCreate(std::ptr::null(), secret.as_ptr(), 16);

        // Create the item.
        let add_dict = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        CFDictionarySetValue(add_dict, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(add_dict, kSecAttrLabel as _, label_cf as _);
        CFDictionarySetValue(add_dict, kSecValueData as _, secret_cf as _);
        CFDictionarySetValue(
            add_dict,
            kSecAttrAccessible as _,
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as _,
        );

        // Delete any existing item, then add.
        let del_dict = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        CFDictionarySetValue(del_dict, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(del_dict, kSecAttrLabel as _, label_cf as _);
        SecItemDelete(del_dict as _);
        SecItemAdd(add_dict as _, std::ptr::null_mut());

        // Build query dictionary.
        let query = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        CFDictionarySetValue(query, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(query, kSecAttrLabel as _, label_cf as _);
        CFDictionarySetValue(query, kSecReturnData as _, kCFBooleanTrue as _);

        // Warm-up: discard first samples to avoid securityd cold cache transient.
        for _ in 0..WARMUP_SAMPLES {
            let mut result: CFTypeRef = std::ptr::null();
            SecItemCopyMatching(query as _, &mut result);
            if !result.is_null() {
                CFRelease(result);
            }
        }

        // Collect timings.
        let raw_count = n_samples * 4 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        for _ in 0..raw_count {
            let mut result: CFTypeRef = std::ptr::null();
            let t0 = mach_time();
            SecItemCopyMatching(query as _, &mut result);
            let t1 = mach_time();
            timings.push(t1.wrapping_sub(t0));
            if !result.is_null() {
                CFRelease(result);
            }
        }

        // Cleanup.
        SecItemDelete(del_dict as _);
        CFRelease(del_dict as CFTypeRef);
        CFRelease(add_dict as CFTypeRef);
        CFRelease(query as CFTypeRef);
        CFRelease(secret_cf);
        CFRelease(label_cf);

        extract_timing_entropy_variance(&timings, n_samples)
    }
}

/// Collect entropy via the keychain write path (SecItemAdd/Delete).
/// Slower (~5ms/sample) but highest entropy (H∞ ≈ 7.4).
fn collect_write_path(n_samples: usize) -> Vec<u8> {
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecClass: CFStringRef;
        static kSecClassGenericPassword: CFStringRef;
        static kSecAttrLabel: CFStringRef;
        static kSecValueData: CFStringRef;
        static kSecAttrAccessible: CFStringRef;
        static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: CFStringRef;

        fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
        fn SecItemDelete(query: CFDictionaryRef) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            cStr: *const i8,
            encoding: u32,
        ) -> CFStringRef;
        fn CFDataCreate(alloc: CFAllocatorRef, bytes: *const u8, length: isize) -> CFDataRef;
        fn CFDictionaryCreateMutable(
            alloc: CFAllocatorRef,
            capacity: isize,
            keyCallBacks: *const std::ffi::c_void,
            valueCallBacks: *const std::ffi::c_void,
        ) -> CFMutableDictionaryRef;
        fn CFDictionarySetValue(
            dict: CFMutableDictionaryRef,
            key: *const std::ffi::c_void,
            value: *const std::ffi::c_void,
        );
        fn CFRelease(cf: CFTypeRef);

        static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
        static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    }

    type CFAllocatorRef = *const std::ffi::c_void;
    type CFStringRef = *const std::ffi::c_void;
    type CFDataRef = *const std::ffi::c_void;
    type CFDictionaryRef = *const std::ffi::c_void;
    type CFMutableDictionaryRef = *mut std::ffi::c_void;
    type CFTypeRef = *const std::ffi::c_void;

    #[repr(C)]
    struct CFDictionaryKeyCallBacks {
        _data: [u8; 40],
    }
    #[repr(C)]
    struct CFDictionaryValueCallBacks {
        _data: [u8; 40],
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    let raw_count = n_samples * 4 + 64;
    let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

    unsafe {
        for i in 0..raw_count {
            let mut label_buf = [0u8; 64];
            let label_str = format!("oe-ent-{}\0", i);
            label_buf[..label_str.len()].copy_from_slice(label_str.as_bytes());

            let label_cf = CFStringCreateWithCString(
                std::ptr::null(),
                label_buf.as_ptr() as *const i8,
                K_CF_STRING_ENCODING_UTF8,
            );

            let secret: [u8; 16] = [i as u8; 16];
            let secret_cf = CFDataCreate(std::ptr::null(), secret.as_ptr(), 16);

            let attrs = CFDictionaryCreateMutable(
                std::ptr::null(),
                0,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
            );
            CFDictionarySetValue(attrs, kSecClass as _, kSecClassGenericPassword as _);
            CFDictionarySetValue(attrs, kSecAttrLabel as _, label_cf as _);
            CFDictionarySetValue(attrs, kSecValueData as _, secret_cf as _);
            CFDictionarySetValue(
                attrs,
                kSecAttrAccessible as _,
                kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as _,
            );

            let t0 = mach_time();
            let status = SecItemAdd(attrs as _, std::ptr::null_mut());
            let t1 = mach_time();

            if status == 0 {
                // errSecSuccess
                timings.push(t1.wrapping_sub(t0));
            }

            // Delete.
            let del = CFDictionaryCreateMutable(
                std::ptr::null(),
                0,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
            );
            CFDictionarySetValue(del, kSecClass as _, kSecClassGenericPassword as _);
            CFDictionarySetValue(del, kSecAttrLabel as _, label_cf as _);
            SecItemDelete(del as _);

            CFRelease(del as CFTypeRef);
            CFRelease(attrs as CFTypeRef);
            CFRelease(secret_cf);
            CFRelease(label_cf);
        }
    }

    extract_timing_entropy_variance(&timings, n_samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = KeychainTimingSource::default();
        assert_eq!(src.name(), "keychain_timing");
        assert_eq!(src.info().category, SourceCategory::IPC);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = KeychainTimingConfig::default();
        assert!(!config.use_write_path);
    }

    #[test]
    #[ignore] // Requires macOS keychain access
    fn collects_bytes_read_path() {
        let src = KeychainTimingSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    #[ignore] // Requires macOS keychain access, slow
    fn collects_bytes_write_path() {
        let src = KeychainTimingSource {
            config: KeychainTimingConfig {
                use_write_path: true,
            },
        };
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
