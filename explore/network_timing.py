#!/usr/bin/env python3
"""
Harvest entropy from network packet timing jitter.

DNS query resolution times vary due to:
- Network path congestion
- Server load
- Route changes
- Packet queuing
- Wireless channel conditions

The microsecond-level timing variations contain genuine
environmental randomness that is very difficult to predict.
"""
import socket
import struct
import time
import os
import numpy as np


def dns_query_timing(hostname, dns_server='8.8.8.8', port=53, timeout=2.0):
    """Time a single DNS query and return resolution time in nanoseconds."""
    # Build a minimal DNS query
    txn_id = os.urandom(2)
    flags = b'\x01\x00'  # Standard query, recursion desired
    counts = b'\x00\x01\x00\x00\x00\x00\x00\x00'  # 1 question
    
    # Encode hostname
    question = b''
    for part in hostname.split('.'):
        question += bytes([len(part)]) + part.encode()
    question += b'\x00'  # root
    question += b'\x00\x01'  # Type A
    question += b'\x00\x01'  # Class IN
    
    packet = txn_id + flags + counts + question
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(timeout)
    
    try:
        t1 = time.perf_counter_ns()
        sock.sendto(packet, (dns_server, port))
        _ = sock.recvfrom(512)
        t2 = time.perf_counter_ns()
        return t2 - t1
    except socket.timeout:
        return None
    finally:
        sock.close()


def collect_dns_timings(n_samples=200, servers=None, hostnames=None):
    """Collect DNS query timing samples from multiple servers."""
    if servers is None:
        servers = [
            '8.8.8.8',       # Google
            '1.1.1.1',       # Cloudflare
            '9.9.9.9',       # Quad9
            '208.67.222.222', # OpenDNS
        ]
    
    if hostnames is None:
        hostnames = [
            'example.com', 'google.com', 'github.com', 'amazon.com',
            'cloudflare.com', 'wikipedia.org', 'reddit.com', 'apple.com',
        ]
    
    all_timings = {}
    
    for server in servers:
        timings = []
        print(f"  Querying {server}...")
        for i in range(n_samples // len(servers)):
            host = hostnames[i % len(hostnames)]
            t = dns_query_timing(host, dns_server=server)
            if t is not None:
                timings.append(t)
            time.sleep(0.01)  # Small delay to avoid rate limiting
        all_timings[server] = np.array(timings)
    
    return all_timings


def tcp_connect_timing(hosts=None, n_per_host=20):
    """Measure TCP connection establishment timing."""
    if hosts is None:
        hosts = [
            ('google.com', 80),
            ('cloudflare.com', 80),
            ('github.com', 80),
        ]
    
    timings = []
    for host, port in hosts:
        print(f"  TCP connect to {host}:{port}...")
        for _ in range(n_per_host):
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(3.0)
            try:
                t1 = time.perf_counter_ns()
                sock.connect((host, port))
                t2 = time.perf_counter_ns()
                timings.append(t2 - t1)
            except (socket.timeout, OSError):
                pass
            finally:
                sock.close()
            time.sleep(0.05)
    
    return np.array(timings)


def extract_entropy(timings, n_bits=4):
    """Extract entropy bits from timing LSBs."""
    mask = (1 << n_bits) - 1
    return np.bitwise_and(timings.astype(np.int64), mask)


if __name__ == '__main__':
    print("=== Network Timing Entropy Explorer ===\n")
    
    # DNS timing
    print("--- DNS Query Timing ---")
    dns_timings = collect_dns_timings(n_samples=100)
    
    all_dns = []
    for server, timings in dns_timings.items():
        if len(timings) > 0:
            print(f"\n  {server}:")
            print(f"    Samples: {len(timings)}")
            print(f"    Mean: {np.mean(timings)/1e6:.2f} ms")
            print(f"    Std: {np.std(timings)/1e6:.2f} ms")
            
            lsb = extract_entropy(timings)
            unique, counts = np.unique(lsb, return_counts=True)
            probs = counts / len(lsb)
            ent = -np.sum(probs * np.log2(probs + 1e-15))
            print(f"    LSB(4bit) entropy: {ent:.4f} / 4.0 bits")
            all_dns.extend(timings)
    
    # TCP timing
    print("\n--- TCP Connect Timing ---")
    tcp_timings = tcp_connect_timing()
    if len(tcp_timings) > 0:
        print(f"\n  Combined TCP:")
        print(f"    Samples: {len(tcp_timings)}")
        print(f"    Mean: {np.mean(tcp_timings)/1e6:.2f} ms")
        print(f"    Std: {np.std(tcp_timings)/1e6:.2f} ms")
        
        tcp_lsb = extract_entropy(tcp_timings)
        unique, counts = np.unique(tcp_lsb, return_counts=True)
        probs = counts / len(tcp_lsb)
        ent = -np.sum(probs * np.log2(probs + 1e-15))
        print(f"    LSB(4bit) entropy: {ent:.4f} / 4.0 bits")
    
    # Combine
    combined_timings = np.array(all_dns + list(tcp_timings))
    combined_lsb = extract_entropy(combined_timings)
    
    outfile = 'entropy_network_timing.bin'
    combined_lsb.astype(np.uint8).tofile(outfile)
    print(f"\nSaved {len(combined_lsb)} LSB samples to {outfile}")
