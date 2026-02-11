#!/usr/bin/env python3
"""
BLE Ambient Noise — entropy from Bluetooth LE advertisements.

Every BLE device nearby is an independent entropy source.
RSSI fluctuates with multipath/interference, advertising intervals
have clock drift jitter, and channel hopping adds randomness.
"""
import subprocess
import time
import hashlib
import re
import json
import numpy as np
from collections import defaultdict

def scan_ble_system_profiler():
    """Get BLE device info via system_profiler."""
    print("[BLE] Scanning via system_profiler...")
    try:
        result = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPBluetoothDataType', '-json'],
            capture_output=True, text=True, timeout=15
        )
        data = json.loads(result.stdout)
        return data
    except Exception as e:
        print(f"  Error: {e}")
        return None

def scan_ble_via_blueutil():
    """Try blueutil for BLE scanning."""
    try:
        result = subprocess.run(['blueutil', '--inquiry', '5'],
                              capture_output=True, text=True, timeout=15)
        return result.stdout
    except FileNotFoundError:
        return None

def scan_ble_pyobjc(duration_s=5.0):
    """Scan for BLE devices using CoreBluetooth via pyobjc."""
    print(f"[BLE] CoreBluetooth scan for {duration_s}s...")
    try:
        import objc
        from CoreBluetooth import (
            CBCentralManager, CBCentralManagerStatePoweredOn,
        )
        from Foundation import NSObject, NSRunLoop, NSDate
        from PyObjCTools import AppHelper
        
        results = []
        
        class BLEDelegate(NSObject):
            def init(self):
                self = objc.super(BLEDelegate, self).init()
                self.powered_on = False
                return self
            
            def centralManagerDidUpdateState_(self, manager):
                if manager.state() == CBCentralManagerStatePoweredOn:
                    self.powered_on = True
                    manager.scanForPeripheralsWithServices_options_(None, {
                        'CBCentralManagerScanOptionAllowDuplicatesKey': True
                    })
            
            def centralManager_didDiscoverPeripheral_advertisementData_RSSI_(
                self, manager, peripheral, ad_data, rssi):
                results.append({
                    'rssi': int(rssi),
                    'time': time.perf_counter_ns(),
                    'name': str(peripheral.name()) if peripheral.name() else 'unknown',
                })
        
        delegate = BLEDelegate.alloc().init()
        manager = CBCentralManager.alloc().initWithDelegate_queue_(delegate, None)
        
        # Run the event loop
        end_time = time.time() + duration_s
        while time.time() < end_time:
            NSRunLoop.currentRunLoop().runUntilDate_(
                NSDate.dateWithTimeIntervalSinceNow_(0.1))
        
        manager.stopScan()
        return results
        
    except ImportError as e:
        print(f"  CoreBluetooth import failed: {e}")
        return []
    except Exception as e:
        print(f"  CoreBluetooth error: {e}")
        return []

def extract_ble_entropy(ble_results):
    """Extract entropy from BLE scan results."""
    if not ble_results:
        return np.array([], dtype=np.uint8)
    
    entropy_bytes = bytearray()
    
    # RSSI values
    rssi_values = [r['rssi'] for r in ble_results]
    if rssi_values:
        # RSSI LSBs
        rssi_arr = np.array(rssi_values, dtype=np.int32)
        lsbs = (rssi_arr & 0xFF).astype(np.uint8)
        entropy_bytes.extend(lsbs.tobytes())
    
    # Timing jitter between advertisements
    times = [r['time'] for r in ble_results]
    if len(times) > 1:
        deltas = np.diff(times)
        delta_lsbs = (deltas.astype(np.uint64) & 0xFF).astype(np.uint8)
        entropy_bytes.extend(delta_lsbs.tobytes())
    
    # Per-device RSSI fluctuation
    by_device = defaultdict(list)
    for r in ble_results:
        by_device[r['name']].append(r['rssi'])
    
    for name, rssis in by_device.items():
        if len(rssis) > 5:
            arr = np.array(rssis)
            noise = arr - np.mean(arr)
            if np.std(noise) > 0:
                norm = ((noise - noise.min()) / (noise.max() - noise.min() + 1e-30) * 255).astype(np.uint8)
                entropy_bytes.extend(norm.tobytes())
    
    return np.frombuffer(bytes(entropy_bytes), dtype=np.uint8)

def run(output_file='explore/entropy_ble_noise.bin'):
    print("=" * 60)
    print("BLE AMBIENT NOISE — Bluetooth LE Environmental Entropy")
    print("=" * 60)
    
    all_entropy = bytearray()
    
    # Check BT state
    print("\n[Phase 1] Checking Bluetooth state...")
    bt_info = scan_ble_system_profiler()
    if bt_info:
        print("  Bluetooth info retrieved")
    
    # CoreBluetooth scan
    print("\n[Phase 2] CoreBluetooth BLE scan...")
    ble_results = scan_ble_pyobjc(duration_s=8.0)
    
    if ble_results:
        # Analyze
        unique_devices = len(set(r['name'] for r in ble_results))
        rssi_values = [r['rssi'] for r in ble_results]
        print(f"  Discovered {len(ble_results)} advertisements from {unique_devices} devices")
        print(f"  RSSI range: {min(rssi_values)} to {max(rssi_values)} dBm")
        print(f"  RSSI std: {np.std(rssi_values):.2f}")
        
        ent = extract_ble_entropy(ble_results)
        all_entropy.extend(ent.tobytes())
    else:
        print("  No BLE devices found (or BLE not available)")
        print("  This may require running from a GUI context for BLE permissions")
    
    # Also harvest timing entropy from the BT query itself
    print("\n[Phase 3] BT query timing jitter...")
    timings = []
    for i in range(50):
        start = time.perf_counter_ns()
        subprocess.run(['/usr/sbin/system_profiler', 'SPBluetoothDataType'],
                      capture_output=True, timeout=10)
        elapsed = time.perf_counter_ns() - start
        timings.append(elapsed)
    
    if timings:
        arr = np.array(timings, dtype=np.uint64)
        lsbs = (arr & 0xFF).astype(np.uint8)
        all_entropy.extend(lsbs.tobytes())
        print(f"  BT query timing: mean={np.mean(arr)/1e6:.1f}ms, std={np.std(arr)/1e6:.1f}ms")
    
    if not all_entropy:
        print("[FAIL] No entropy collected")
        return None
    
    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))
    
    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes")
    print(f"  SHA256: {sha[:32]}...")
    
    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")
    
    return {'total_bytes': len(all_entropy), 'sha256': sha}

if __name__ == '__main__':
    run()
