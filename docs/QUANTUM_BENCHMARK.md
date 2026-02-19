# Quantum Benchmark (`quantum_proxy_v3`)

This document defines the **experimental** quantum:classical contribution proxy used by:

- `openentropy bench --quantum`
- `openentropy analyze --quantum-ratio`
- `openentropy sessions ... --quantum-ratio`
- server diagnostics (`/sources?experimental=true`, `/pool/status?experimental=true`)
- Python SDK: `EntropyPool.quantum_report()`, `quantum_assess_batch(...)`

Implementation:

- `crates/openentropy-core/src/metrics/experimental/quantum_proxy_v3.rs`

## Status

`quantum_proxy_v3` is intentionally versioned under `experimental`.

It is designed for:

- source comparison and research ranking
- decomposition diagnostics (`q_bits` vs `c_bits`)
- model behavior tracking across runs and versions

It is **not** a formal QRNG proof and must not be used as a compliance claim.

## Output Contract

Quantum results are emitted under:

- `experimental.quantum_proxy_v3`

with:

- `model_id`
- `model_version`
- `report` (`sources`, `aggregate`, `config`, `calibration`, `ablation`, `sensitivity`, optional `telemetry_confound`)

`standard` metrics remain the release/production benchmark surface.

## Per-Source Model

For each source, the model computes:

1. `physics_prior` in `[0,1]`
2. `quality_factor` in `[0,1]`
3. `stress_sensitivity` in `[0,1]`
4. `coupling_penalty` in `[0,1]`
5. optional telemetry confound adjustment when a telemetry window is available

Then:

```text
stress_effective = clamp01(stress_sensitivity + telemetry_confound_penalty)
q      = physics_prior * quality_factor * (1 - stress_effective) * (1 - coupling_penalty)
q_bits = H∞ * q
c_bits = H∞ - q_bits
```

Where `H∞` is per-source min-entropy from standard measurements.

## Component Details

### 1) Physics Prior (calibrated, hierarchical)

`physics_prior` is produced from a calibrated prior table (`PriorCalibration`) with:

- global beta posterior
- category-level posteriors
- source-level posteriors

The final prior uses shrinkage:

- source estimate (when available)
- otherwise category estimate
- otherwise global estimate

Optional custom calibration is supported in CLI:

- `openentropy bench --quantum --quantum-calibration path/to/calibration.json`

If loading fails, the model falls back to seeded default calibration.

### 2) Quality Factor (measured)

`quality_factor` is derived from measured source statistics:

- autocorrelation
- bit bias
- spectral flatness
- stationarity heuristic
- runs behavior

The terms are combined multiplicatively and clamped to `[0,1]`.

### 3) Stress Sensitivity (measured proxy)

`stress_sensitivity` maps entropy instability to `[0,1]` via `stress_delta_bits`.

Two pathways:

- default: stream variability estimate over windows (`estimate_stress_sensitivity_from_streams`)
- optional active stress sweep (`collect_stress_sweep`) with CPU/memory/scheduler load

CLI flag for active sweep:

- `openentropy bench --quantum --quantum-live-stress`

### 4) Coupling Penalty (measured + null-debiased)

Coupling is estimated from raw stream pairs using:

- absolute Pearson correlation (zero-lag + max-lag)
- adaptive-bin mutual information (zero-lag + max-lag)

`v3` applies finite-sample debiasing:

- compute observed coupling
- compute null coupling from circularly shifted pair baselines
- keep only excess over null (`observed - (null_mean + sigma * null_std)`, clamped at 0)

`v3` also applies finite-sample MI correction (Miller-Madow style) before coupling aggregation.

Significance diagnostics are computed per pair/metric:

- one-sided p-values from a null-fitted tail approximation (with permutation-style fallback for tiny null samples)
- Benjamini-Hochberg FDR correction to q-values
- per-source `coupling_significant_pair_fraction_*` and `coupling_mean_q_*` outputs

By default, coupling penalty remains continuous (not hard-gated by significance).  
Optional hard gating is available through `coupling_use_fdr_gate`.

