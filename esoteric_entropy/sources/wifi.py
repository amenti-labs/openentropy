"""WiFi RSSI entropy source — RF field measurement via WiFi radio.

WiFi signal strength fluctuates due to multipath fading, environmental
reflections, RF interference, atmospheric effects, and human movement.
The micro-variations in RSSI are genuine environmental randomness from
the electromagnetic field.

Supports multiple macOS methods:
1. CoreWLAN via pyobjc (best — gives RSSI + noise + channel)
2. wdutil (modern macOS replacement for airport)
3. networksetup (basic but reliable)
4. airport (deprecated but still works on some systems)
"""

from __future__ import annotations

import platform
import re
import subprocess
import time

import numpy as np

from esoteric_entropy.sources.base import EntropySource


def _get_rssi_corewlan() -> dict | None:
    """Get WiFi RSSI, noise, and channel via CoreWLAN framework."""
    try:
        import objc
        bundle = {}
        objc.loadBundle(
            'CoreWLAN',
            bundle_path='/System/Library/Frameworks/CoreWLAN.framework',
            module_globals=bundle,
        )
        CWWiFiClient = objc.lookUpClass('CWWiFiClient')
        client = CWWiFiClient.sharedWiFiClient()
        iface = client.interface()
        if iface is None:
            return None
        rssi = iface.rssiValue()
        noise = iface.noiseMeasurement()
        channel = iface.wlanChannel()
        ch_num = channel.channelNumber() if channel else 0
        return {'rssi': int(rssi), 'noise': int(noise), 'channel': int(ch_num)}
    except Exception:
        return None


def _get_rssi_wdutil() -> dict | None:
    """Get WiFi RSSI via wdutil (modern macOS)."""
    try:
        r = subprocess.run(
            ["/usr/bin/wdutil", "info"],
            capture_output=True, text=True, timeout=5,
        )
        rssi_match = re.search(r'RSSI\s*:\s*(-?\d+)', r.stdout)
        noise_match = re.search(r'Noise\s*:\s*(-?\d+)', r.stdout)
        channel_match = re.search(r'Channel\s*:\s*(\d+)', r.stdout)
        if rssi_match:
            return {
                'rssi': int(rssi_match.group(1)),
                'noise': int(noise_match.group(1)) if noise_match else 0,
                'channel': int(channel_match.group(1)) if channel_match else 0,
            }
    except (FileNotFoundError, OSError, subprocess.TimeoutExpired):
        pass
    return None


def _get_rssi_networksetup() -> dict | None:
    """Get basic WiFi info via networksetup."""
    # networksetup doesn't give RSSI directly, but we can check connection
    try:
        # Try to find the WiFi interface
        r = subprocess.run(
            ["/usr/sbin/networksetup", "-listallhardwareports"],
            capture_output=True, text=True, timeout=5,
        )
        wifi_device = None
        lines = r.stdout.split('\n')
        for i, line in enumerate(lines):
            if 'Wi-Fi' in line:
                for j in range(i + 1, min(i + 3, len(lines))):
                    m = re.match(r'Device:\s*(\w+)', lines[j])
                    if m:
                        wifi_device = m.group(1)
                        break
        if not wifi_device:
            return None

        # Use ipconfig for more details
        r2 = subprocess.run(
            ["/usr/sbin/ipconfig", "getsummary", wifi_device],
            capture_output=True, text=True, timeout=5,
        )
        rssi_match = re.search(r'RSSI\s*:\s*(-?\d+)', r2.stdout)
        if rssi_match:
            return {'rssi': int(rssi_match.group(1)), 'noise': 0, 'channel': 0}
    except (FileNotFoundError, OSError, subprocess.TimeoutExpired):
        pass
    return None


def _get_rssi_airport() -> dict | None:
    """Get WiFi RSSI via deprecated airport command."""
    try:
        r = subprocess.run(
            ["/System/Library/PrivateFrameworks/Apple80211.framework/"
             "Versions/Current/Resources/airport", "-I"],
            capture_output=True, text=True, timeout=5,
        )
        rssi_match = re.search(r'agrCtlRSSI:\s*(-?\d+)', r.stdout)
        noise_match = re.search(r'agrCtlNoise:\s*(-?\d+)', r.stdout)
        channel_match = re.search(r'channel:\s*(\d+)', r.stdout)
        if rssi_match:
            return {
                'rssi': int(rssi_match.group(1)),
                'noise': int(noise_match.group(1)) if noise_match else 0,
                'channel': int(channel_match.group(1)) if channel_match else 0,
            }
    except (FileNotFoundError, OSError, subprocess.TimeoutExpired):
        pass
    return None


