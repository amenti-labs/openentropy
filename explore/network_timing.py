#!/usr/bin/env python3
"""
Network Timing Entropy — DNS, TCP, and HTTP timing jitter.
The microsecond-level variations contain genuine environmental randomness.
"""
import socket
import time
import os
import hashlib
import numpy as np


def dns_query_timing(hostname, dns_server='8.8.8.8', port=53, timeout=2.0):
    """Time a single DNS query in nanoseconds."""
    txn_id = os.urandom(2)
    flags = b'\x01\x00'
    counts = b'\x00\x01\x00\x00\x00\x00\x00\x00'
    question = b''
    for part in hostname.split('.'):
        question += bytes([len(part)]) + part.encode()
    question += b'\x00\x00\x01\x00\x01'
    packet = txn_id + flags + counts + question
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(timeout)
    try:
        t1 = time.perf_counter_ns()
        sock.sendto(packet, (dns_server, port))
        _ = sock.recvfrom(512)
        t2 = time.perf_counter_ns()
        return t2 - t1
    except (socket.timeout, OSError):
        return None
    finally:
        sock.close()


def collect_dns_timings(n_per_server=30):
    """Collect DNS timing from many servers."""
    servers = [
        '8.8.8.8', '8.8.4.4',           # Google
        '1.1.1.1', '1.0.0.1',           # Cloudflare
        '9.9.9.9', '149.112.112.112',   # Quad9
        '208.67.222.222', '208.67.220.220',  # OpenDNS
        '94.140.14.14',                   # AdGuard
        '76.76.2.0',                      # Control D
    ]
    hostnames = [
        'example.com', 'google.com', 'github.com', 'amazon.com',
        'cloudflare.com', 'wikipedia.org', 'reddit.com', 'apple.com',
        'twitter.com', 'microsoft.com', 'netflix.com', 'yahoo.com',
    ]
    
    all_timings = {}
    for server in servers:
        timings = []
        print(f"  DNS {server}...", end='', flush=True)
        for i in range(n_per_server):
            host = hostnames[i % len(hostnames)]
            t = dns_query_timing(host, dns_server=server)
            if t is not None:
                timings.append(t)
            time.sleep(0.01)
        all_timings[server] = np.array(timings) if timings else np.array([])
        if len(timings) > 0:
            print(f" {len(timings)} samples, mean={np.mean(timings)/1e6:.2f}ms")
        else:
            print(" failed")
    
    return all_timings


def tcp_connect_timing(n_per_host=15):
    """Measure TCP connection establishment timing."""
    hosts = [
        ('google.com', 80), ('google.com', 443),
        ('cloudflare.com', 80), ('cloudflare.com', 443),
        ('github.com', 80), ('github.com', 443),
        ('amazon.com', 80), ('apple.com', 443),
        ('microsoft.com', 80), ('wikipedia.org', 443),
    ]
    
    timings = []
    for host, port in hosts:
        print(f"  TCP {host}:{port}...", end='', flush=True)
        count = 0
        for _ in range(n_per_host):
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(3.0)
            try:
                t1 = time.perf_counter_ns()
                sock.connect((host, port))
                t2 = time.perf_counter_ns()
                timings.append(t2 - t1)
                count += 1
            except (socket.timeout, OSError):
                pass
            finally:
                sock.close()
            time.sleep(0.03)
        print(f" {count} ok")
    
    return np.array(timings) if timings else np.array([])


def extract_entropy(timings, n_bits=4):
    """Extract entropy bits from timing LSBs."""
    mask = (1 << n_bits) - 1
    return np.bitwise_and(timings.astype(np.int64), mask)


def run(output_file='explore/entropy_network_timing.bin'):
    print("=" * 60)
    print("NETWORK TIMING ENTROPY — DNS + TCP Jitter Harvester")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # DNS timing
    print("\n[Phase 1] DNS query timing...")
    dns_timings = collect_dns_timings(n_per_server=25)
    
    all_dns = []
    for server, timings in dns_timings.items():
        if len(timings) > 0:
            lsb = extract_entropy(timings)
            all_dns.extend(timings)
            # Also extract full 8-bit LSBs for more entropy
            lsb8 = (timings.astype(np.int64) & 0xFF).astype(np.uint8)
            all_entropy.extend(lsb8.tobytes())
    
    print(f"  Total DNS samples: {len(all_dns)}")
    
    # TCP timing
    print("\n[Phase 2] TCP connection timing...")
    tcp_timings = tcp_connect_timing(n_per_host=10)
    if len(tcp_timings) > 0:
        print(f"  TCP samples: {len(tcp_timings)}, Mean: {np.mean(tcp_timings)/1e6:.2f}ms")
        lsb8 = (tcp_timings.astype(np.int64) & 0xFF).astype(np.uint8)
        all_entropy.extend(lsb8.tobytes())
    
    if not all_entropy:
        print("[FAIL] No timing data collected")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    # Statistics
    arr = np.frombuffer(bytes(all_entropy), dtype=np.uint8)
    unique = len(set(arr.tolist()))
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    
    print(f"\n[RESULT] {len(all_entropy)} entropy bytes")
    print(f"  Unique byte values: {unique}/256")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
    print(f"  Compression ratio: {ratio:.3f}")
    
    return {'total_bytes': len(all_entropy), 'sha256': sha}


if __name__ == '__main__':
    run()
