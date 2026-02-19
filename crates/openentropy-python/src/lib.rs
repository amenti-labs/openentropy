//! Python bindings for openentropy via PyO3.
//!
//! Provides the same API as the pure-Python package but backed by Rust.

use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::metrics::experimental::quantum_proxy_v3::{
    CouplingStats, MODEL_ID as QUANTUM_MODEL_ID, MODEL_VERSION as QUANTUM_MODEL_VERSION,
    QuantumAssessmentConfig, QuantumBatchReport, QuantumSourceInput, TelemetryConfoundConfig,
    assess_batch, assess_batch_from_streams_with_telemetry,
    estimate_stress_sensitivity_from_streams, parse_source_category, quality_factor_from_analysis,
};
use openentropy_core::metrics::standard::{EntropyMeasurements, SourceMeasurementRecord};
use openentropy_core::metrics::streams::collect_source_stream_samples;
use openentropy_core::metrics::telemetry::{collect_telemetry_snapshot, collect_telemetry_window};
use openentropy_core::pool::EntropyPool as RustPool;

fn parse_conditioning_mode(conditioning: &str) -> PyResult<ConditioningMode> {
    match conditioning {
        "raw" => Ok(ConditioningMode::Raw),
        "vonneumann" | "vn" | "von_neumann" => Ok(ConditioningMode::VonNeumann),
        "sha256" => Ok(ConditioningMode::Sha256),
        _ => Err(PyValueError::new_err(format!(
            "invalid conditioning mode '{conditioning}'. expected one of: raw, vonneumann|vn|von_neumann, sha256"
        ))),
    }
}

fn measurements_to_pydict<'py>(
    py: Python<'py>,
    measurements: &EntropyMeasurements,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("bytes", measurements.bytes)?;
    d.set_item("shannon_entropy", measurements.shannon_entropy)?;
    d.set_item("min_entropy", measurements.min_entropy)?;
    d.set_item("compression_ratio", measurements.compression_ratio)?;
    d.set_item("throughput_bps", measurements.throughput_bps)?;
    Ok(d)
}

fn source_measurement_record_to_pydict<'py>(
    py: Python<'py>,
    row: &SourceMeasurementRecord,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("name", &row.name)?;
    d.set_item("category", &row.category)?;
    d.set_item(
        "measurements",
        measurements_to_pydict(py, &row.measurements)?,
    )?;
    Ok(d)
}

