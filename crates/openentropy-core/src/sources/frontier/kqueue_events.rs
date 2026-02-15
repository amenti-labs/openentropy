//! Kqueue event timing — entropy from BSD kernel event notification multiplexing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Configuration for kqueue events entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::KqueueEventsConfig;
/// let config = KqueueEventsConfig {
///     num_file_watchers: 2,   // fewer watchers
///     num_timers: 16,         // more timers for richer interference
///     num_sockets: 4,         // default socket pairs
///     timeout_ms: 2,          // slightly longer timeout
/// };
/// ```
#[derive(Debug, Clone)]
pub struct KqueueEventsConfig {
    /// Number of file watchers to register via `EVFILT_VNODE`.
    ///
    /// Each watcher monitors a temp file for write/attribute changes.
    /// The filesystem notification path traverses VFS, APFS/HFS event queues,
    /// and the kqueue knote hash table.
    ///
    /// **Range:** 0+. **Default:** `4`
    pub num_file_watchers: usize,

    /// Number of timer events to register via `EVFILT_TIMER`.
    ///
    /// Each timer fires at a different interval (1-10ms). Multiple timers
    /// create scheduling contention and exercise kernel timer coalescing.
    /// Timer delivery is affected by interrupt handling and power management.
    ///
    /// **Range:** 0+. **Default:** `8`
    pub num_timers: usize,

    /// Number of socket pairs for `EVFILT_READ`/`EVFILT_WRITE` monitoring.
    ///
    /// Socket buffer management interacts with the network stack's mbuf
    /// allocator. A background thread periodically writes to sockets to
    /// generate asynchronous events.
    ///
    /// **Range:** 0+. **Default:** `4`
    pub num_sockets: usize,

    /// Timeout in milliseconds for `kevent()` calls.
    ///
    /// Controls how long each `kevent()` waits for events. Shorter timeouts
    /// capture more frequent timing samples; longer timeouts allow more
    /// events to accumulate per call.
    ///
    /// **Range:** 1+. **Default:** `1`
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

/// Harvests timing jitter from kqueue event notification multiplexing.
///
/// # What it measures
/// Nanosecond timing of `kevent()` calls with multiple registered event
/// types (timers, file watchers, socket monitors) firing concurrently.
///
/// # Why it's entropic
/// kqueue is the macOS/BSD kernel event notification system. Registering
/// diverse event types simultaneously creates rich interference:
/// - **Timer events** — `EVFILT_TIMER` with different intervals fire at
///   kernel-determined times affected by timer coalescing, interrupt handling,
///   and power management state
/// - **File watchers** — `EVFILT_VNODE` on temp files monitors inode changes;
///   traverses VFS, APFS/HFS event queues, and the kqueue knote hash table
/// - **Socket events** — `EVFILT_READ`/`EVFILT_WRITE` on socket pairs monitors
///   buffer state; interacts with the network stack's mbuf allocator
/// - **Knote lock contention** — many registered watchers all compete for the
///   kqueue's internal knote lock and dispatch queue
///
/// # What makes it unique
/// No prior work has combined multiple kqueue event types as an entropy source.
/// The cross-event-type interference (timer delivery affecting socket
/// notification timing) produces entropy that is independent from any single
/// event source.
///
/// # Configuration
/// See [`KqueueEventsConfig`] for tunable parameters. Key options:
/// - `num_timers`: controls timer coalescing interference
/// - `num_sockets`: controls mbuf allocator contention
/// - `num_file_watchers`: controls VFS notification path diversity
/// - `timeout_ms`: controls `kevent()` wait duration
#[derive(Default)]
pub struct KqueueEventsSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
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
    category: SourceCategory::IPC,
    platform: Platform::MacOS,
    requirements: &[],
    entropy_rate_estimate: 2500.0,
    composite: false,
};

impl EntropySource for KqueueEventsSource {
    fn info(&self) -> &SourceInfo {
        &KQUEUE_EVENTS_INFO
    }

    fn is_available(&self) -> bool {
        cfg!(any(
            target_os = "macos",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))
    }

    #[cfg(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
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
        for _i in 0..self.config.num_sockets {
            let mut sv: [i32; 2] = [0; 2];
            let ret =
                unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, sv.as_mut_ptr()) };
            if ret == 0 {
                cleanup_fds.push(sv[0]);
                cleanup_fds.push(sv[1]);

                let mut ev: libc::kevent = unsafe { std::mem::zeroed() };
                ev.ident = sv[0] as usize;
                ev.filter = libc::EVFILT_READ;
                ev.flags = libc::EV_ADD | libc::EV_ENABLE;
                ev.udata = std::ptr::null_mut();
                changes.push(ev);

                let byte = [0xAAu8];
                unsafe {
                    libc::write(sv[1], byte.as_ptr() as *const _, 1);
                }

                let mut ev2: libc::kevent = unsafe { std::mem::zeroed() };
                ev2.ident = sv[1] as usize;
                ev2.filter = libc::EVFILT_WRITE;
                ev2.flags = libc::EV_ADD | libc::EV_ENABLE;
                ev2.udata = std::ptr::null_mut();
                changes.push(ev2);
            }
        }

        // Register file watchers on temp files.
        let mut temp_files: Vec<(i32, std::path::PathBuf)> = Vec::new();
        for i in 0..self.config.num_file_watchers {
            let path = std::env::temp_dir().join(format!("oe_kq_{i}_{}", std::process::id()));
            if std::fs::write(&path, b"entropy").is_ok() {
                let path_str = path.to_str().unwrap_or("");
                let c_path = match std::ffi::CString::new(path_str) {
                    Ok(c) => c,
                    Err(_) => continue, // skip paths with null bytes
                };
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

        // Spawn a thread to periodically poke watched files and sockets.
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let socket_write_fds: Vec<i32> = cleanup_fds.iter().skip(1).step_by(2).copied().collect();
        let file_paths: Vec<std::path::PathBuf> =
            temp_files.iter().map(|(_, p)| p.clone()).collect();

        let poker = thread::spawn(move || {
            let byte = [0xBBu8];
            while !stop2.load(Ordering::Relaxed) {
                for &fd in &socket_write_fds {
                    unsafe {
                        libc::write(fd, byte.as_ptr() as *const _, 1);
                    }
                }
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

        let socket_read_fds: Vec<i32> = cleanup_fds.iter().step_by(2).copied().collect();

        for _ in 0..raw_count {
            let t0 = mach_time();

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
            unsafe {
                libc::close(*fd);
            }
            let _ = std::fs::remove_file(path);
        }
        for &fd in &cleanup_fds {
            unsafe {
                libc::close(fd);
            }
        }
        unsafe {
            libc::close(kq);
        }

        extract_timing_entropy(&timings, n_samples)
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    fn collect(&self, _n_samples: usize) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = KqueueEventsSource::default();
        assert_eq!(src.name(), "kqueue_events");
        assert_eq!(src.info().category, SourceCategory::IPC);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = KqueueEventsConfig::default();
        assert_eq!(config.num_file_watchers, 4);
        assert_eq!(config.num_timers, 8);
        assert_eq!(config.num_sockets, 4);
        assert_eq!(config.timeout_ms, 1);
    }

    #[test]
    fn custom_config() {
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
    fn collects_bytes() {
        let src = KqueueEventsSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }
}
