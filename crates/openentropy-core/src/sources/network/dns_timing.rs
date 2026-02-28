//! DNS timing entropy source.
//!
//! Exploits the inherent unpredictability in DNS query round-trip times,
//! which arise from queuing delays, congestion, server load, NIC
//! interrupt coalescing, and electromagnetic propagation variations.

use std::net::{SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::source::{EntropySource, Platform, SourceCategory, SourceInfo};
use crate::sources::helpers::extract_timing_entropy;

// ---------------------------------------------------------------------------
// DNS timing source
// ---------------------------------------------------------------------------

const DNS_SERVERS: &[&str] = &["8.8.8.8", "1.1.1.1", "9.9.9.9"];
const DNS_HOSTNAMES: &[&str] = &["example.com", "google.com", "github.com"];
const DNS_PORT: u16 = 53;
const DNS_TIMEOUT: Duration = Duration::from_secs(2);

/// Entropy source that measures the round-trip time of DNS A-record queries
/// sent to public resolvers. Timing jitter in the nanosecond range is
/// harvested as raw entropy.
///
/// No tunable parameters — cycles through a fixed set of public DNS servers
/// and hostnames automatically.
pub struct DNSTimingSource {
    /// Monotonically increasing index used to cycle through servers/hostnames.
    index: AtomicUsize,
}

static DNS_TIMING_INFO: SourceInfo = SourceInfo {
    name: "dns_timing",
    description: "Round-trip timing of DNS A-record queries to public resolvers",
    physics: "Measures round-trip time of DNS queries to public resolvers. \
              Jitter comes from: network switch queuing, router buffer state, \
              ISP congestion, DNS server load, TCP/IP stack scheduling, NIC \
              interrupt coalescing, and electromagnetic propagation variations.",
    category: SourceCategory::Network,
    platform: Platform::Any,
    requirements: &[],
    entropy_rate_estimate: 3.0,
    composite: false,
    is_fast: false,
};

impl DNSTimingSource {
    pub fn new() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}

impl Default for DNSTimingSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode a hostname into DNS wire format (sequence of length-prefixed labels).
///
/// Example: "example.com" -> b"\x07example\x03com\x00"
fn encode_dns_name(hostname: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hostname.len() + 2);
    for label in hostname.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0); // root label
    out
}

/// Build a minimal DNS query packet for an A record.
fn build_dns_query(tx_id: u16, hostname: &str) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(32);
    // Header
    pkt.extend_from_slice(&tx_id.to_be_bytes()); // Transaction ID
    pkt.extend_from_slice(&[0x01, 0x00]); // Flags: standard query, recursion desired
    pkt.extend_from_slice(&[0x00, 0x01]); // Questions: 1
    pkt.extend_from_slice(&[0x00, 0x00]); // Answer RRs: 0
    pkt.extend_from_slice(&[0x00, 0x00]); // Authority RRs: 0
    pkt.extend_from_slice(&[0x00, 0x00]); // Additional RRs: 0
    // Question section
    pkt.extend_from_slice(&encode_dns_name(hostname));
    pkt.extend_from_slice(&[0x00, 0x01]); // Type: A
    pkt.extend_from_slice(&[0x00, 0x01]); // Class: IN
    pkt
}

/// Send a single DNS query and return the RTT in nanoseconds, or `None` on
/// failure.
fn dns_query_rtt(server: &str, hostname: &str, timeout: Duration) -> Option<u128> {
    let addr: SocketAddr = format!("{}:{}", server, DNS_PORT).parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(timeout)).ok()?;
    socket.set_write_timeout(Some(timeout)).ok()?;

    // Use low 16 bits of wall clock nanoseconds as transaction ID.
    let tx_id = (std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        & 0xFFFF) as u16;
    let query = build_dns_query(tx_id, hostname);

    let tx_id_bytes = tx_id.to_be_bytes();
    let start = Instant::now();
    socket.send_to(&query, addr).ok()?;

    let mut buf = [0u8; 512];
    // Read responses until we get one matching our transaction ID or timeout.
    // Cap attempts to avoid spinning on a flood of stale responses.
    for _ in 0..8 {
        let (n, _src) = socket.recv_from(&mut buf).ok()?;
        if n >= 2 && buf[0] == tx_id_bytes[0] && buf[1] == tx_id_bytes[1] {
            return Some(start.elapsed().as_nanos());
        }
        // Wrong txid — stale response from a prior query. Try again.
    }
    None
}

impl EntropySource for DNSTimingSource {
    fn info(&self) -> &SourceInfo {
        &DNS_TIMING_INFO
    }

    fn is_available(&self) -> bool {
        static DNS_AVAILABLE: OnceLock<bool> = OnceLock::new();
        *DNS_AVAILABLE
            .get_or_init(|| dns_query_rtt(DNS_SERVERS[0], DNS_HOSTNAMES[0], DNS_TIMEOUT).is_some())
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let server_count = DNS_SERVERS.len();
        let hostname_count = DNS_HOSTNAMES.len();

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
            let server = DNS_SERVERS[idx % server_count];
            let hostname = DNS_HOSTNAMES[idx % hostname_count];

            if let Some(nanos) = dns_query_rtt(server, hostname, DNS_TIMEOUT) {
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
    fn dns_name_encoding() {
        let encoded = encode_dns_name("example.com");
        assert_eq!(encoded[0], 7); // length of "example"
        assert_eq!(&encoded[1..8], b"example");
        assert_eq!(encoded[8], 3); // length of "com"
        assert_eq!(&encoded[9..12], b"com");
        assert_eq!(encoded[12], 0); // root label
    }

    #[test]
    fn dns_query_packet_structure() {
        let pkt = build_dns_query(0x1234, "example.com");
        // Transaction ID
        assert_eq!(pkt[0], 0x12);
        assert_eq!(pkt[1], 0x34);
        // Flags: standard query, recursion desired
        assert_eq!(pkt[2], 0x01);
        assert_eq!(pkt[3], 0x00);
        // Questions count
        assert_eq!(pkt[4], 0x00);
        assert_eq!(pkt[5], 0x01);
        // The packet should end with type A (0x0001) and class IN (0x0001)
        let len = pkt.len();
        assert_eq!(&pkt[len - 4..], &[0x00, 0x01, 0x00, 0x01]);
    }

    #[test]
    fn dns_source_info() {
        let src = DNSTimingSource::new();
        assert_eq!(src.info().name, "dns_timing");
        assert_eq!(src.info().category, SourceCategory::Network);
        assert!((src.info().entropy_rate_estimate - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Requires network connectivity
    fn dns_timing_collects_bytes() {
        let src = DNSTimingSource::new();
        if src.is_available() {
            let data = src.collect(32);
            assert!(!data.is_empty());
            assert!(data.len() <= 32);
        }
    }
}