fn quantum_report_to_pydict<'py>(
    py: Python<'py>,
    report: &QuantumBatchReport,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    let cfg = PyDict::new(py);
    cfg.set_item("corr_threshold", report.config.corr_threshold)?;
    cfg.set_item("lagged_corr_threshold", report.config.lagged_corr_threshold)?;
    cfg.set_item("mi_threshold_bits", report.config.mi_threshold_bits)?;
    cfg.set_item(
        "lagged_mi_threshold_bits",
        report.config.lagged_mi_threshold_bits,
    )?;
    cfg.set_item("stress_delta_bits", report.config.stress_delta_bits)?;
    cfg.set_item("max_lag", report.config.max_lag)?;
    cfg.set_item("bootstrap_rounds", report.config.bootstrap_rounds)?;
    cfg.set_item("bootstrap_windows", report.config.bootstrap_windows)?;
    cfg.set_item("coupling_weight_corr", report.config.coupling_weight_corr)?;
    cfg.set_item(
        "coupling_weight_lag_corr",
        report.config.coupling_weight_lag_corr,
    )?;
    cfg.set_item("coupling_weight_mi", report.config.coupling_weight_mi)?;
    cfg.set_item(
        "coupling_weight_lag_mi",
        report.config.coupling_weight_lag_mi,
    )?;
    cfg.set_item("coupling_null_rounds", report.config.coupling_null_rounds)?;
    cfg.set_item("coupling_null_sigma", report.config.coupling_null_sigma)?;
    cfg.set_item("coupling_fdr_alpha", report.config.coupling_fdr_alpha)?;
    cfg.set_item("coupling_use_fdr_gate", report.config.coupling_use_fdr_gate)?;
    out.set_item("config", cfg)?;

    let calibration = PyDict::new(py);
    calibration.set_item("global_prior", report.calibration.global_prior)?;
    calibration.set_item(
        "global_prior_ci_low",
        report.calibration.global_prior_ci_low,
    )?;
    calibration.set_item(
        "global_prior_ci_high",
        report.calibration.global_prior_ci_high,
    )?;
    calibration.set_item("category_entries", report.calibration.category_entries)?;
    calibration.set_item("source_entries", report.calibration.source_entries)?;
    out.set_item("calibration", calibration)?;

    let sources = PyList::empty(py);
    for row in &report.sources {
        let d = PyDict::new(py);
        d.set_item("name", &row.name)?;
        d.set_item("category", &row.category)?;
        d.set_item("min_entropy_bits", row.min_entropy_bits)?;
        d.set_item("physics_prior", row.physics_prior)?;
        d.set_item("physics_prior_ci_low", row.physics_prior_ci_low)?;
        d.set_item("physics_prior_ci_high", row.physics_prior_ci_high)?;
        d.set_item("prior_source_samples", row.prior_source_samples)?;
        d.set_item("prior_category_samples", row.prior_category_samples)?;
        d.set_item("quality_factor", row.quality_factor)?;
        d.set_item("stress_sensitivity", row.stress_sensitivity)?;
        d.set_item(
            "stress_sensitivity_effective",
            row.stress_sensitivity_effective,
        )?;
        d.set_item("telemetry_confound_penalty", row.telemetry_confound_penalty)?;
        d.set_item("coupling_mean_abs_r_raw", row.coupling_mean_abs_r_raw)?;
        d.set_item(
            "coupling_mean_abs_r_lag_raw",
            row.coupling_mean_abs_r_lag_raw,
        )?;
        d.set_item("coupling_mean_mi_bits_raw", row.coupling_mean_mi_bits_raw)?;
        d.set_item(
            "coupling_mean_mi_bits_lag_raw",
            row.coupling_mean_mi_bits_lag_raw,
        )?;
        d.set_item("coupling_mean_abs_r_null", row.coupling_mean_abs_r_null)?;
        d.set_item(
            "coupling_mean_abs_r_lag_null",
            row.coupling_mean_abs_r_lag_null,
        )?;
        d.set_item("coupling_mean_mi_bits_null", row.coupling_mean_mi_bits_null)?;
        d.set_item(
            "coupling_mean_mi_bits_lag_null",
            row.coupling_mean_mi_bits_lag_null,
        )?;
        d.set_item("coupling_mean_abs_r", row.coupling_mean_abs_r)?;
        d.set_item("coupling_mean_abs_r_lag", row.coupling_mean_abs_r_lag)?;
        d.set_item("coupling_mean_mi_bits", row.coupling_mean_mi_bits)?;
        d.set_item("coupling_mean_mi_bits_lag", row.coupling_mean_mi_bits_lag)?;
        d.set_item("coupling_mean_q_corr", row.coupling_mean_q_corr)?;
        d.set_item("coupling_mean_q_corr_lag", row.coupling_mean_q_corr_lag)?;
        d.set_item("coupling_mean_q_mi", row.coupling_mean_q_mi)?;
        d.set_item("coupling_mean_q_mi_lag", row.coupling_mean_q_mi_lag)?;
        d.set_item(
            "coupling_significant_pair_fraction_any",
            row.coupling_significant_pair_fraction_any,
        )?;
        d.set_item(
            "coupling_significant_pair_fraction_corr",
            row.coupling_significant_pair_fraction_corr,
        )?;
        d.set_item(
            "coupling_significant_pair_fraction_corr_lag",
            row.coupling_significant_pair_fraction_corr_lag,
        )?;
        d.set_item(
            "coupling_significant_pair_fraction_mi",
            row.coupling_significant_pair_fraction_mi,
        )?;
        d.set_item(
            "coupling_significant_pair_fraction_mi_lag",
            row.coupling_significant_pair_fraction_mi_lag,
        )?;
        d.set_item("coupling_penalty", row.coupling_penalty)?;
        d.set_item("quantum_score", row.quantum_score)?;
        d.set_item("quantum_score_ci_low", row.quantum_score_ci_low)?;
        d.set_item("quantum_score_ci_high", row.quantum_score_ci_high)?;
        d.set_item("classical_score", row.classical_score)?;
        d.set_item("quantum_min_entropy_bits", row.quantum_min_entropy_bits)?;
        d.set_item(
            "quantum_min_entropy_bits_ci_low",
            row.quantum_min_entropy_bits_ci_low,
        )?;
        d.set_item(
            "quantum_min_entropy_bits_ci_high",
            row.quantum_min_entropy_bits_ci_high,
        )?;
        d.set_item("classical_min_entropy_bits", row.classical_min_entropy_bits)?;
        d.set_item(
            "classical_min_entropy_bits_ci_low",
            row.classical_min_entropy_bits_ci_low,
        )?;
        d.set_item(
            "classical_min_entropy_bits_ci_high",
            row.classical_min_entropy_bits_ci_high,
        )?;
        sources.append(d)?;
    }
    out.set_item("sources", sources)?;

    let aggregate = PyDict::new(py);
    aggregate.set_item("quantum_bits", report.aggregate.quantum_bits)?;
    aggregate.set_item("classical_bits", report.aggregate.classical_bits)?;
    aggregate.set_item("quantum_fraction", report.aggregate.quantum_fraction)?;
    aggregate.set_item("classical_fraction", report.aggregate.classical_fraction)?;
    aggregate.set_item(
        "quantum_to_classical",
        report.aggregate.quantum_to_classical,
    )?;
    aggregate.set_item("quantum_bits_ci_low", report.aggregate.quantum_bits_ci_low)?;
    aggregate.set_item(
        "quantum_bits_ci_high",
        report.aggregate.quantum_bits_ci_high,
    )?;
    aggregate.set_item(
        "classical_bits_ci_low",
        report.aggregate.classical_bits_ci_low,
    )?;
    aggregate.set_item(
        "classical_bits_ci_high",
        report.aggregate.classical_bits_ci_high,
    )?;
    aggregate.set_item(
        "quantum_fraction_ci_low",
        report.aggregate.quantum_fraction_ci_low,
    )?;
    aggregate.set_item(
        "quantum_fraction_ci_high",
        report.aggregate.quantum_fraction_ci_high,
    )?;
    aggregate.set_item(
        "quantum_to_classical_ci_low",
        report.aggregate.quantum_to_classical_ci_low,
    )?;
    aggregate.set_item(
        "quantum_to_classical_ci_high",
        report.aggregate.quantum_to_classical_ci_high,
    )?;
    out.set_item("aggregate", aggregate)?;

    if let Some(tc) = &report.telemetry_confound {
        let telemetry_confound = PyDict::new(py);
        telemetry_confound.set_item("confound_index", tc.confound_index)?;
        telemetry_confound.set_item("load_abs_per_core", tc.load_abs_per_core)?;
        telemetry_confound.set_item("load_delta_per_core", tc.load_delta_per_core)?;
        telemetry_confound.set_item("thermal_rise_c", tc.thermal_rise_c)?;
        telemetry_confound.set_item("frequency_drift_ratio", tc.frequency_drift_ratio)?;
        telemetry_confound.set_item("memory_pressure", tc.memory_pressure)?;
        telemetry_confound.set_item("rail_drift_ratio", tc.rail_drift_ratio)?;
        telemetry_confound.set_item("confound_to_stress_scale", tc.confound_to_stress_scale)?;
        out.set_item("telemetry_confound", telemetry_confound)?;
    }

    let ablation = PyList::empty(py);
    for entry in &report.ablation.entries {
        let d = PyDict::new(py);
        d.set_item("scenario", &entry.scenario)?;
        d.set_item("quantum_fraction", entry.quantum_fraction)?;
        d.set_item("quantum_to_classical", entry.quantum_to_classical)?;
        d.set_item("delta_quantum_fraction", entry.delta_quantum_fraction)?;
        d.set_item(
            "delta_quantum_to_classical",
            entry.delta_quantum_to_classical,
        )?;
        ablation.append(d)?;
    }
    out.set_item("ablation", ablation)?;

    let sensitivity = PyDict::new(py);
    let sources = PyList::empty(py);
    for row in &report.sensitivity.sources {
        let d = PyDict::new(py);
        d.set_item("name", &row.name)?;
        d.set_item("baseline_q", row.baseline_q)?;
        d.set_item("impact_without_prior", row.impact_without_prior)?;
        d.set_item("impact_without_quality", row.impact_without_quality)?;
        d.set_item("impact_without_coupling", row.impact_without_coupling)?;
        d.set_item("impact_without_stress", row.impact_without_stress)?;
        sources.append(d)?;
    }
    let summary = PyDict::new(py);
    summary.set_item(
        "mean_impact_without_prior",
        report.sensitivity.summary.mean_impact_without_prior,
    )?;
    summary.set_item(
        "mean_impact_without_quality",
        report.sensitivity.summary.mean_impact_without_quality,
    )?;
    summary.set_item(
        "mean_impact_without_coupling",
        report.sensitivity.summary.mean_impact_without_coupling,
    )?;
    summary.set_item(
        "mean_impact_without_stress",
        report.sensitivity.summary.mean_impact_without_stress,
    )?;
    sensitivity.set_item("sources", sources)?;
    sensitivity.set_item("summary", summary)?;
    out.set_item("sensitivity", sensitivity)?;
    Ok(out)
}

