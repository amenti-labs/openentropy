//! Network-based entropy sources: DNS query timing and TCP handshake timing.
//!
//! These sources exploit the inherent unpredictability in network round-trip
//! times, which arise from queuing delays, congestion, server load, NIC
//! interrupt coalescing, and electromagnetic propagation variations.

use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::source::{EntropySource, SourceCategory, SourceInfo};

// ---------------------------------------------------------------------------
// DNS timing source
// ---------------------------------------------------------------------------

const DNS_SERVERS: &[&str] = &["8.8.8.8", "1.1.1.1", "9.9.9.9"];
const DNS_HOSTNAMES: &[&str] = &["example.com", "google.com", "github.com"];
const DNS_PORT: u16 = 53;
const DNS_TIMEOUT: Duration = Duration::from_secs(2);

/// Entropy source that measures the round-trip time of DNS A-record queries
/// sent to public resolvers.  Timing jitter in the nanosecond range is
/// harvested as raw entropy.
pub struct DNSTimingSource {
    info: SourceInfo,
    /// Monotonically increasing index used to cycle through servers/hostnames.
    index: AtomicUsize,
}

impl DNSTimingSource {
    pub fn new() -> Self {
        Self {
            info: SourceInfo {
                name: "dns_timing",
                description: "Round-trip timing of DNS A-record queries to public resolvers",
                physics: "Measures round-trip time of DNS queries to public resolvers. \
                          Jitter comes from: network switch queuing, router buffer state, \
                          ISP congestion, DNS server load, TCP/IP stack scheduling, NIC \
                          interrupt coalescing, and electromagnetic propagation variations.",
                category: SourceCategory::Network,
                platform_requirements: &[],
                entropy_rate_estimate: 100.0,
                composite: false,
            },
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

    // Use low 16 bits of current nanosecond timestamp as transaction ID.
    let tx_id = (Instant::now().elapsed().as_nanos() & 0xFFFF) as u16;
    let query = build_dns_query(tx_id, hostname);

    let start = Instant::now();
    socket.send_to(&query, addr).ok()?;

    let mut buf = [0u8; 512];
    let _n = socket.recv_from(&mut buf).ok()?;
    Some(start.elapsed().as_nanos())
}

impl EntropySource for DNSTimingSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        // Try one query; if we get a response within the timeout the source is
        // usable.
        dns_query_rtt(DNS_SERVERS[0], DNS_HOSTNAMES[0], DNS_TIMEOUT).is_some()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut entropy = Vec::with_capacity(n_samples);
        let server_count = DNS_SERVERS.len();
        let hostname_count = DNS_HOSTNAMES.len();

        let mut prev_nanos: Option<u128> = None;

        while entropy.len() < n_samples {
            let idx = self.index.fetch_add(1, Ordering::Relaxed);
            let server = DNS_SERVERS[idx % server_count];
            let hostname = DNS_HOSTNAMES[idx % hostname_count];

            if let Some(nanos) = dns_query_rtt(server, hostname, DNS_TIMEOUT) {
                // Extract least-significant bytes of the RTT (most jittery bits).
                let nanos_bytes = nanos.to_le_bytes(); // 16 bytes (u128)

                // LSB of the raw RTT
                entropy.push(nanos_bytes[0]);
                if entropy.len() >= n_samples {
                    break;
                }

                // Second byte has some entropy too
                entropy.push(nanos_bytes[1]);
                if entropy.len() >= n_samples {
                    break;
                }

                // XOR of byte 0 and byte 1 for mixing
                entropy.push(nanos_bytes[0] ^ nanos_bytes[1]);
                if entropy.len() >= n_samples {
                    break;
                }

                // Timing delta from previous query (inter-query jitter)
                if let Some(prev) = prev_nanos {
                    let delta = nanos.abs_diff(prev);
                    let delta_bytes = delta.to_le_bytes();
                    entropy.push(delta_bytes[0]);
                    if entropy.len() < n_samples {
                        entropy.push(delta_bytes[1]);
                    }
                }
                prev_nanos = Some(nanos);
            }
            // On failure, just move on to the next server/hostname pair.
        }

        entropy.truncate(n_samples);
        entropy
    }
}

// ---------------------------------------------------------------------------
// TCP connect timing source
// ---------------------------------------------------------------------------

const TCP_TARGETS: &[&str] = &["8.8.8.8:53", "1.1.1.1:53", "9.9.9.9:53"];
const TCP_TIMEOUT: Duration = Duration::from_secs(2);

/// Entropy source that times TCP three-way handshakes to remote hosts.
/// The nanosecond-resolution timing captures NIC DMA jitter, kernel buffer
/// allocation, remote server load, and network path congestion.
pub struct TCPConnectSource {
    info: SourceInfo,
    /// Monotonically increasing index used to cycle through targets.
    index: AtomicUsize,
}

impl TCPConnectSource {
    pub fn new() -> Self {
        Self {
            info: SourceInfo {
                name: "tcp_connect_timing",
                description: "Nanosecond timing of TCP three-way handshakes to remote hosts",
                physics: "Times the TCP three-way handshake (SYN -> SYN-ACK -> ACK). \
                          The timing captures: NIC DMA transfer jitter, kernel socket \
                          buffer allocation, remote server load, network path congestion, \
                          and router queuing delays.",
                category: SourceCategory::Network,
                platform_requirements: &[],
                entropy_rate_estimate: 50.0,
                composite: false,
            },
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
        &self.info
    }

    fn is_available(&self) -> bool {
        tcp_connect_rtt(TCP_TARGETS[0], TCP_TIMEOUT).is_some()
    }

    fn collect(&self, n_samples: usize) -> Vec<u8> {
        let mut entropy = Vec::with_capacity(n_samples);
        let target_count = TCP_TARGETS.len();

        let mut prev_nanos: Option<u128> = None;

        while entropy.len() < n_samples {
            let idx = self.index.fetch_add(1, Ordering::Relaxed);
            let target = TCP_TARGETS[idx % target_count];

            if let Some(nanos) = tcp_connect_rtt(target, TCP_TIMEOUT) {
                let nanos_bytes = nanos.to_le_bytes();

                // Least-significant bytes of the handshake RTT
                entropy.push(nanos_bytes[0]);
                if entropy.len() >= n_samples {
                    break;
                }

                entropy.push(nanos_bytes[1]);
                if entropy.len() >= n_samples {
                    break;
                }

                // XOR mix
                entropy.push(nanos_bytes[0] ^ nanos_bytes[1]);
                if entropy.len() >= n_samples {
                    break;
                }

                // Timing delta from previous handshake
                if let Some(prev) = prev_nanos {
                    let delta = nanos.abs_diff(prev);
                    let delta_bytes = delta.to_le_bytes();
                    entropy.push(delta_bytes[0]);
                    if entropy.len() < n_samples {
                        entropy.push(delta_bytes[1]);
                    }
                }
                prev_nanos = Some(nanos);
            }
        }

        entropy.truncate(n_samples);
        entropy
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
        assert!((src.info().entropy_rate_estimate - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tcp_source_info() {
        let src = TCPConnectSource::new();
        assert_eq!(src.info().name, "tcp_connect_timing");
        assert_eq!(src.info().category, SourceCategory::Network);
        assert!((src.info().entropy_rate_estimate - 50.0).abs() < f64::EPSILON);
    }
}
