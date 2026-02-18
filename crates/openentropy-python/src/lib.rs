//! Python bindings for openentropy via PyO3.
//!
//! Provides the same API as the pure-Python package but backed by Rust.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use openentropy_core::conditioning::ConditioningMode;
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
    m.add_function(wrap_pyfunction!(quick_min_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(quick_shannon, m)?)?;
    m.add_function(wrap_pyfunction!(grade_min_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(quick_quality, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
