#!/usr/bin/env python3
"""
Discovery script: Find EVERY fluctuating data source on this Mac Mini.
Not just sensors — kernel counters, network state, process table, 
memory pressure, USB polling, everything.
"""
import subprocess
import time
import hashlib
import os
import json
import sys

def run(cmd, timeout=5):
    try:
        r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout)
        return r.stdout.strip()
    except:
        return ""

def sample_and_diff(cmd, label, n=3, delay=0.1):
    """Sample a command N times, measure how much changes."""
    samples = []
    for i in range(n):
        samples.append(run(cmd))
        if i < n-1:
            time.sleep(delay)
    
    if not samples[0]:
        return None
    
    # Count changing lines
    lines0 = set(samples[0].split('\n'))
    total_lines = len(lines0)
    changing_lines = 0
    unique_hashes = set()
    
    for s in samples:
        unique_hashes.add(hashlib.md5(s.encode()).hexdigest())
        lines_s = set(s.split('\n'))
        changing_lines += len(lines0.symmetric_difference(lines_s))
    
    changes = len(unique_hashes) - 1  # how many samples differed
    
    return {
        'label': label,
        'cmd': cmd,
        'total_lines': total_lines,
        'samples_that_changed': changes,
        'total_samples': n,
        'output_bytes': len(samples[0]),
    }

print("=== Overlooked Data Source Discovery ===\n")
print("Probing every accessible data source on this Mac Mini...\n")

sources = [
    # Kernel counters (sysctl)
    ("/usr/sbin/sysctl vm.swapusage", "VM swap usage"),
    ("/usr/sbin/sysctl vm.loadavg", "VM load average"),
    ("/usr/sbin/sysctl kern.boottime", "Kernel boot time (static baseline)"),
    ("/usr/sbin/sysctl net.inet.tcp.stats 2>/dev/null | head -30", "TCP stack statistics"),
    ("/usr/sbin/sysctl net.inet.udp.stats 2>/dev/null | head -20", "UDP stack statistics"),
    ("/usr/sbin/sysctl net.inet.ip.stats 2>/dev/null | head -30", "IP stack statistics"),
    ("/usr/sbin/sysctl net.inet.icmp.stats 2>/dev/null | head -20", "ICMP statistics"),
    ("/usr/sbin/sysctl hw.cachelinesize hw.l1dcachesize hw.l1icachesize hw.l2cachesize 2>/dev/null", "Cache sizes (static)"),
    ("/usr/sbin/sysctl machdep.cpu.thread_count machdep.cpu.core_count 2>/dev/null", "CPU topology (static)"),
    ("/usr/sbin/sysctl debug 2>/dev/null | wc -l", "Debug counters count"),
    
    # Network state (rapidly changing)
    ("netstat -s 2>/dev/null | head -40", "Netstat protocol statistics"),
    ("netstat -an 2>/dev/null | grep -c ESTABLISHED", "Active TCP connections count"),
    ("netstat -an 2>/dev/null | grep -c TIME_WAIT", "TIME_WAIT connections"),
    ("arp -a 2>/dev/null", "ARP table (network device fingerprint)"),
    ("ndp -a 2>/dev/null 2>&1 | head -20", "IPv6 neighbor discovery"),
    ("route -n get default 2>/dev/null", "Default route info"),
    
    # Process table entropy
    ("ps -eo pid,pcpu,pmem,rss,vsz,time 2>/dev/null | tail -20", "Process table snapshot"),
    ("ps -eo pid 2>/dev/null | wc -l", "Process count"),
    ("echo $$", "Shell PID (ASLR indicator)"),
    
    # File system state
    ("df -k 2>/dev/null", "Disk usage (changes with writes)"),
    ("ls -la /tmp/ 2>/dev/null | wc -l", "Temp file count"),
    ("stat -f '%m' /var/log/system.log 2>/dev/null", "System log last modified"),
    
    # System profiler (hardware state)  
    ("system_profiler SPPowerDataType 2>/dev/null | head -20", "Power data"),
    ("system_profiler SPNVMeDataType 2>/dev/null | head -20", "NVMe data"),
    ("system_profiler SPNetworkDataType 2>/dev/null | head -30", "Network config"),
    ("system_profiler SPThunderboltDataType 2>/dev/null | head -20", "Thunderbolt state"),
    
    # IOKit (the goldmine)
    ("/usr/sbin/ioreg -l -w0 -c IOPlatformDevice 2>/dev/null | grep -i 'temperature\\|voltage\\|current\\|power\\|sensor' | head -30", "IOKit sensor properties"),
    ("/usr/sbin/ioreg -l -w0 2>/dev/null | grep -c '\".*\" ='", "Total IORegistry properties"),
    ("/usr/sbin/ioreg -l -w0 -c AppleARMIODevice 2>/dev/null | head -20", "ARM IO devices"),
    
    # Memory pressure
    ("memory_pressure 2>/dev/null | head -5", "Memory pressure state"),
    ("vm_stat 2>/dev/null", "VM statistics (page faults, etc)"),
    
    # Launchd / system state
    ("launchctl list 2>/dev/null | wc -l", "Launchd job count"),
    
    # DNS/mDNS
    ("scutil --dns 2>/dev/null | head -20", "DNS resolver config"),
    ("dscacheutil -statistics 2>/dev/null", "Directory service cache stats"),
    
    # Thermal/power (no sudo)
    ("pmset -g thermlog 2>/dev/null | head -10", "Thermal throttle log"),
    ("pmset -g 2>/dev/null", "Power management state"),
    
    # USB devices
    ("system_profiler SPUSBDataType 2>/dev/null | head -30", "USB device tree"),
    
    # Thread/timing
    ("date +%s%N 2>/dev/null || python3 -c 'import time; print(time.time_ns())'", "Nanosecond timestamp"),
    
    # Filesystem entropy
    ("ls -lai /dev/ 2>/dev/null | wc -l", "Device node count"),
    ("cat /dev/urandom 2>/dev/null | head -c 8 | xxd -p", "/dev/urandom sample (baseline)"),
]

results = []
for cmd, label in sources:
    sys.stdout.write(f"  Probing: {label}... ")
    sys.stdout.flush()
    r = sample_and_diff(cmd, label, n=5, delay=0.2)
    if r:
        status = "CHANGES" if r['samples_that_changed'] > 0 else "static"
        print(f"{status} ({r['samples_that_changed']}/{r['total_samples']} differ, {r['output_bytes']} bytes)")
        results.append(r)
    else:
        print("UNAVAILABLE")

print(f"\n{'='*60}")
print("ENTROPY SOURCE CANDIDATES (data that changes between samples):")
print(f"{'='*60}")
changing = [r for r in results if r['samples_that_changed'] > 0]
static = [r for r in results if r['samples_that_changed'] == 0]

changing.sort(key=lambda x: x['samples_that_changed'], reverse=True)
for r in changing:
    print(f"  🔥 {r['label']}")
    print(f"     Changed: {r['samples_that_changed']}/{r['total_samples']} samples, {r['output_bytes']} bytes")
    print(f"     Cmd: {r['cmd'][:80]}")

print(f"\nStatic sources (no changes detected): {len(static)}")
print(f"Dynamic sources (entropy candidates): {len(changing)}")

# Save results
outfile = 'docs/findings/overlooked_sources.json'
os.makedirs(os.path.dirname(outfile), exist_ok=True)
with open(outfile, 'w') as f:
    json.dump({'changing': changing, 'static': static}, f, indent=2)
print(f"\nSaved to {outfile}")