fn parse_quantum_inputs(inputs: &Bound<'_, PyList>) -> PyResult<Vec<QuantumSourceInput>> {
    let mut out = Vec::with_capacity(inputs.len());
    for (idx, item) in inputs.iter().enumerate() {
        let d = item.downcast::<PyDict>()?;
        let name: String = d
            .get_item("name")?
            .ok_or_else(|| PyValueError::new_err(format!("inputs[{idx}] missing 'name'")))?
            .extract()?;
        let min_entropy_bits: f64 = if let Some(v) = d.get_item("min_entropy_bits")? {
            v.extract()?
        } else if let Some(v) = d.get_item("min_entropy")? {
            v.extract()?
        } else {
            return Err(PyValueError::new_err(format!(
                "inputs[{idx}] missing 'min_entropy_bits'"
            )));
        };
        let category = d
            .get_item("category")?
            .and_then(|v| v.extract::<String>().ok())
            .and_then(|s| parse_source_category(&s));
        let quality_factor = d
            .get_item("quality_factor")?
            .and_then(|v| v.extract::<f64>().ok())
            .unwrap_or(1.0);
        let stress_sensitivity = d
            .get_item("stress_sensitivity")?
            .and_then(|v| v.extract::<f64>().ok())
            .unwrap_or(0.0);
        let physics_prior_override = d
            .get_item("physics_prior_override")?
            .and_then(|v| v.extract::<f64>().ok());
        out.push(QuantumSourceInput {
            name,
            category,
            min_entropy_bits,
            quality_factor,
            stress_sensitivity,
            physics_prior_override,
        });
    }
    Ok(out)
}

