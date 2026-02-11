# Esoteric Entropy Research Plan

## Mission
Discover, harvest, and characterize unconventional entropy sources from consumer device hardware. Build toward an open-source package that combines multiple esoteric sources into a high-quality entropy pool.

---

## Phase 1: Discovery & Exploration (Week 1-2)
**Goal:** Cast a wide net — probe every accessible hardware source for entropy potential.

### 1.1 Audio Domain
- [x] Mic thermal noise (Johnson-Nyquist from ADC)
- [ ] Mic with input muted vs unmuted comparison
- [ ] Inter-channel phase noise (stereo mic differential)
- [ ] Ultrasonic band noise (>18kHz — inaudible environmental RF coupling)
- [ ] Audio codec clock jitter (sample rate drift between devices)

### 1.2 RF & Electromagnetic
- [x] WiFi RSSI fluctuations
- [ ] WiFi channel scan timing jitter
- [ ] Bluetooth LE advertisement RSSI + timing from ambient devices
- [ ] Bluetooth frequency hop pattern timing
- [ ] CoreLocation WiFi fingerprint noise (macOS)
- [ ] RTL-SDR raw IQ noise floor (if hardware available)
- [ ] NFC field strength variations (if accessible)

### 1.3 Sensors (macOS/iOS via IOKit/CoreMotion)
- [ ] Magnetometer LSB noise — geomagnetic micro-fluctuations
- [ ] Accelerometer rest-state noise — MEMS Brownian motion
- [ ] Gyroscope bias drift — temperature-dependent wandering
- [ ] Barometric pressure sensor LSB jitter
- [ ] Ambient light sensor photon shot noise (low-light regime)

### 1.4 Thermal & Power
- [ ] CPU die temperature sensor jitter (SMC via IOKit)
- [ ] GPU temperature fluctuations
- [ ] Fan RPM measurement noise
- [ ] Battery discharge rate micro-variations
- [ ] USB power rail voltage noise (if accessible via IOKit)
- [ ] Thunderbolt/USB-C PD negotiation timing

### 1.5 Timing & System
- [x] Clock call jitter (perf_counter)
- [x] Sleep-wake interrupt jitter
- [ ] TSC (Time Stamp Counter) LSB noise
- [ ] Mach absolute time differential jitter
- [ ] Context switch timing noise
- [ ] Memory allocation timing (ASLR + heap state dependent)
- [ ] Disk I/O latency jitter (NVMe command timing)
- [ ] Network packet arrival timing (ambient traffic)

### 1.6 Exotic / Esoteric
- [ ] Camera sensor dark current (lens cap / tape over camera)
- [ ] Camera sensor hot pixel map — radiation damage accumulation
- [ ] Trackpad capacitive sensor noise floor
- [ ] Touch Bar sensor noise (if applicable)
- [ ] Schumann resonance detection via magnetometer FFT at 7.83Hz
- [ ] Cosmic ray detection via repeated memory pattern checks
- [ ] Kernel entropy pool timing (`/dev/random` read latency)
- [ ] Secure Enclave RNG timing side-channel

---

## Phase 2: Characterization & Quality (Week 2-3)
**Goal:** Rigorous statistical testing of each discovered source.

### 2.1 Statistical Test Suite
- [ ] Implement Shannon entropy calculator
- [ ] Implement min-entropy estimator (NIST SP 800-90B)
- [ ] Chi-squared uniformity test
- [ ] Serial correlation test
- [ ] Runs test (FIPS 140-2)
- [ ] Maurer's universal statistical test
- [ ] Integrate NIST SP 800-22 test suite (external binary)
- [ ] Autocorrelation analysis (lag 1-100)
- [ ] FFT spectral analysis for hidden periodicity

### 2.2 Per-Source Characterization
For each source, produce a report card:
- Raw bit rate (bits/second)
- Shannon entropy per bit
- Min-entropy per bit  
- Autocorrelation profile
- Spectral analysis
- Environmental sensitivity (temperature, time of day, load)
- Platform availability (macOS/Linux/Windows)
- Privilege requirements (user/root/entitlements)

### 2.3 Conditioning Pipeline
- [ ] Von Neumann debiasing (already in Phase 1 scripts)
- [ ] XOR folding (combine N bits → 1)
- [ ] LFSR whitening
- [ ] SHA-256 conditioning (NIST approved)
- [ ] Toeplitz hashing (information-theoretic extraction)
- [ ] Compare conditioning methods per source

---

## Phase 3: Combination & Mixing (Week 3-4)
**Goal:** Combine multiple independent sources into a high-quality entropy pool.

### 3.1 Multi-Source Mixer
- [ ] Design entropy pool architecture (Linux /dev/random inspired)
- [ ] Implement credit-based mixing (weight by measured entropy rate)
- [ ] Cross-correlation analysis between sources (verify independence)
- [ ] Continuous health monitoring — detect source degradation
- [ ] Automatic fallback when sources become unavailable

