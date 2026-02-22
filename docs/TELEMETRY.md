# Telemetry Model (`telemetry_v1`)

`telemetry_v1` is a best-effort environment snapshot model used to contextualize entropy measurements.

Design contract:

- One model id/version: `telemetry_v1` / `1`
- One structured report shape for diagnostics: `TelemetryWindowReport` (`start`, `end`, `deltas`)
- One point-in-time shape for ad hoc display: `TelemetrySnapshot`

## Purpose

- Capture host conditions during benchmark/analysis runs.
- Provide reproducible context for comparing runs across machines and time.
- Expose measured values instead of embedding hidden assumptions.

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
- `scheduling` (CPU time counters, runnable/process counters, tasks/threads)
- `pressure` (Linux PSI and macOS memory pressure level when available)
- `frequency` (timebase/cpu frequency proxies)
- `network` (aggregate byte/packet/error counters)
- `disk` (aggregate I/O counters and timings)
- `entropy` (Linux kernel entropy pool availability)
- `vm` (macOS vm_stat counters)
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
- `openentropy analyze --report --telemetry`
- `openentropy sessions ... --telemetry`
- `openentropy record --telemetry` (stored in `session.json`)
- `openentropy server --telemetry` (startup snapshot)
- HTTP diagnostics:
  - `/sources?telemetry=true`
  - `/pool/status?telemetry=true`
  - returns `telemetry_v1` as a `TelemetryWindowReport`

CLI JSON outputs that include telemetry store it at:

- `telemetry_v1` (`TelemetryWindowReport`)

Session recordings store telemetry at:

- `session.json -> telemetry_v1` (`TelemetryWindowReport`)
- Backward compatibility: reader accepts legacy `session.json -> telemetry`.

`scan --telemetry`, `monitor --telemetry`, and `server --telemetry` print a startup snapshot to stdout for operator context.

## Interpretation

- Treat telemetry as **context**, not a direct entropy score.
- Use telemetry deltas to explain run-to-run changes in entropy metrics.
- Do not infer unavailable physical channels (e.g., rail voltage) on platforms where they are not exposed.