/// Thread-safe multi-source entropy pool.
#[pyclass(name = "EntropyPool")]
struct PyEntropyPool {
    inner: RustPool,
}

#[pymethods]
impl PyEntropyPool {
    #[new]
    #[pyo3(signature = (seed=None))]
    fn new(seed: Option<&[u8]>) -> Self {
        Self {
            inner: RustPool::new(seed),
        }
    }

    /// Create a pool with all available sources on this machine.
    #[staticmethod]
    fn auto() -> Self {
        Self {
            inner: RustPool::auto(),
        }
    }

    /// Number of registered sources.
    #[getter]
    fn source_count(&self) -> usize {
        self.inner.source_count()
    }

    /// Collect entropy from all sources.
    #[pyo3(signature = (parallel=false, timeout=10.0))]
    fn collect_all(&self, parallel: bool, timeout: f64) -> usize {
        if parallel {
            self.inner.collect_all_parallel(timeout)
        } else {
            self.inner.collect_all()
        }
    }

    /// Return n_bytes of conditioned random output (SHA-256).
    fn get_random_bytes<'py>(&self, py: Python<'py>, n_bytes: usize) -> Bound<'py, PyBytes> {
        let data = self.inner.get_random_bytes(n_bytes);
        PyBytes::new(py, &data)
    }

    /// Return n_bytes with the specified conditioning mode.
    ///
    /// Mode can be "raw", "vonneumann"/"vn", or "sha256" (default).
    #[pyo3(signature = (n_bytes, conditioning="sha256"))]
    fn get_bytes<'py>(
        &self,
        py: Python<'py>,
        n_bytes: usize,
        conditioning: &str,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let mode = parse_conditioning_mode(conditioning)?;
        let data = self.inner.get_bytes(n_bytes, mode);
        Ok(PyBytes::new(py, &data))
    }

    /// Return n_bytes of raw, unconditioned entropy (XOR-combined only).
    ///
    /// No SHA-256, no DRBG, no whitening. Preserves the raw hardware noise
    /// signal for researchers studying actual device entropy characteristics.
    fn get_raw_bytes<'py>(&self, py: Python<'py>, n_bytes: usize) -> Bound<'py, PyBytes> {
        let data = self.inner.get_raw_bytes(n_bytes);
        PyBytes::new(py, &data)
    }

    /// Health report as a Python dict.
    fn health_report<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let report = self.inner.health_report();
        let dict = PyDict::new(py);
        dict.set_item("healthy", report.healthy)?;
        dict.set_item("total", report.total)?;
        dict.set_item("raw_bytes", report.raw_bytes)?;
        dict.set_item("output_bytes", report.output_bytes)?;
        dict.set_item("buffer_size", report.buffer_size)?;

        let sources = PyList::empty(py);
        for s in &report.sources {
            let sd = PyDict::new(py);
            sd.set_item("name", &s.name)?;
            sd.set_item("healthy", s.healthy)?;
            sd.set_item("bytes", s.bytes)?;
            sd.set_item("entropy", s.entropy)?;
            sd.set_item("min_entropy", s.min_entropy)?;
            sd.set_item("time", s.time)?;
            sd.set_item("failures", s.failures)?;
            sources.append(sd)?;
        }
        dict.set_item("sources", sources)?;
        Ok(dict)
    }

    /// Pretty-print health report.
    fn print_health(&self) {
        self.inner.print_health();
    }

    /// Get source info for all registered sources.
    fn sources<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let infos = self.inner.source_infos();
        let list = PyList::empty(py);
        for info in &infos {
            let d = PyDict::new(py);
            d.set_item("name", &info.name)?;
            d.set_item("description", &info.description)?;
            d.set_item("physics", &info.physics)?;
            d.set_item("category", &info.category)?;
            d.set_item("platform", &info.platform)?;
            d.set_item("requirements", &info.requirements)?;
            d.set_item("entropy_rate_estimate", info.entropy_rate_estimate)?;
            d.set_item("composite", info.composite)?;
            list.append(d)?;
        }
        Ok(list)
    }

    /// List registered source names.
    fn source_names(&self) -> Vec<String> {
        self.inner.source_names()
    }

    /// Collect conditioned bytes from a single named source.
    ///
    /// Returns None if no source matches the given name.
    #[pyo3(signature = (source_name, n_bytes, conditioning="sha256"))]
    fn get_source_bytes<'py>(
        &self,
        py: Python<'py>,
        source_name: &str,
        n_bytes: usize,
        conditioning: &str,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let mode = parse_conditioning_mode(conditioning)?;
        Ok(self
            .inner
            .get_source_bytes(source_name, n_bytes, mode)
            .map(|data| PyBytes::new(py, &data)))
    }

    /// Collect raw bytes from a single named source.
    ///
    /// Returns None if no source matches the given name.
    fn get_source_raw_bytes<'py>(
        &self,
        py: Python<'py>,
        source_name: &str,
        n_samples: usize,
    ) -> Option<Bound<'py, PyBytes>> {
        self.inner
            .get_source_raw_bytes(source_name, n_samples)
            .map(|data| PyBytes::new(py, &data))
    }

    /// Collect per-source measurements and a quantum:classical proxy report.
    #[pyo3(signature = (sample_bytes=1024, min_pair_samples=64, telemetry=false))]
    fn quantum_report<'py>(
        &self,
        py: Python<'py>,
        sample_bytes: usize,
        min_pair_samples: usize,
        telemetry: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let sample_bytes = sample_bytes.clamp(64, 4096);
        let min_pair_samples = min_pair_samples.max(2);
        let telemetry_start = telemetry.then(collect_telemetry_snapshot);
        let measurement_rows = PyList::empty(py);
        let mut inputs = Vec::<QuantumSourceInput>::new();
        let mut streams = Vec::<(String, Vec<u8>)>::new();

        let samples = collect_source_stream_samples(&self.inner, sample_bytes, min_pair_samples);
        for sample in samples {
            let standard = SourceMeasurementRecord::from_bytes(
                sample.name.clone(),
                sample.category.clone(),
                &sample.data,
                None,
            );
            let analysis = openentropy_core::analysis::full_analysis(&sample.name, &sample.data);
            let quality_factor = quality_factor_from_analysis(&analysis);

            let row = source_measurement_record_to_pydict(py, &standard)?;
            row.set_item("quality_factor", quality_factor)?;
            measurement_rows.append(row)?;

            inputs.push(QuantumSourceInput {
                name: sample.name.clone(),
                category: parse_source_category(&sample.category),
                min_entropy_bits: standard.measurements.min_entropy,
                quality_factor,
                stress_sensitivity: 0.0,
                physics_prior_override: None,
            });
            streams.push((sample.name, sample.data));
        }

        let qcfg = QuantumAssessmentConfig::default();
        let stress_by_name = estimate_stress_sensitivity_from_streams(&streams, qcfg);
        for input in &mut inputs {
            input.stress_sensitivity = stress_by_name.get(&input.name).copied().unwrap_or(0.0);
        }
        let telemetry_window = telemetry_start.map(collect_telemetry_window);

        let report = if streams.is_empty() {
            let empty: HashMap<String, CouplingStats> = HashMap::new();
            assess_batch(&inputs, &empty, qcfg)
        } else {
            assess_batch_from_streams_with_telemetry(
                &inputs,
                &streams,
                qcfg,
                min_pair_samples,
                telemetry_window.as_ref(),
                TelemetryConfoundConfig::default(),
            )
        };
        let quantum_ratio = quantum_report_to_pydict(py, &report)?;

        let out = PyDict::new(py);
        let standard = PyDict::new(py);
        standard.set_item("measurements", &measurement_rows)?;
        out.set_item("standard", standard)?;

        let model = PyDict::new(py);
        model.set_item("model_id", QUANTUM_MODEL_ID)?;
        model.set_item("model_version", QUANTUM_MODEL_VERSION)?;
        model.set_item("sample_bytes", sample_bytes)?;
        model.set_item("report", &quantum_ratio)?;

        let experimental = PyDict::new(py);
        experimental.set_item("quantum_proxy_v3", model)?;
        out.set_item("experimental", experimental)?;
        Ok(out)
    }
}

