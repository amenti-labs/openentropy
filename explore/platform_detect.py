#!/usr/bin/env python3
"""
Platform detection for esoteric-entropy.

Detects which entropy sources are available on the current hardware.
Mac Mini vs MacBook vs Linux — different sensors, different capabilities.
"""
import subprocess
import platform
import os
import json
from pathlib import Path

def _sysctl(key):
    """Read a sysctl key, trying full path if needed."""
    for cmd in [['/usr/sbin/sysctl', '-n', key], ['/usr/sbin/sysctl', '-n', key]]:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            if result.returncode == 0 and result.stdout.strip():
                return result.stdout.strip()
        except Exception:
            continue
    return None

def get_mac_model():
    """Return Mac model identifier (e.g., 'Mac16,10', 'MacBookPro18,1')."""
    return _sysctl('hw.model')

def get_chip():
    """Return chip name (e.g., 'Apple M1', 'Apple M4')."""
    return _sysctl('machdep.cpu.brand_string')

def is_mac_mini():
    model = get_mac_model() or ''
    return 'Macmini' in model or 'Mac mini' in model or (model and model.startswith('Mac') and not 'MacBook' in model and not 'iMac' in model and not 'MacPro' in model)

def is_macbook():
    model = get_mac_model() or ''
    return 'MacBook' in model

def is_imac():
    model = get_mac_model() or ''
    return 'iMac' in model

def has_camera():
    """Check for built-in camera."""
    try:
        result = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPCameraDataType'], capture_output=True, text=True
        )
        return 'FaceTime' in result.stdout or 'Camera' in result.stdout
    except Exception:
        return False

def has_microphone():
    """Check for audio input (built-in mic or external)."""
    try:
        result = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPAudioDataType'], capture_output=True, text=True
        )
        return 'Input' in result.stdout or 'Microphone' in result.stdout
    except Exception:
        return False

def has_battery():
    """Check for battery (MacBooks only)."""
    try:
        result = subprocess.run(
            ['pmset', '-g', 'batt'], capture_output=True, text=True
        )
        return 'Battery' in result.stdout or 'InternalBattery' in result.stdout
    except Exception:
        return False

def has_trackpad():
    """Check for built-in trackpad."""
    try:
        result = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPUSBDataType'], capture_output=True, text=True
        )
        has_builtin = 'Trackpad' in result.stdout
        # Also check Bluetooth for Magic Trackpad
        result2 = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPBluetoothDataType'], capture_output=True, text=True
        )
        has_bt = 'Trackpad' in result2.stdout
        return has_builtin or has_bt
    except Exception:
        return False

def has_magnetometer():
    """Check for magnetometer (some MacBooks only)."""
    try:
        result = subprocess.run(
            ['/usr/sbin/ioreg', '-l', '-w0'], capture_output=True, text=True
        )
        return 'Magnetometer' in result.stdout or 'compass' in result.stdout.lower()
    except Exception:
        return False

def has_ambient_light_sensor():
    """Check for ambient light sensor."""
    try:
        result = subprocess.run(
            ['/usr/sbin/ioreg', '-l', '-w0', '-n', 'AppleHIDKeyboardEventDriverV2'],
            capture_output=True, text=True
        )
        return 'ALSSensor' in result.stdout or 'AmbientLight' in result.stdout
    except Exception:
        return False

def has_motion_sensors():
    """Check for accelerometer/gyroscope (MacBooks with SMS/SuddenMotionSensor)."""
    try:
        result = subprocess.run(
            ['/usr/sbin/ioreg', '-l', '-w0'], capture_output=True, text=True
        )
        return 'SMCMotionSensor' in result.stdout or 'Accelerometer' in result.stdout
    except Exception:
        return False

def has_bluetooth():
    """Check for Bluetooth."""
    try:
        result = subprocess.run(
            ['/usr/sbin/system_profiler', 'SPBluetoothDataType'], capture_output=True, text=True
        )
        return 'Bluetooth' in result.stdout
    except Exception:
        return False

def has_wifi():
    """Check for WiFi hardware."""
    for cmd in [
        ['/usr/sbin/networksetup', '-listallhardwareports'],
        ['networksetup', '-listallhardwareports'],
    ]:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            if 'Wi-Fi' in result.stdout:
                return True
        except Exception:
            continue
    return False

def has_sudo():
    """Check if we can run sudo without password."""
    for cmd in [['/usr/bin/sudo', '-n', 'true'], ['/usr/bin/sudo', '-n', 'true']]:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=3)
            if result.returncode == 0:
                return True
        except Exception:
            continue
    return False

def has_smartctl():
    """Check for smartmontools."""
    try:
        result = subprocess.run(['which', 'smartctl'], capture_output=True, text=True)
        return result.returncode == 0
    except Exception:
        return False

