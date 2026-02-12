"""Bluetooth LE advertisement RSSI noise entropy source.

Uses CoreBluetooth via pyobjc to perform actual BLE scanning and harvest
RSSI values from ambient advertising devices. Each nearby BLE device
(phones, watches, beacons, headphones) broadcasts at intervals with RSSI
that fluctuates due to multipath fading, movement, and RF interference.

Falls back to system_profiler connected device RSSI if CoreBluetooth
is unavailable (e.g., no pyobjc or permission denied).
"""

from __future__ import annotations

import platform
import subprocess
import re
import time
import threading

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _scan_ble_corewlan(duration: float = 5.0) -> list[int]:
    """Scan BLE advertisements via CoreBluetooth and collect RSSI values.
    
    Returns list of RSSI integers from discovered peripherals.
    Requires pyobjc-framework-CoreBluetooth.
    """
    try:
        import objc
        from Foundation import NSRunLoop, NSDate, NSObject
        from dispatch import dispatch_queue_create
    except ImportError:
        raise ImportError("pyobjc not available")

    # Load CoreBluetooth
    cb_bundle = {}
    objc.loadBundle(
        'CoreBluetooth',
        bundle_path='/System/Library/Frameworks/CoreBluetooth.framework',
        module_globals=cb_bundle,
    )
    CBCentralManager = objc.lookUpClass('CBCentralManager')

    rssi_values: list[int] = []
    scan_done = threading.Event()

    class BLEDelegate(NSObject):
        def init(self):
            self = objc.super(BLEDelegate, self).init()
            return self

        def centralManagerDidUpdateState_(self, central):
            # State 5 = PoweredOn
            if central.state() == 5:
                central.scanForPeripheralsWithServices_options_(None, None)
            elif central.state() == 4:
                # PoweredOff
                scan_done.set()

        def centralManager_didDiscoverPeripheral_advertisementData_RSSI_(
            self, central, peripheral, ad_data, rssi
        ):
            rssi_values.append(int(rssi))

    delegate = BLEDelegate.alloc().init()
    queue = dispatch_queue_create(b"ble_entropy", None)
    manager = CBCentralManager.alloc().initWithDelegate_queue_(delegate, queue)

    # Run the event loop for the scan duration
    end_time = time.time() + duration
    while time.time() < end_time and not scan_done.is_set():
        NSRunLoop.currentRunLoop().runUntilDate_(
            NSDate.dateWithTimeIntervalSinceNow_(0.1)
        )

    try:
        manager.stopScan()
    except Exception:
        pass

    return rssi_values


def _scan_ble_system_profiler() -> list[int]:
    """Fallback: extract RSSI from connected BT devices via system_profiler."""
    try:
        r = subprocess.run(
            ["/usr/sbin/system_profiler", "SPBluetoothDataType"],
            capture_output=True, text=True, timeout=10,
        )
        rssi_values = []
        for match in re.finditer(r'RSSI:\s*(-?\d+)', r.stdout):
            rssi_values.append(int(match.group(1)))
        return rssi_values
    except (OSError, subprocess.TimeoutExpired):
        return []


def _scan_ble_blueutil() -> list[int]:
    """Fallback: use blueutil CLI if installed."""
    try:
        r = subprocess.run(
            ["blueutil", "--inquiry", "3"],
            capture_output=True, text=True, timeout=10,
        )
        rssi_values = []
        for match in re.finditer(r'RSSI:\s*(-?\d+)', r.stdout):
            rssi_values.append(int(match.group(1)))
        return rssi_values
    except (FileNotFoundError, OSError, subprocess.TimeoutExpired):
        return []