/// Run the full NIST test battery on a bytes object.
#[pyfunction]
fn run_all_tests<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyList>> {
    let results = openentropy_tests::run_all_tests(data);
    let list = PyList::empty(py);
    for r in &results {
        let d = PyDict::new(py);
        d.set_item("name", &r.name)?;
        d.set_item("passed", r.passed)?;
        d.set_item("p_value", r.p_value)?;
        d.set_item("statistic", r.statistic)?;
        d.set_item("details", &r.details)?;
        d.set_item("grade", r.grade.to_string())?;
        list.append(d)?;
    }
    Ok(list)
}

/// Calculate quality score from test results.
#[pyfunction]
fn calculate_quality_score(results: &Bound<'_, PyList>) -> PyResult<f64> {
    let mut rust_results = Vec::new();
    for item in results.iter() {
        let d = item.downcast::<PyDict>()?;
        let grade: String = d
            .get_item("grade")?
            .map(|v| v.extract::<String>())
            .unwrap_or(Ok("F".to_string()))?;
        rust_results.push(openentropy_tests::TestResult {
            name: d
                .get_item("name")?
                .map(|v| v.extract::<String>())
                .unwrap_or(Ok(String::new()))?,
            passed: d
                .get_item("passed")?
                .map(|v| v.extract::<bool>())
                .unwrap_or(Ok(false))?,
            p_value: d.get_item("p_value")?.and_then(|v| v.extract::<f64>().ok()),
            statistic: d
                .get_item("statistic")?
                .map(|v| v.extract::<f64>())
                .unwrap_or(Ok(0.0))?,
            details: d
                .get_item("details")?
                .map(|v| v.extract::<String>())
                .unwrap_or(Ok(String::new()))?,
            grade: grade.chars().next().unwrap_or('F'),
        });
    }
    Ok(openentropy_tests::calculate_quality_score(&rust_results))
}