def has_imagesnap():
    """Check for imagesnap (camera capture CLI)."""
    try:
        result = subprocess.run(['which', 'imagesnap'], capture_output=True, text=True)
        return result.returncode == 0
    except Exception:
        return False


# ── Source availability matrix ──

ENTROPY_SOURCES = {
    # Source ID: (name, description, requires)
    'smc_sensors': {
        'name': 'SMC Sensor Galaxy',
        'description': 'Hundreds of voltage, current, power, temp ADC readings',
        'requires': ['macos'],
        'best_with': ['/usr/bin/sudo'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'audio_thermal': {
        'name': 'Audio Thermal Noise',
        'description': 'Johnson-Nyquist noise from audio ADC',
        'requires': ['microphone'],
        'available_on': ['macbook', 'imac'],  # Mac Mini needs external mic
        'note_mac_mini': 'Requires external audio input device',
    },
    'audio_emi': {
        'name': 'Audio EMI Coupling',
        'description': 'Electromagnetic interference from CPU/GPU via audio ADC',
        'requires': ['microphone'],
        'available_on': ['macbook', 'imac'],
        'note_mac_mini': 'Requires external audio input device',
    },
    'wifi_rssi': {
        'name': 'WiFi RSSI Fluctuations',
        'description': 'Signal strength micro-variations from multipath fading',
        'requires': ['wifi'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'ble_ambient': {
        'name': 'BLE Advertisement Noise',
        'description': 'RSSI and timing from ambient Bluetooth devices',
        'requires': ['bluetooth'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'gpu_timing': {
        'name': 'GPU Compute Timing Jitter',
        'description': 'Metal/GPU dispatch completion time variance',
        'requires': ['macos'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'clock_jitter': {
        'name': 'Clock/Timing Jitter',
        'description': 'High-resolution clock call and sleep-wake jitter',
        'requires': [],
        'available_on': ['mac_mini', 'macbook', 'imac', 'linux'],
    },
    'mach_timing': {
        'name': 'Mach Kernel Timing',
        'description': 'Mach absolute time, port ops, page fault timing',
        'requires': ['macos'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'nvme_jitter': {
        'name': 'NVMe I/O & SMART Jitter',
        'description': 'Storage timing and SMART attribute fluctuations',
        'requires': [],
        'best_with': ['/usr/bin/sudo', 'smartctl'],
        'available_on': ['mac_mini', 'macbook', 'imac', 'linux'],
    },
    'camera_quantum': {
        'name': 'Camera Photon Shot Noise',
        'description': 'Quantum Poisson noise from image sensor',
        'requires': ['camera'],
        'available_on': ['macbook', 'imac'],
        'note_mac_mini': 'Requires external USB camera',
    },
    'trackpad_capacitance': {
        'name': 'Trackpad Capacitive Noise',
        'description': 'EM noise from capacitive sensor array',
        'requires': ['trackpad'],
        'available_on': ['macbook'],
        'note_mac_mini': 'Requires Magic Trackpad connected via BT/USB',
    },
    'magnetometer': {
        'name': 'Magnetometer Geomagnetic Noise',
        'description': 'Geomagnetic micro-fluctuations and Schumann resonance',
        'requires': ['magnetometer'],
        'available_on': ['macbook'],
        'note_mac_mini': 'Not available — no magnetometer hardware',
    },
    'ambient_light': {
        'name': 'Ambient Light Sensor Noise',
        'description': 'Photon shot noise at low light levels',
        'requires': ['ambient_light_sensor'],
        'available_on': ['macbook', 'imac'],
        'note_mac_mini': 'Not available — no ALS hardware',
    },
    'battery_noise': {
        'name': 'Battery Electrochemical Noise',
        'description': 'Discharge rate and impedance micro-variations',
        'requires': ['battery'],
        'available_on': ['macbook'],
        'note_mac_mini': 'Not available — no battery',
    },
    'motion_sensors': {
        'name': 'Accelerometer/Gyroscope Noise',
        'description': 'MEMS Brownian motion and bias drift',
        'requires': ['motion_sensors'],
        'available_on': ['macbook'],
        'note_mac_mini': 'Not available — no motion sensors',
    },
    'ioregistry': {
        'name': 'IORegistry Deep Mining',
        'description': 'Auto-discover fluctuating numeric values across IOKit',
        'requires': ['macos'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'cross_domain_beat': {
        'name': 'Cross-Clock-Domain Beat Frequency',
        'description': 'Phase noise beats between independent PLL domains',
        'requires': ['macos'],
        'available_on': ['mac_mini', 'macbook', 'imac'],
    },
    'memory_timing': {
        'name': 'DRAM Access Timing',
        'description': 'Memory access/allocation timing variations',
        'requires': [],
        'available_on': ['mac_mini', 'macbook', 'imac', 'linux'],
    },
}


def detect_platform():
    """Full platform detection. Returns dict of capabilities and available sources."""
    sys_platform = platform.system()
    
    capabilities = {
        'system': sys_platform,
        'machine': platform.machine(),
        'model': get_mac_model(),
        'chip': get_chip(),
        'is_mac_mini': is_mac_mini(),
        'is_macbook': is_macbook(),
        'is_imac': is_imac(),
        'has_camera': has_camera(),
        'has_microphone': has_microphone(),
        'has_battery': has_battery(),
        'has_trackpad': has_trackpad(),
        'has_magnetometer': has_magnetometer(),
        'has_ambient_light_sensor': has_ambient_light_sensor(),
        'has_motion_sensors': has_motion_sensors(),
        'has_bluetooth': has_bluetooth(),
        'has_wifi': has_wifi(),
        'has_sudo': has_sudo(),
        'has_smartctl': has_smartctl(),
        'has_imagesnap': has_imagesnap(),
    }
    
    # Determine available sources
    available = {}
    unavailable = {}
    
    requirement_map = {
        'macos': sys_platform == 'Darwin',
        'camera': capabilities['has_camera'],
        'microphone': capabilities['has_microphone'],
        'battery': capabilities['has_battery'],
        'trackpad': capabilities['has_trackpad'],
        'magnetometer': capabilities['has_magnetometer'],
        'ambient_light_sensor': capabilities['has_ambient_light_sensor'],
        'motion_sensors': capabilities['has_motion_sensors'],
        'bluetooth': capabilities['has_bluetooth'],
        'wifi': capabilities['has_wifi'],
        'sudo': capabilities['has_sudo'],
        'smartctl': capabilities['has_smartctl'],
    }
    
    for source_id, source_info in ENTROPY_SOURCES.items():
        requires = source_info.get('requires', [])
        met = all(requirement_map.get(r, False) for r in requires)
        
        if met:
            available[source_id] = source_info
            # Check for enhanced capabilities
            best_with = source_info.get('best_with', [])
            missing_best = [b for b in best_with if not requirement_map.get(b, False)]
            if missing_best:
                available[source_id]['degraded'] = missing_best
        else:
            missing = [r for r in requires if not requirement_map.get(r, False)]
            unavailable[source_id] = {**source_info, 'missing': missing}
    
    return {
        'capabilities': capabilities,
        'available_sources': available,
        'unavailable_sources': unavailable,
    }


if __name__ == '__main__':
    print("=== Esoteric Entropy Platform Detection ===\n")
    
    result = detect_platform()
    caps = result['capabilities']
    
    print(f"System:  {caps['system']} {caps['machine']}")
    print(f"Model:   {caps['model']}")
    print(f"Chip:    {caps['chip']}")
    print()
    
    device_type = 'Mac Mini' if caps['is_mac_mini'] else \
                  'MacBook' if caps['is_macbook'] else \
                  'iMac' if caps['is_imac'] else 'Unknown Mac'
    print(f"Device:  {device_type}")
    print()
    
    print("Hardware Capabilities:")
    for key in ['has_camera', 'has_microphone', 'has_battery', 'has_trackpad',
                'has_magnetometer', 'has_ambient_light_sensor', 'has_motion_sensors',
                'has_bluetooth', 'has_wifi', 'has_sudo', 'has_smartctl', 'has_imagesnap']:
        symbol = '✅' if caps[key] else '❌'
        print(f"  {symbol} {key.replace('has_', '')}")
    
    print(f"\n{'='*50}")
    print(f"Available Entropy Sources ({len(result['available_sources'])}):")
    print(f"{'='*50}")
    for sid, info in result['available_sources'].items():
        degraded = info.get('degraded', [])
        status = ' ⚠️  (better with: ' + ', '.join(degraded) + ')' if degraded else ''
        print(f"  ✅ {info['name']}{status}")
        print(f"     {info['description']}")
    
    print(f"\n{'='*50}")
    print(f"Unavailable Sources ({len(result['unavailable_sources'])}):")
    print(f"{'='*50}")
    for sid, info in result['unavailable_sources'].items():
        missing = info.get('missing', [])
        note = info.get('note_mac_mini', '') if caps['is_mac_mini'] else ''
        print(f"  ❌ {info['name']} — needs: {', '.join(missing)}")
        if note:
            print(f"     💡 {note}")
    
    # Save detection result
    outfile = Path(__file__).parent.parent / 'docs' / 'findings' / 'platform_detection.json'
    outfile.parent.mkdir(parents=True, exist_ok=True)
    with open(outfile, 'w') as f:
        json.dump(result, f, indent=2, default=str)
    print(f"\nSaved detection to {outfile}")
