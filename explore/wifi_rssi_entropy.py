#!/usr/bin/env python3
"""
WiFi RSSI Entropy — harvest entropy from WiFi signal fluctuations via CoreWLAN.
"""
import time
import hashlib
import numpy as np


def _get_wifi_interface():
    """Get CoreWLAN WiFi interface."""
    import objc
    _g = {}
    objc.loadBundle('CoreWLAN',
        bundle_path='/System/Library/Frameworks/CoreWLAN.framework',
        module_globals=_g)
    CWWiFiClient = objc.lookUpClass('CWWiFiClient')
    client = CWWiFiClient.sharedWiFiClient()
    return client.interface()


def run(output_file='explore/entropy_wifi_rssi.bin'):
    print("=" * 60)
    print("WIFI RSSI ENTROPY — Signal Fluctuation Harvester")
    print("=" * 60)

    try:
        iface = _get_wifi_interface()
    except Exception as e:
        print(f"[FAIL] Cannot load CoreWLAN: {e}")
        return None

    if iface is None:
        print("[FAIL] No WiFi interface available")
        return None

    r = int(iface.rssiValue())
    n = int(iface.noiseMeasurement())
    print(f"[WiFi] Connected: RSSI={r}dBm, Noise={n}dBm")

    # Collect samples
    n_samples = 200
    interval = 0.05
    print(f"\n[Phase 1] Collecting {n_samples} samples at {1/interval:.0f}Hz...")

    rssi_vals = []
    noise_vals = []
    tx_rate_vals = []
    timings = []

    for i in range(n_samples):
        t0 = time.perf_counter_ns()
        rssi_vals.append(int(iface.rssiValue()))
        noise_vals.append(int(iface.noiseMeasurement()))
        tx_rate_vals.append(float(iface.transmitRate()))
        t1 = time.perf_counter_ns()
        timings.append(t1 - t0)
        time.sleep(interval)
        if (i + 1) % 100 == 0:
            print(f"  {i+1}/{n_samples}...")

    all_entropy = bytearray()
    streams = {}

    # Process each signal source
    for label, vals in [('rssi', rssi_vals), ('noise', noise_vals), ('tx_rate', tx_rate_vals)]:
        arr = np.array(vals, dtype=np.float64)
        unique = len(set(vals))
        print(f"  [{label}] Samples: {len(vals)}, Unique: {unique}, "
              f"Mean: {arr.mean():.1f}, Std: {arr.std():.2f}")
        if unique <= 1:
            continue
        # Detrend and extract
        detrended = arr - np.convolve(arr, np.ones(5)/5, mode='same')
        noise = detrended[2:-2]
        if noise.std() == 0:
            continue
        normalized = (noise - noise.min()) / (noise.max() - noise.min() + 1e-30)
        quantized = (normalized * 255).astype(np.uint8)
        all_entropy.extend(quantized.tobytes())
        streams[label] = len(quantized)

    # Timing LSBs — the call timing itself contains entropy
    t_arr = np.array(timings, dtype=np.uint64)
    lsbs = (t_arr & 0xFF).astype(np.uint8)
    all_entropy.extend(lsbs.tobytes())
    streams['timing'] = len(lsbs)
    print(f"  [timing] Unique LSBs: {len(set(lsbs))}/256, Mean: {t_arr.mean():.0f}ns")

    if not all_entropy:
        print("[FAIL] No entropy collected")
        return None

    with open(output_file, 'wb') as f:
        f.write(bytes(all_entropy))

    sha = hashlib.sha256(bytes(all_entropy)).hexdigest()
    print(f"\n[RESULT] Collected {len(all_entropy)} entropy bytes from {len(streams)} streams")
    print(f"  Streams: {streams}")
    print(f"  SHA256: {sha[:32]}...")

    import zlib
    if len(all_entropy) > 100:
        ratio = len(zlib.compress(bytes(all_entropy))) / len(all_entropy)
        print(f"  Compression ratio: {ratio:.3f}")

    return {'total_bytes': len(all_entropy), 'sha256': sha, 'streams': streams}


if __name__ == '__main__':
    run()
