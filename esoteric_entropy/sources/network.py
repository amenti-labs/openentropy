"""Network timing entropy sources — DNS and TCP connect jitter."""

from __future__ import annotations

import os
import socket
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


class DNSTimingSource(EntropySource):
    """Entropy from DNS query round-trip time jitter.

    Each UDP DNS query traverses physical network links whose latency
    fluctuates due to queuing, routing decisions, congestion, and
    electromagnetic interference.  The nanosecond-level variations are
    genuine environmental randomness.
    """

    name = "dns_timing"
    description = "DNS query round-trip timing jitter"
    category = "network"
    physics = (
        "Measures round-trip time of DNS queries to public resolvers. Jitter comes from: network switch queuing, router buffer state, ISP congestion, DNS server load, TCP/IP stack scheduling, NIC interrupt coalescing, and electromagnetic propagation variations. Each query traverses dozens of independent physical systems."
    )
    platform_requirements: list[str] = []
    entropy_rate_estimate = 100.0

    DNS_SERVERS = ["8.8.8.8", "1.1.1.1", "9.9.9.9"]
    HOSTNAMES = ["example.com", "google.com", "github.com"]

    def is_available(self) -> bool:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.settimeout(2)
            s.connect(("8.8.8.8", 53))
            s.close()
            return True
        except OSError:
            return False

    def _query(self, host: str, server: str) -> int | None:
        txn_id = os.urandom(2)
        packet = txn_id + b"\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00"
        for part in host.split("."):
            packet += bytes([len(part)]) + part.encode()
        packet += b"\x00\x00\x01\x00\x01"

        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(2.0)
        try:
            t1 = time.perf_counter_ns()
            sock.sendto(packet, (server, 53))
            sock.recvfrom(512)
            return time.perf_counter_ns() - t1
        except OSError:
            return None
        finally:
            sock.close()

    def collect(self, n_samples: int = 100) -> np.ndarray:
        timings: list[int] = []
        for _ in range(max(1, n_samples // 9)):
            for server in self.DNS_SERVERS:
                for host in self.HOSTNAMES:
                    t = self._query(host, server)
                    if t is not None:
                        timings.append(t)
                    time.sleep(0.005)
        if not timings:
            return np.array([], dtype=np.uint8)
        return (np.array(timings, dtype=np.int64) & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(100), self.name)


class TCPConnectSource(EntropySource):
    """Entropy from TCP three-way handshake timing.

    Similar physics to DNS but uses TCP, adding SYN/ACK round-trip
    and server-side processing jitter.
    """

    name = "tcp_connect"
    description = "TCP handshake timing jitter"
    category = "network"
    physics = (
        "Times the TCP three-way handshake (SYN → SYN-ACK → ACK). The timing captures: "
        "NIC DMA transfer jitter, kernel socket buffer allocation, remote server load, "
        "network path congestion, and router queuing delays. The handshake crosses multiple "
        "autonomous systems, each adding independent timing noise."
    )
    platform_requirements: list[str] = []
    entropy_rate_estimate = 50.0

    TARGETS = [("8.8.8.8", 53), ("1.1.1.1", 53), ("9.9.9.9", 53)]

    def is_available(self) -> bool:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(3)
            s.connect(("8.8.8.8", 53))
            s.close()
            return True
        except OSError:
            return False

    def collect(self, n_samples: int = 50) -> np.ndarray:
        timings: list[int] = []
        for _ in range(max(1, n_samples // len(self.TARGETS))):
            for host, port in self.TARGETS:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3)
                try:
                    t0 = time.perf_counter_ns()
                    s.connect((host, port))
                    timings.append(time.perf_counter_ns() - t0)
                except OSError:
                    pass
                finally:
                    s.close()
                time.sleep(0.01)
        if not timings:
            return np.array([], dtype=np.uint8)
        return (np.array(timings, dtype=np.int64) & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        return self._quick_quality(self.collect(50), self.name)
