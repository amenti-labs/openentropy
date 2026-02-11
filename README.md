# esoteric-entropy

Exploring unconventional, intuitive, and esoteric sources of entropy from device hardware.

## Vision

Modern devices contain sensors and radios that detect subtle physical phenomena — electromagnetic fields, thermal noise, RF interference, quantum tunneling effects in semiconductors, and more. Most of these signals are treated as *noise to eliminate*. We treat them as **entropy to harvest**.

## Potential Entropy Sources

### RF & Electromagnetic
- **WiFi RSSI fluctuations** — micro-variations in signal strength from ambient RF
- **Bluetooth LE advertisement noise** — timing jitter and RSSI from ambient BLE beacons
- **NFC field perturbations** — near-field electromagnetic coupling variations
- **Software-defined radio** (RTL-SDR) — raw RF spectrum noise floor

### Sensor-Based
- **Magnetometer noise** — geomagnetic micro-fluctuations (available on phones/some laptops)
- **Accelerometer/gyroscope LSB noise** — quantum-limited MEMS thermal noise
- **Barometric pressure sensor noise** — atmospheric micro-turbulence
- **Ambient light sensor fluctuations** — photon shot noise at low light
- **Microphone thermal noise** — with input muted, ADC thermal/Johnson noise

### Thermal & Electrical
- **CPU/GPU temperature sensor jitter** — thermal noise in temperature ADCs
- **USB voltage fluctuations** — power rail noise from switching regulators
- **Fan speed sensor timing** — RPM measurement jitter
- **Battery charge rate micro-variations** — electrochemical noise

### Timing & System
- **Clock drift between oscillators** — crystal oscillator phase noise
- **Interrupt timing jitter** — non-deterministic hardware interrupt spacing
- **Memory access timing** — DRAM refresh timing variations (rowhammer-adjacent)
- **Disk I/O latency noise** — mechanical or flash cell timing variations

### Exotic / Esoteric
- **Camera sensor dark current** — photon shot noise with lens cap on
- **Schumann resonance detection** — Earth's electromagnetic resonance (~7.83 Hz) via magnetometer
- **Solar/cosmic ray bit flips** — radiation-induced memory errors
- **Piezoelectric effects** — vibration-to-voltage in MEMS structures

## Project Structure

```
explore/          # Exploratory scripts — one per entropy source
  wifi_rssi.py
  mic_thermal.py
  sensor_noise.py
  ...
analysis/         # Statistical quality analysis (NIST tests, etc.)
src/              # Package source (future)
docs/             # Research notes and findings
```

## Getting Started

```bash
pip install -r requirements.txt
python explore/<source>.py
```

## Philosophy

Every sensor is a window into physical reality. The "noise" in these signals isn't random in the boring sense — it's the fingerprint of actual physical processes: thermal agitation of electrons, quantum effects in semiconductors, electromagnetic waves passing through space. By harvesting this noise thoughtfully, we create entropy that is grounded in the physical world in ways that PRNGs can never be.

## License

MIT
