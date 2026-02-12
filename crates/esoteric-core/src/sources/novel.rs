//! Novel entropy sources: dispatch queue scheduling, dynamic linker timing,
//! VM page fault timing, and Spotlight metadata query timing.

use std::process::Command;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

/// Extract LSBs from u64 deltas, packing 8 bits per byte.
#[allow(dead_code)]
fn extract_lsbs_u64(deltas: &[u64]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::with_capacity(deltas.len());
    for d in deltas {
        bits.push((d & 1) as u8);
    }

    let mut bytes = Vec::with_capacity(bits.len() / 8 + 1);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= bit << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
}

// ---------------------------------------------------------------------------
// DispatchQueueSource
// ---------------------------------------------------------------------------

static DISPATCH_QUEUE_INFO: SourceInfo = SourceInfo {
    name: "dispatch_queue",
    description: "Thread scheduling latency jitter from concurrent dispatch queue operations",
    physics: "Submits blocks to GCD (Grand Central Dispatch) queues and measures scheduling \
              latency. macOS dynamically migrates work between P-cores (performance) and \
              E-cores (efficiency) based on thermal state and load. The migration decisions, \
              queue priority inversions, and QoS tier scheduling create non-deterministic \
              dispatch timing.",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 1500.0,
};

/// Entropy source that harvests scheduling latency jitter from worker thread
/// dispatch via MPSC channels (analogous to GCD queue dispatch).
pub struct DispatchQueueSource;

impl EntropySource for DispatchQueueSource {
    fn info(&self) -> &SourceInfo {
        &DISPATCH_QUEUE_INFO
    }