### 3.2 Entropy Rate Estimation
- [ ] Real-time entropy rate tracking per source
- [ ] Adaptive conditioning based on estimated rate
- [ ] Starvation detection and alerting

### 3.3 Output Interface
- [ ] Raw bytes API
- [ ] Uniform integer generation
- [ ] Uniform float generation
- [ ] Gaussian/normal distribution
- [ ] Configurable output rate limiting

---

## Phase 4: Package Architecture (Week 4-6)
**Goal:** Transform exploration into a proper open-source package.

### 4.1 Core Library (`esoteric-entropy`)
```
src/
  core/
    pool.py          # Entropy pool + mixer
    health.py        # Continuous health monitoring
    conditioning.py  # Whitening/extraction algorithms
  sources/
    base.py          # Abstract source interface
    audio.py         # Mic thermal, ultrasonic, phase noise
    rf.py            # WiFi, BLE, SDR
    sensor.py        # Magnetometer, accelerometer, etc.
    thermal.py       # CPU/GPU temp, fan, power
    timing.py        # Clock jitter, interrupt, TSC
    exotic.py        # Camera dark current, cosmic rays, Schumann
  platform/
    macos.py         # IOKit, CoreWLAN, CoreMotion bindings
    linux.py         # sysfs, iw, ALSA bindings
    windows.py       # WMI, Win32 sensor API
  stats/
    tests.py         # Statistical test suite
    report.py        # Source characterization reports
```

### 4.2 CLI Tool
```bash
esoteric-entropy scan          # Discover available sources
esoteric-entropy probe <src>   # Test a specific source
esoteric-entropy bench         # Benchmark all sources
esoteric-entropy stream        # Stream mixed entropy to stdout
esoteric-entropy report        # Full characterization report
```

### 4.3 Quality Standards
- [ ] Type hints throughout
- [ ] 90%+ test coverage
- [ ] Sphinx documentation
- [ ] CI/CD with GitHub Actions
- [ ] Platform matrix testing (macOS arm64, Linux x86_64)
- [ ] Security review of entropy claims

---

## Phase 5: Advanced Research (Ongoing)
**Goal:** Push into truly novel territory.

### 5.1 Temporal Correlation Studies
- [ ] Entropy quality vs time of day (solar activity?)
- [ ] Entropy quality vs geomagnetic conditions (Kp index)
- [ ] Lunar cycle correlation (tidal electromagnetic effects?)
- [ ] Solar flare / CME event impact on sensor noise

### 5.2 Cross-Device Correlation
- [ ] Run on two devices simultaneously — are noise sources truly independent?
- [ ] Geographic correlation (same sources, different locations)
- [ ] Network effect — does nearby device activity create correlated noise?

### 5.3 Consciousness / Intention Studies
- [ ] REG (Random Event Generator) protocol implementation
- [ ] GCP (Global Consciousness Project) compatible output format
- [ ] Intention experiments — can focused attention bias hardware entropy?
- [ ] Integration with QRNG research (qrng-research repo)

### 5.4 Novel Source Research
- [ ] Quantum tunneling in flash memory cells (write timing)
- [ ] Josephson junction effects in superconducting elements (Apple Silicon?)
- [ ] Photonic noise from display backlight PWM
- [ ] Electromagnetic emanation from CPU computation patterns
- [ ] Earth's telluric currents via grounded metal chassis

---

## Recursive Improvement Protocol

After each exploration cycle:
1. **Measure** — Run statistical tests on all collected entropy
2. **Rank** — Order sources by entropy quality and bit rate
3. **Prune** — Drop sources below min-entropy threshold
4. **Deepen** — Investigate top sources for optimization (sampling rate, bit selection, conditioning)
5. **Combine** — Test new mixing combinations
6. **Document** — Update characterization reports
7. **Repeat** — Each cycle improves the overall pool quality

### Automated Improvement
- [ ] Benchmark script that runs all sources and produces ranked report
- [ ] CI job that tracks entropy quality over time
- [ ] Automatic parameter tuning (sampling rate, bit extraction method)
- [ ] Regression detection — alert if entropy quality degrades

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Unique sources discovered | 15+ |
| Sources passing NIST tests | 8+ |
| Combined entropy rate | >1 Kbit/s |
| Min-entropy per output bit | >0.99 |
| Platform support | macOS + Linux |
| Package installable via pip | Yes |
| Academic paper potential | Yes |

---

## Immediate Next Steps (Today)
1. Run existing 3 explorers, collect baseline data
2. Build statistical test framework
3. Add camera dark current explorer
4. Add magnetometer/accelerometer explorer (IOKit)
5. Create benchmark runner that tests all sources
6. Document findings in `docs/findings/`
