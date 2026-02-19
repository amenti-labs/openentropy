# Telemetry Model (`telemetry_v1`)

`telemetry_v1` is a best-effort environment snapshot model used to contextualize entropy measurements.

It is emitted as an **experimental** surface in CLI/HTTP outputs.

## Purpose

- Capture host conditions during benchmark/analysis runs.
- Provide reproducible context for comparing runs across machines and time.
- Expose measured values instead of embedding hidden assumptions.
- Feed `quantum_proxy_v3` confound adjustment when quantum proxy and telemetry are enabled together.

## Output Shape

`TelemetrySnapshot` includes:

- model id/version
- timestamp (`collected_unix_ms`)
- host identity (`os`, `arch`, `cpu_count`)
- load average (`1m`, `5m`, `15m`) when available
- metric list (`domain`, `name`, `value`, `unit`, `source`)

`TelemetryWindowReport` includes:

- `start` snapshot
- `end` snapshot
- `elapsed_ms`
- aligned metric deltas (`start_value`, `end_value`, `delta_value`)

## Metric Domains

Depending on host support and permissions:

- `system` (e.g. uptime)
- `memory` (total/free/available and related counters)
- `frequency` (timebase/cpu frequency proxies)
- `thermal` (sensor temperatures, typically Linux hwmon)
- `voltage` / `current` / `power` (typically Linux hwmon)
- `cooling` (fan RPM where exposed)

Unavailable domains are omitted rather than synthesized.

## Where It Appears

- `openentropy telemetry` (standalone snapshot/window capture)
- `openentropy scan --telemetry`
- `openentropy monitor --telemetry`
- `openentropy bench --telemetry`
- `openentropy analyze --telemetry`
- `openentropy report --telemetry`
- `openentropy sessions ... --telemetry`
- `openentropy record --telemetry` (stored in `session.json`)
- `openentropy server --telemetry` (startup snapshot)
- HTTP diagnostics:
  - `/sources?telemetry=true`
  - `/pool/status?telemetry=true`
  - telemetry is an explicit opt-in (`telemetry=true`), independent from `experimental=true`

## Interpretation

- Treat telemetry as **context**, not a direct entropy score.
- Use telemetry deltas to explain run-to-run changes in entropy metrics and quantum proxy terms.
- Do not infer unavailable physical channels (e.g., rail voltage) on platforms where they are not exposed.

## Quantum Integration

`quantum_proxy_v3` can consume a `TelemetryWindowReport` and derive:

- `report.telemetry_confound.confound_index`
- per-source `telemetry_confound_penalty`
- per-source `stress_sensitivity_effective`

Operationally: telemetry increases effective stress under unstable host-state windows, reducing quantum-attributed bits conservatively.

For HTTP diagnostics, this adjustment is only applied when telemetry is requested (`telemetry=true`).
