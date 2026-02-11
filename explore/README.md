# Exploration Scripts

This directory contains the original exploration scripts used to discover and characterise entropy sources. They're kept as examples and tutorials.

Each script is standalone — run with `python explore/<script>.py`.

## Scripts

| Script | What it explores |
|--------|-----------------|
| `mach_timing_deep.py` | Mach kernel timing, thread scheduling, page faults |
| `network_timing.py` | DNS and TCP round-trip timing jitter |
| `memory_timing.py` | DRAM access and allocation timing |
| `disk_io_jitter.py` | NVMe/SSD read latency variations |
| `gpu_compute_jitter.py` | GPU dispatch timing via sips |
| `sensor_jitter.py` | Accelerometer noise via IOKit |
| `mic_thermal_noise.py` | Microphone Johnson-Nyquist noise |
| `wifi_rssi_entropy.py` | WiFi signal strength fluctuations |
| `camera_dark_current.py` | Camera sensor dark noise |
| `camera_quantum.py` | Photon shot noise from camera |
| `ble_ambient_noise.py` | Bluetooth LE advertisement RSSI |
| `nvme_smart_jitter.py` | NVMe SMART attribute fluctuations |
| `smc_sensor_galaxy.py` | SMC sensor readings via powermetrics |
| `overlooked_discovery.py` | Broad sweep of all fluctuating data on the system |
| `platform_detect.py` | Hardware capability detection |

## How to Explore a New Source

1. Write a script that samples the phenomenon rapidly
2. Save raw samples to a `.bin` file
3. Run `python benchmark.py` to test entropy quality
4. If it passes, promote it to a source in `esoteric_entropy/sources/`