def _scan_wifi_networks() -> list[dict]:
    """Scan for all visible WiFi networks and their RSSI values."""
    try:
        import objc
        bundle = {}
        objc.loadBundle(
            'CoreWLAN',
            bundle_path='/System/Library/Frameworks/CoreWLAN.framework',
            module_globals=bundle,
        )
        CWWiFiClient = objc.lookUpClass('CWWiFiClient')
        client = CWWiFiClient.sharedWiFiClient()
        iface = client.interface()
        if iface is None:
            return []

        networks, error = iface.scanForNetworksWithName_error_(None, None)
        if error or not networks:
            return []

        results = []
        for net in networks:
            results.append({
                'rssi': int(net.rssiValue()),
                'noise': int(net.noiseMeasurement()),
                'channel': int(net.wlanChannel().channelNumber()) if net.wlanChannel() else 0,
            })
        return results
    except Exception:
        return []


# Method priority: try each until one works
_METHODS = [
    ("corewlan", _get_rssi_corewlan),
    ("wdutil", _get_rssi_wdutil),
    ("networksetup", _get_rssi_networksetup),
    ("airport", _get_rssi_airport),
]


class WiFiRSSISource(EntropySource):
    """Entropy from WiFi RSSI fluctuations — direct RF field measurement.

    The WiFi radio measures the electromagnetic field at 2.4/5/6 GHz.
    Signal strength fluctuates due to:
    - Multipath fading (constructive/destructive interference of reflected waves)
    - Human and object movement through the RF path
    - Interference from other WiFi networks, Bluetooth, microwaves
    - Atmospheric and thermal effects on propagation
    - Frequency-selective fading across channels

    This is one of the most genuinely field-based entropy sources —
    the WiFi radio is essentially an RF spectrum analyzer.
    """

    name = "wifi_rssi"
    description = "WiFi RSSI fluctuations (RF field measurement at 2.4/5/6 GHz)"
    platform_requirements = ["darwin", "wifi"]
    entropy_rate_estimate = 30.0

    def __init__(self) -> None:
        self._method: str = "none"
        self._method_fn = None

    def is_available(self) -> bool:
        if platform.system() != "Darwin":
            return False

        # Check WiFi hardware exists
        try:
            r = subprocess.run(
                ["/usr/sbin/networksetup", "-listallhardwareports"],
                capture_output=True, text=True, timeout=5,
            )
            if "Wi-Fi" not in r.stdout:
                return False
        except (FileNotFoundError, OSError, subprocess.TimeoutExpired):
            return False

        # Try each method
        for name, fn in _METHODS:
            try:
                result = fn()
                if result and result.get('rssi') is not None:
                    self._method = name
                    self._method_fn = fn
                    return True
            except Exception:
                continue

        return False

    def collect(self, n_samples: int = 100) -> np.ndarray:
        """Collect RSSI samples by rapid repeated measurement."""
        if self._method_fn is None:
            return np.array([], dtype=np.uint8)

        rssi_values: list[int] = []
        noise_values: list[int] = []
        timings: list[int] = []

        for _ in range(n_samples):
            t0 = time.perf_counter_ns()
            try:
                result = self._method_fn()
                if result:
                    rssi_values.append(result['rssi'])
                    if result.get('noise'):
                        noise_values.append(result['noise'])
            except Exception:
                pass
            timings.append(time.perf_counter_ns() - t0)
            # Small delay to allow RF conditions to change
            time.sleep(0.01)

        if not rssi_values:
            return np.array([], dtype=np.uint8)

        # Combine multiple signals for maximum entropy:
        parts = []

        # 1. RSSI LSBs — the most volatile bits of signal strength
        arr = np.array(rssi_values, dtype=np.int64)
        parts.append((arr & 0xFF).astype(np.uint8))

        # 2. RSSI deltas — changes are more entropic than raw values
        if len(arr) > 1:
            deltas = np.diff(arr)
            parts.append((deltas & 0xFF).astype(np.uint8))

        # 3. Noise floor LSBs (if available)
        if noise_values:
            narr = np.array(noise_values, dtype=np.int64)
            parts.append((narr & 0xFF).astype(np.uint8))

        # 4. Timing jitter of the measurement itself
        tarr = np.array(timings, dtype=np.int64)
        parts.append((tarr & 0xFF).astype(np.uint8))

        combined = np.concatenate(parts)
        if len(combined) >= n_samples:
            return combined[:n_samples]
        return np.resize(combined, n_samples)

    def collect_scan(self, n_scans: int = 3) -> np.ndarray:
        """Collect entropy from full WiFi network scans.

        More expensive but captures RSSI from ALL visible networks,
        giving many more independent RF field measurements per scan.
        """
        all_rssi: list[int] = []
        for _ in range(n_scans):
            networks = _scan_wifi_networks()
            for net in networks:
                all_rssi.append(net['rssi'])
                if net.get('noise'):
                    all_rssi.append(net['noise'])
            time.sleep(0.5)

        if not all_rssi:
            return np.array([], dtype=np.uint8)

        arr = np.array(all_rssi, dtype=np.int64)
        return (arr & 0xFF).astype(np.uint8)

    def entropy_quality(self) -> dict:
        data = self.collect(200)
        return self._quick_quality(data, self.name)