class BluetoothNoiseSource(EntropySource):
    """Entropy from BLE advertisement RSSI — actual RF field measurement.

    Each nearby BLE device's signal strength fluctuates due to:
    - Multipath fading (reflections off walls, furniture, people)
    - Frequency hopping across 37 advertising channels
    - Device movement and orientation changes
    - Environmental RF interference

    This is genuine electromagnetic field measurement — the Bluetooth
    radio is essentially acting as an RF sensor.
    """

    name = "bluetooth_ble"
    description = "BLE advertisement RSSI noise (RF field measurement)"
    platform_requirements = ["darwin", "bluetooth"]
    entropy_rate_estimate = 50.0

    def __init__(self) -> None:
        self._method: str = "none"

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False
        try:
            r = subprocess.run(
                ["/usr/sbin/system_profiler", "SPBluetoothDataType"],
                capture_output=True, text=True, timeout=10,
            )
            if "Bluetooth" not in r.stdout:
                return False

            # Try CoreBluetooth first
            try:
                import objc  # noqa: F401
                self._method = "corebluetooth"
                return True
            except ImportError:
                pass

            # Try blueutil
            try:
                subprocess.run(["blueutil", "--version"],
                              capture_output=True, timeout=3)
                self._method = "blueutil"
                return True
            except (FileNotFoundError, OSError):
                pass

            # Fall back to system_profiler RSSI parsing
            if re.search(r'RSSI:\s*-?\d+', r.stdout):
                self._method = "system_profiler"
                return True

            # Last resort: timing-based (not ideal but still some entropy)
            self._method = "timing"
            return True

        except (OSError, subprocess.TimeoutExpired):
            return False

    def collect(self, n_samples: int = 100) -> np.ndarray:
        """Collect RSSI samples from BLE environment."""
        if self._method == "corebluetooth":
            return self._collect_corebluetooth(n_samples)
        elif self._method == "blueutil":
            return self._collect_blueutil(n_samples)
        elif self._method == "system_profiler":
            return self._collect_system_profiler(n_samples)
        else:
            return self._collect_timing(n_samples)

    def _collect_corebluetooth(self, n_samples: int) -> np.ndarray:
        """Real BLE scanning via CoreBluetooth — best method."""
        # Scan for longer with more samples
        duration = min(max(n_samples / 20, 3.0), 30.0)
        try:
            rssi_values = _scan_ble_corewlan(duration)
        except Exception:
            # Fall back to timing
            return self._collect_timing(n_samples)

        if len(rssi_values) < 10:
            return self._collect_timing(n_samples)

        # Convert RSSI to uint8 — RSSI typically ranges -30 to -100
        # Map to 0-255 range and extract LSBs for maximum entropy
        arr = np.array(rssi_values, dtype=np.int64)

        # Method 1: Use raw RSSI values mod 256
        raw_bytes = (arr & 0xFF).astype(np.uint8)

        # Method 2: Use deltas between successive RSSI readings
        if len(arr) > 1:
            deltas = np.diff(arr)
            delta_bytes = (deltas & 0xFF).astype(np.uint8)
            combined = np.concatenate([raw_bytes, delta_bytes])
        else:
            combined = raw_bytes

        # Pad or truncate to requested size
        if len(combined) >= n_samples:
            return combined[:n_samples]
        # Repeat with more scanning if needed
        return np.resize(combined, n_samples)

    def _collect_blueutil(self, n_samples: int) -> np.ndarray:
        """BLE scanning via blueutil CLI."""
        all_rssi: list[int] = []
        attempts = max(n_samples // 10, 3)
        for _ in range(attempts):
            all_rssi.extend(_scan_ble_blueutil())
            if len(all_rssi) >= n_samples:
                break

        if len(all_rssi) < 5:
            return self._collect_timing(n_samples)

        arr = np.array(all_rssi, dtype=np.int64)
        combined = (arr & 0xFF).astype(np.uint8)
        return np.resize(combined, n_samples)

    def _collect_system_profiler(self, n_samples: int) -> np.ndarray:
        """Extract RSSI from system_profiler output — uses connected devices."""
        all_rssi: list[int] = []
        timings: list[int] = []

        for _ in range(max(n_samples // 5, 20)):
            t0 = time.perf_counter_ns()
            rssi_values = _scan_ble_system_profiler()
            t1 = time.perf_counter_ns()
            all_rssi.extend(rssi_values)
            timings.append(t1 - t0)
            time.sleep(0.01)

        # Combine RSSI values AND timing jitter
        parts = []
        if all_rssi:
            arr = np.array(all_rssi, dtype=np.int64)
            parts.append((arr & 0xFF).astype(np.uint8))
        if timings:
            tarr = np.array(timings, dtype=np.int64)
            parts.append((tarr & 0xFF).astype(np.uint8))

        if not parts:
            return self._collect_timing(n_samples)

        combined = np.concatenate(parts)
        return np.resize(combined, n_samples)

    def _collect_timing(self, n_samples: int) -> np.ndarray:
        """Last resort: BT subsystem query timing jitter."""
        timings: list[int] = []
        for _ in range(n_samples):
            t0 = time.perf_counter_ns()
            try:
                subprocess.run(
                    ["/usr/sbin/system_profiler", "SPBluetoothDataType"],
                    capture_output=True, timeout=5,
                )
            except (OSError, subprocess.TimeoutExpired):
                pass
            timings.append(time.perf_counter_ns() - t0)
        return (np.array(timings, dtype=np.int64) & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(min(200, 100))
        return self._quick_quality(data, self.name)