Penalty is computed from excess terms, while reports retain raw/null/excess/significance diagnostics for auditability.

### 5) Telemetry Confound (measured host-state adjustment)

When telemetry windows are available (for example `--quantum --telemetry` in CLI benchmarks),
`v3` computes a `confound_index` from measured environment drift across the run window:

- load level and load delta (normalized per core)
- thermal rise
- frequency drift
- memory pressure
- voltage/current/power rail drift when exposed by host telemetry

The confound index is mapped into a bounded stress addend:

- `telemetry_confound_penalty` (per source, category-scaled)
- `stress_sensitivity_effective = clamp01(stress_sensitivity + telemetry_confound_penalty)`

This penalizes quantum attribution under unstable host conditions without discarding sources.

## Uncertainty

`v3` includes uncertainty intervals:

- windowed variability for `H∞`, quality, stress, coupling
- Monte Carlo draws over component uncertainty
- per-source CI for `q`, `q_bits`, `c_bits`
- aggregate CI for `Q_bits`, `C_bits`, `Q_fraction`, `Q:C`

Aggregate point values in `report.aggregate` are Monte-Carlo-centered (same distribution as the CI bounds).

## Aggregate Ratio

Across included sources:

```text
Q_bits     = sum(q_bits)
C_bits     = sum(c_bits)
Q_fraction = Q_bits / (Q_bits + C_bits)
Q_to_C     = Q_bits / C_bits
```

## Ablation and Sensitivity

`v3` reports:

- ablation scenarios (`without_prior`, `without_quality`, `without_coupling`, `without_stress`, etc.)
- per-source and mean impact sensitivity summaries

These are useful for understanding what terms are driving the reported quantum fraction.
`ablation` rows are deterministic point-estimate scenarios (not uncertainty-centered).

## Measured vs Assumed

Measured directly from sampled streams:

- min-entropy (`H∞`)
- quality term inputs
- coupling raw moments
- coupling null baseline and excess
- coupling significance diagnostics (`q`-value summaries and significant pair fractions)
- stress (window proxy and optional active sweep)
- telemetry confound inputs (load/thermal/frequency/memory/rail deltas) when telemetry is enabled

Model assumptions / priors:

- calibration labels used to seed source/category priors
- shrinkage weights and thresholds
- multiplicative decomposition form
- null tail fit family used for coupling p-value approximation
- Monte Carlo distribution assumptions for CI generation

## Interpretation Guidance

Use this split in practice:

- `standard`: release/production decisions (`H∞`, stability, throughput, health)
- `experimental.quantum_proxy_v3`: research diagnostics and ranking context

Interpretation tips:

- high `q` with strong quality and low coupling/stress is stronger evidence
- high prior alone is insufficient
- high raw coupling with low excess-to-null indicates likely finite-sample artifact, not true dependence

## Limitations

- still a proxy model, not physical proof
- calibration quality depends on labeled calibration quality
- stress sweeps are operational perturbations, not complete physical controls
- telemetry confound quality depends on host-exposed sensors (some platforms expose fewer channels)
- uncertainty intervals are model-based, not formal nonparametric confidence guarantees
- coupling significance is still model-level statistical evidence (not causal/physical proof)

## Research Basis (Selected)

- Benjamini, Hochberg (1995): False Discovery Rate control. https://doi.org/10.1111/j.2517-6161.1995.tb02031.x
- Phipson, Smyth (2010): permutation p-value safeguards. https://doi.org/10.2202/1544-6115.1585
- Paninski (2003): finite-sample entropy/MI estimator bias behavior. https://doi.org/10.1162/089976603321780272
- Harris (2020): shift/surrogate-style testing for autocorrelated data. https://arxiv.org/abs/2011.01177
- NIST SP 800-90B: entropy source evaluation framing. https://csrc.nist.gov/pubs/sp/800/90/b/final

## Versioning Policy

Changes to formulas, priors, thresholds, uncertainty model, or feature terms should bump `model_version`.
Consumers should key behavior on both:

- `model_id`
- `model_version`
