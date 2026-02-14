//! Pipe buffer timing — entropy from multi-pipe kernel zone allocator contention.

use crate::source::{EntropySource, SourceCategory, SourceInfo};
use crate::sources::helpers::{extract_timing_entropy, mach_time};

/// Configuration for pipe buffer entropy collection.
///
/// # Example
/// ```
/// # use openentropy_core::sources::frontier::PipeBufferConfig;
/// let config = PipeBufferConfig {
///     num_pipes: 8,              // more pipes = more zone contention
///     min_write_size: 64,        // skip tiny writes
///     max_write_size: 2048,      // cap at 2KB
///     non_blocking: true,        // capture EAGAIN timing (recommended)
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PipeBufferConfig {
    /// Number of pipes to use simultaneously.
    ///
    /// Multiple pipes competing for kernel buffer space creates zone allocator
    /// contention. Each pipe is allocated from the kernel's pipe zone, and
    /// cross-CPU magazine transfers add nondeterminism.
    ///
    /// **Range:** 1+ (clamped to >=1). **Default:** `4`
    pub num_pipes: usize,

    /// Minimum write size in bytes.
    ///
    /// Small writes use inline pipe buffer storage; larger writes chain mbufs.
    /// The transition between these paths adds entropy.
    ///
    /// **Range:** 1+. **Default:** `1`
    pub min_write_size: usize,

    /// Maximum write size in bytes.
    ///
    /// Larger writes exercise different mbuf allocation paths and are more
    /// likely to trigger cross-CPU magazine transfers in the zone allocator.
    ///
    /// **Range:** >= `min_write_size`. **Default:** `4096`
    pub max_write_size: usize,

    /// Use non-blocking mode for pipe writes.
    ///
    /// Non-blocking writes that hit `EAGAIN` (pipe buffer full) follow a
    /// different kernel path than blocking writes. The timing of the failure
    /// check is itself a source of entropy.
    ///
    /// **Default:** `true`
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

/// Harvests timing jitter from pipe I/O with multiple pipes competing for
/// kernel buffer space.
///
/// # What it measures
/// Nanosecond timing of `write()` + `read()` cycles on a pool of pipes,
/// with variable write sizes and periodic pipe creation/destruction for
/// zone allocator churn.
///
/// # Why it's entropic
/// Multiple simultaneous pipes competing for kernel zone allocator resources
/// amplifies nondeterminism:
/// - **Zone allocator contention** — multiple pipes allocating from the pipe
///   zone simultaneously creates cross-CPU magazine transfer contention
/// - **Variable buffer sizes** — different write sizes exercise different mbuf
///   allocation paths (small = inline storage, large = chained mbufs)
/// - **Non-blocking I/O** — `EAGAIN` on full pipe buffers follows a different
///   kernel path with its own latency characteristics
/// - **Cross-pipe interference** — reading from one pipe while another has
///   pending data creates wakeup scheduling interference
///
/// # What makes it unique
/// Pipe buffers exercise the kernel's zone allocator (magazine layer) in a way
/// that no other entropy source does. The zone allocator's per-CPU caching
/// and cross-CPU transfers create timing that depends on every CPU's allocation
/// history.
///
/// # Configuration
/// See [`PipeBufferConfig`] for tunable parameters. Key options:
/// - `non_blocking`: capture EAGAIN failure path timing (recommended: `true`)
/// - `num_pipes`: controls zone allocator contention level
/// - `min_write_size`/`max_write_size`: controls mbuf allocation path diversity
#[derive(Default)]
pub struct PipeBufferSource {
    /// Source configuration. Use `Default::default()` for recommended settings.
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
    composite: false,
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
                    // SAFETY: fds[1] is a valid file descriptor from pipe().
                    unsafe {
                        let flags = libc::fcntl(fds[1], libc::F_GETFL);
                        libc::fcntl(fds[1], libc::F_SETFL, flags | libc::O_NONBLOCK);
                    }
                }
                pipe_pool.push(fds);
            }
        }

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

            let pipe_idx = i % pipe_pool.len();
            let fds = pipe_pool[pipe_idx];

            let t0 = mach_time();

            // SAFETY: fds are valid file descriptors from pipe().
            unsafe {
                let written = libc::write(fds[1], write_data.as_ptr() as *const _, write_size);

                if written > 0 {
                    libc::read(fds[0], read_buf.as_mut_ptr() as *mut _, written as usize);
                }
            }

            let t1 = mach_time();
            std::hint::black_box(&read_buf);
            timings.push(t1.wrapping_sub(t0));

            // Periodically create/destroy an extra pipe for zone allocator churn.
            if i % 8 == 0 {
                let mut extra_fds: [i32; 2] = [0; 2];
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
    pub(crate) fn collect_single_pipe(&self, n_samples: usize) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info() {
        let src = PipeBufferSource::default();
        assert_eq!(src.name(), "pipe_buffer");
        assert_eq!(src.info().category, SourceCategory::Frontier);
        assert!(!src.info().composite);
    }

    #[test]
    fn default_config() {
        let config = PipeBufferConfig::default();
        assert_eq!(config.num_pipes, 4);
        assert_eq!(config.min_write_size, 1);
        assert_eq!(config.max_write_size, 4096);
        assert!(config.non_blocking);
    }

    #[test]
    fn custom_config() {
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
    fn collects_bytes() {
        let src = PipeBufferSource::default();
        if src.is_available() {
            let data = src.collect(64);
            assert!(!data.is_empty());
            assert!(data.len() <= 64);
        }
    }

    #[test]
    #[ignore] // Uses pipe syscall
    fn single_pipe_mode() {
        let src = PipeBufferSource {
            config: PipeBufferConfig {
                num_pipes: 0,
                ..PipeBufferConfig::default()
            },
        };
        if src.is_available() {
            assert!(!src.collect_single_pipe(64).is_empty());
        }
    }
}