/// Detect available entropy sources on this machine.
#[pyfunction]
fn detect_available_sources<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
    let sources = openentropy_core::detect_available_sources();
    let list = PyList::empty(py);
    for s in &sources {
        let info = s.info();
        let d = PyDict::new(py);
        d.set_item("name", info.name)?;
        d.set_item("description", info.description)?;
        d.set_item("category", info.category.to_string())?;
        d.set_item("entropy_rate_estimate", info.entropy_rate_estimate)?;
        list.append(d)?;
    }
    Ok(list)
}

/// Platform information.
#[pyfunction]
fn platform_info<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let info = openentropy_core::platform_info();
    let d = PyDict::new(py);
    d.set_item("system", info.system)?;
    d.set_item("machine", info.machine)?;
    d.set_item("family", info.family)?;
    Ok(d)
}

/// Detect machine information (best-effort).
#[pyfunction]
fn detect_machine_info<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let info = openentropy_core::detect_machine_info();
    let d = PyDict::new(py);
    d.set_item("os", info.os)?;
    d.set_item("arch", info.arch)?;
    d.set_item("chip", info.chip)?;
    d.set_item("cores", info.cores)?;
    Ok(d)
}

/// Apply conditioning mode to bytes.
#[pyfunction]
#[pyo3(signature = (data, n_output, conditioning="sha256"))]
fn condition<'py>(
    py: Python<'py>,
    data: &[u8],
    n_output: usize,
    conditioning: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let mode = parse_conditioning_mode(conditioning)?;
    let out = openentropy_core::condition(data, n_output, mode);
    Ok(PyBytes::new(py, &out))
}

