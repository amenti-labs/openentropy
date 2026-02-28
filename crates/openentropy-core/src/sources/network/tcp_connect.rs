//! TCP connect timing entropy source.
//!
//! Times TCP three-way handshakes to remote hosts. The nanosecond-resolution
//! timing captures NIC DMA jitter, kernel buffer allocation, remote server
//! load, and network path congestion.

use std::net::{SocketAddr, TcpStream};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

// ---------------------------------------------------------------------------
// TCP connect timing source
// ---------------------------------------------------------------------------

const TCP_TARGETS: &[&str] = &["8.8.8.8:53", "1.1.1.1:53", "9.9.9.9:53"];
const TCP_TIMEOUT: Duration = Duration::from_secs(2);

/// Entropy source that times TCP three-way handshakes to remote hosts.
/// The nanosecond-resolution timing captures NIC DMA jitter, kernel buffer
/// allocation, remote server load, and network path congestion.
///
/// No tunable parameters — cycles through a fixed set of TCP targets
/// automatically.
pub struct TCPConnectSource {
    /// Monotonically increasing index used to cycle through targets.
    index: AtomicUsize,
}

static TCP_CONNECT_INFO: SourceInfo = SourceInfo {
    name: "tcp_connect_timing",
    description: "Nanosecond timing of TCP three-way handshakes to remote hosts",
    physics: "Times the TCP three-way handshake (SYN -> SYN-ACK -> ACK). \
              The timing captures: NIC DMA transfer jitter, kernel socket \
              buffer allocation, remote server load, network path congestion, \
              and router queuing delays.",
    category: SourceCategory::Network,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 2.0,
    composite: false,
    is_fast: false,
};

impl TCPConnectSource {
    pub fn new() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}

impl Default for TCPConnectSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Attempt a TCP connect and return the handshake duration in nanoseconds.
fn tcp_connect_rtt(target: &str, timeout: Duration) -> Option<u128> {
    let addr: SocketAddr = target.parse().ok()?;
    let start = Instant::now();
    let _stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    Some(start.elapsed().as_nanos())
}

impl EntropySource for TCPConnectSource {
    fn info(&self) -> &SourceInfo {
        &TCP_CONNECT_INFO
    }

    fn is_available(&self) -> bool {
        static TCP_AVAILABLE: OnceLock<bool> = OnceLock::new();
        *TCP_AVAILABLE.get_or_init(|| tcp_connect_rtt(TCP_TARGETS[0], TCP_TIMEOUT).is_some())
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let target_count = TCP_TARGETS.len();

        let raw_count = n_samples + 64;
        let mut timings = Vec::with_capacity(raw_count);
        let max_iterations = raw_count * 4;
        let mut iter_count = 0;
        let deadline = Instant::now() + Duration::from_secs(4);

        while timings.len() < raw_count && iter_count < max_iterations {
            if Instant::now() >= deadline {
                break;
            }
            iter_count += 1;
            let idx = self.index.fetch_add(1, Ordering::Relaxed);
            let target = TCP_TARGETS[idx % target_count];

            if let Some(nanos) = tcp_connect_rtt(target, TCP_TIMEOUT) {
                timings.push(nanos as u64);
            }
        }

        extract_timing_entropy(&timings, n_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_source_info() {
        let src = TCPConnectSource::new();
        assert_eq!(src.info().name, "tcp_connect_timing");
        assert_eq!(src.info().category, SourceCategory::Network);
        assert!((src.info().entropy_rate_estimate - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Requires network connectivity
    fn tcp_connect_collects_bytes() {
        let src = TCPConnectSource::new();
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