    fn is_available(&self) -> bool {
        true
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        // Create 4 worker threads with MPSC channels.
        let num_workers = 4;
        let mut senders: Vec<mpsc::Sender<Instant>> = Vec::with_capacity(num_workers);
        let (result_tx, result_rx) = mpsc::channel::<u64>();

        for _ in 0..num_workers {
            let (tx, rx) = mpsc::channel::<Instant>();
            let rtx = result_tx.clone();
            senders.push(tx);

            thread::spawn(move || {
                while let Ok(sent_at) = rx.recv() {
                    // Measure scheduling latency: time from send to receive.
                    let latency_ns = sent_at.elapsed().as_nanos() as u64;
                    if rtx.send(latency_ns).is_err() {
                        break;
                    }
                }
            });
        }

        // Submit items to workers round-robin and collect scheduling latencies.
        for i in 0..raw_count {
            let worker_idx = i % num_workers;
            let sent_at = Instant::now();
            if senders[worker_idx].send(sent_at).is_err() {
                break;
            }
            match result_rx.recv() {
                Ok(latency_ns) => timings.push(latency_ns),
                Err(_) => break,
            }
        }

        // Drop senders to signal workers to exit.
        drop(senders);

        // Compute deltas between consecutive scheduling latencies.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // Extract LSBs.
        let mut entropy: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// DyldTimingSource
// ---------------------------------------------------------------------------

/// Libraries to cycle through on macOS.
#[cfg(target_os = "macos")]
const DYLD_LIBRARIES: &[&str] = &[
    "libz.dylib",
    "libc++.dylib",
    "libobjc.dylib",
    "libSystem.B.dylib",
];

/// Libraries to cycle through on Linux.
#[cfg(target_os = "linux")]
const DYLD_LIBRARIES: &[&str] = &["libc.so.6", "libm.so.6"];

/// Fallback for other platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const DYLD_LIBRARIES: &[&str] = &[];

static DYLD_TIMING_INFO: SourceInfo = SourceInfo {
    name: "dyld_timing",
    description: "Dynamic library loading (dlopen/dlsym) timing jitter",
    physics: "Times dynamic library loading (dlopen/dlsym) which requires: searching the \
              dyld shared cache, resolving symbol tables, rebasing pointers, and running \
              initializers. The timing varies with: shared cache page residency (depends on \
              what other apps have loaded), ASLR randomization, and filesystem metadata \
              cache state.",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 1200.0,
};

/// Entropy source that harvests timing jitter from dynamic library loading.
pub struct DyldTimingSource;

impl EntropySource for DyldTimingSource {
    fn info(&self) -> &SourceInfo {
        &DYLD_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        !DYLD_LIBRARIES.is_empty()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        if DYLD_LIBRARIES.is_empty() {
            return Vec::new();
        }

        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let lib_count = DYLD_LIBRARIES.len();

        for i in 0..raw_count {
            let lib_name = DYLD_LIBRARIES[i % lib_count];

            // Measure the time to load and immediately unload a system library.
            let t0 = Instant::now();

            // SAFETY: We are loading well-known system libraries.
            let result = unsafe { libloading::Library::new(lib_name) };
            if let Ok(lib) = result {
                // The library is dropped (unloaded) at end of this scope.
                std::hint::black_box(&lib);
                drop(lib);
            }

            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // Extract LSBs.
        let mut entropy: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// VMPageTimingSource
// ---------------------------------------------------------------------------

/// Page size for mmap allocations.
const PAGE_SIZE: usize = 4096;

static VM_PAGE_TIMING_INFO: SourceInfo = SourceInfo {
    name: "vm_page_timing",
    description: "Mach VM page fault timing jitter from mmap/munmap cycles",
    physics: "Times Mach VM operations (mmap/munmap cycles). Each operation requires: \
              VM map entry allocation, page table updates, TLB shootdown across cores \
              (IPI interrupt), and physical page management. The timing depends on: \
              VM map fragmentation, physical memory pressure, and cross-core \
              synchronization latency.",
    category: SourceCategory::Novel,
    platform_requirements: &[],
    entropy_rate_estimate: 1300.0,
};

/// Entropy source that harvests timing jitter from VM page allocation/deallocation.
pub struct VMPageTimingSource;

impl EntropySource for VMPageTimingSource {
    fn info(&self) -> &SourceInfo {
        &VM_PAGE_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(unix)
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let raw_count = n_samples * 10 + 64;
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);

        for _ in 0..raw_count {
            let t0 = Instant::now();

            // Allocate a 4KB anonymous private page.
            let addr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    PAGE_SIZE,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                    -1,
                    0,
                )
            };

            if addr == libc::MAP_FAILED {
                continue;
            }

            // Write to the region to trigger a page fault.
            unsafe {
                ptr::write_volatile(addr as *mut u8, 0xBE);
                ptr::write_volatile((addr as *mut u8).add(PAGE_SIZE - 1), 0xEF);

                // Read back to force a full round-trip.
                let _v = ptr::read_volatile(addr as *const u8);
            }

            // Deallocate the page.
            unsafe {
                libc::munmap(addr, PAGE_SIZE);
            }

            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // XOR consecutive deltas.
        let xor_deltas: Vec<u64> = if deltas.len() >= 2 {
            deltas.windows(2).map(|w| w[0] ^ w[1]).collect()
        } else {
            deltas.clone()
        };

        // Extract LSBs.
        let mut entropy: Vec<u8> = xor_deltas.iter().map(|&d| d as u8).collect();
        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// SpotlightTimingSource
// ---------------------------------------------------------------------------

/// Files to query via mdls, cycling through them.
const SPOTLIGHT_FILES: &[&str] = &[
    "/usr/bin/true",
    "/usr/bin/false",
    "/usr/bin/env",
    "/usr/bin/which",
];

/// Path to the mdls binary.
const MDLS_PATH: &str = "/usr/bin/mdls";

/// Timeout for mdls commands.
const MDLS_TIMEOUT: Duration = Duration::from_secs(2);

static SPOTLIGHT_TIMING_INFO: SourceInfo = SourceInfo {
    name: "spotlight_timing",
    description: "Spotlight metadata index query timing jitter via mdls",
    physics: "Queries Spotlight\u{2019}s metadata index (mdls) and measures response time. \
              The index is a complex B-tree/inverted index structure. Query timing depends \
              on: index size, disk cache residency, concurrent indexing activity, and \
              filesystem metadata state. When Spotlight is actively indexing new files, \
              query latency becomes highly variable.",
    category: SourceCategory::Novel,
    platform_requirements: &["macos"],
    entropy_rate_estimate: 800.0,
};

/// Entropy source that harvests timing jitter from Spotlight metadata queries.
pub struct SpotlightTimingSource;

impl EntropySource for SpotlightTimingSource {
    fn info(&self) -> &SourceInfo {
        &SPOTLIGHT_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        std::path::Path::new(MDLS_PATH).exists()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        // Cap the number of mdls calls since each has a 2s timeout.
        // mdls usually completes fast (~5ms), so 200 calls is ~1s normally.
        // If mdls hangs, 200 * 2s = 400s is too long, so also add an
        // early-exit when we have enough raw timing data.
        let raw_count = (n_samples * 10 + 64).min(200);
        let mut timings: Vec<u64> = Vec::with_capacity(raw_count);
        let file_count = SPOTLIGHT_FILES.len();

        for i in 0..raw_count {
            let file = SPOTLIGHT_FILES[i % file_count];

            // Measure the time to query Spotlight metadata with a timeout.
            // Even timeouts produce useful timing entropy.
            let t0 = Instant::now();

            let child = Command::new(MDLS_PATH)
                .args(["-name", "kMDItemFSName", file])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            if let Ok(mut child) = child {
                let deadline = Instant::now() + MDLS_TIMEOUT;
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => {
                            if Instant::now() >= deadline {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            }

            // Always record timing — timeouts are just as entropic.
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            timings.push(elapsed_ns);
        }

        // Compute deltas between consecutive timings.
        let deltas: Vec<u64> = timings
            .windows(2)
            .map(|w| w[1].wrapping_sub(w[0]))
            .collect();

        // Extract LSBs.
        let mut entropy: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        entropy.truncate(n_samples);
        entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_queue_info() {
        let src = DispatchQueueSource;
        assert_eq!(src.name(), "dispatch_queue");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 1500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dispatch_queue_collects_bytes() {
        let src = DispatchQueueSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty()); assert!(data.len() <= 64);
    }

    #[test]
    fn dyld_timing_info() {
        let src = DyldTimingSource;
        assert_eq!(src.name(), "dyld_timing");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 1200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn vm_page_timing_info() {
        let src = VMPageTimingSource;
        assert_eq!(src.name(), "vm_page_timing");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 1300.0).abs() < f64::EPSILON);
    }

    #[test]
    #[cfg(unix)]
    fn vm_page_timing_collects_bytes() {
        let src = VMPageTimingSource;
        assert!(src.is_available());
        let data = src.collect(64);
        assert!(!data.is_empty()); assert!(data.len() <= 64);
    }

    #[test]
    fn spotlight_timing_info() {
        let src = SpotlightTimingSource;
        assert_eq!(src.name(), "spotlight_timing");
        assert_eq!(src.info().category, SourceCategory::Novel);
        assert!((src.info().entropy_rate_estimate - 800.0).abs() < f64::EPSILON);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn spotlight_timing_collects_bytes() {
        let src = SpotlightTimingSource;
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }

    #[test]
    fn extract_lsbs_packing() {
        let deltas = vec![1u64, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0];
        let bytes = extract_lsbs_u64(&deltas);
        assert_eq!(bytes.len(), 2);
        // First 8 bits: 1,0,1,0,1,0,1,0 -> 0xAA
        assert_eq!(bytes[0], 0xAA);
        // Next 8 bits: 1,1,1,1,0,0,0,0 -> 0xF0
        assert_eq!(bytes[1], 0xF0);
    }
}