/// Full min-entropy estimator report.
#[pyfunction]
fn min_entropy_estimate<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let report = openentropy_core::min_entropy_estimate(data);
    let d = PyDict::new(py);
    d.set_item("shannon_entropy", report.shannon_entropy)?;
    d.set_item("min_entropy", report.min_entropy)?;
    d.set_item("heuristic_floor", report.heuristic_floor)?;
    d.set_item("mcv_estimate", report.mcv_estimate)?;
    d.set_item("mcv_p_upper", report.mcv_p_upper)?;
    d.set_item("collision_estimate", report.collision_estimate)?;
    d.set_item("markov_estimate", report.markov_estimate)?;
    d.set_item("compression_estimate", report.compression_estimate)?;
    d.set_item("t_tuple_estimate", report.t_tuple_estimate)?;
    d.set_item("samples", report.samples)?;
    Ok(d)
}

/// Standardized per-stream entropy measurements.
#[pyfunction]
#[pyo3(signature = (data, elapsed_seconds=None))]
fn entropy_measurements<'py>(
    py: Python<'py>,
    data: &[u8],
    elapsed_seconds: Option<f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let measurements = EntropyMeasurements::from_bytes(data, elapsed_seconds);
    measurements_to_pydict(py, &measurements)
}

/// Assess quantum:classical proxy ratios for precomputed source inputs.
///
/// `inputs` is a list of dicts containing:
/// - required: `name`, `min_entropy_bits` (or `min_entropy`)
/// - optional: `category`, `quality_factor`, `stress_sensitivity`, `physics_prior_override`
///
/// `streams` (optional) is a dict of `name -> bytes` used to compute coupling penalties.
#[pyfunction]
#[pyo3(signature = (inputs, streams=None, min_pair_samples=64, telemetry=false))]
fn quantum_assess_batch<'py>(
    py: Python<'py>,
    inputs: &Bound<'_, PyList>,
    streams: Option<&Bound<'_, PyDict>>,
    min_pair_samples: usize,
    telemetry: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let mut inputs = parse_quantum_inputs(inputs)?;
    let cfg = QuantumAssessmentConfig::default();
    let report = if let Some(streams_dict) = streams {
        let telemetry_start = telemetry.then(collect_telemetry_snapshot);
        let mut stream_rows = Vec::<(String, Vec<u8>)>::new();
        for (name, value) in streams_dict.iter() {
            let source_name: String = name.extract()?;
            let bytes = value.extract::<&[u8]>()?.to_vec();
            stream_rows.push((source_name, bytes));
        }
        let stress_map = estimate_stress_sensitivity_from_streams(&stream_rows, cfg);
        for input in &mut inputs {
            if input.stress_sensitivity <= 0.0 {
                input.stress_sensitivity = stress_map.get(&input.name).copied().unwrap_or(0.0);
            }
        }
        let telemetry_window = telemetry_start.map(collect_telemetry_window);
        assess_batch_from_streams_with_telemetry(
            &inputs,
            &stream_rows,
            cfg,
            min_pair_samples.max(2),
            telemetry_window.as_ref(),
            TelemetryConfoundConfig::default(),
        )
    } else {
        let empty: HashMap<String, CouplingStats> = HashMap::new();
        assess_batch(&inputs, &empty, cfg)
    };
    quantum_report_to_pydict(py, &report)
}

/// Fast MCV min-entropy estimate.
#[pyfunction]
fn quick_min_entropy(data: &[u8]) -> f64 {
    openentropy_core::quick_min_entropy(data)
}

/// Fast Shannon entropy estimate.
#[pyfunction]
fn quick_shannon(data: &[u8]) -> f64 {
    openentropy_core::quick_shannon(data)
}

/// Grade a source based on min-entropy.
#[pyfunction]
fn grade_min_entropy(min_entropy: f64) -> String {
    openentropy_core::grade_min_entropy(min_entropy).to_string()
}

/// Quick quality report.
#[pyfunction]
fn quick_quality<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let report = openentropy_core::quick_quality(data);
    let d = PyDict::new(py);
    d.set_item("samples", report.samples)?;
    d.set_item("unique_values", report.unique_values)?;
    d.set_item("shannon_entropy", report.shannon_entropy)?;
    d.set_item("compression_ratio", report.compression_ratio)?;
    d.set_item("quality_score", report.quality_score)?;
    d.set_item("grade", report.grade.to_string())?;
    Ok(d)
}

/// Library version.
#[pyfunction]
fn version() -> &'static str {
    openentropy_core::VERSION
}

/// Python module definition.
#[pymodule]
fn openentropy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", openentropy_core::VERSION)?;
    m.add_class::<PyEntropyPool>()?;
    m.add_function(wrap_pyfunction!(run_all_tests, m)?)?;
    m.add_function(wrap_pyfunction!(calculate_quality_score, m)?)?;
    m.add_function(wrap_pyfunction!(detect_available_sources, m)?)?;
    m.add_function(wrap_pyfunction!(platform_info, m)?)?;
    m.add_function(wrap_pyfunction!(detect_machine_info, m)?)?;
    m.add_function(wrap_pyfunction!(condition, m)?)?;
    m.add_function(wrap_pyfunction!(min_entropy_estimate, m)?)?;
    m.add_function(wrap_pyfunction!(entropy_measurements, m)?)?;
    m.add_function(wrap_pyfunction!(quantum_assess_batch, m)?)?;
    m.add_function(wrap_pyfunction!(quick_min_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(quick_shannon, m)?)?;
    m.add_function(wrap_pyfunction!(grade_min_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(quick_quality, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
