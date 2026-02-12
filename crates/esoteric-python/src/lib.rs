//! Python bindings for esoteric-entropy via PyO3.
//!
//! Provides the same API as the pure-Python package but backed by Rust.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use esoteric_core::pool::EntropyPool as RustPool;

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
            d.set_item("entropy_rate_estimate", info.entropy_rate_estimate)?;
            list.append(d)?;
        }
        Ok(list)
    }
}

/// Run the full NIST test battery on a bytes object.
#[pyfunction]
fn run_all_tests<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyList>> {
    let results = esoteric_tests::run_all_tests(data);
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
        rust_results.push(esoteric_tests::TestResult {
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
    Ok(esoteric_tests::calculate_quality_score(&rust_results))
}

/// Detect available entropy sources on this machine.
#[pyfunction]
fn detect_available_sources<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
    let sources = esoteric_core::detect_available_sources();
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

/// Library version.
#[pyfunction]
fn version() -> &'static str {
    esoteric_core::VERSION
}

/// Python module definition.
#[pymodule]
fn esoteric_entropy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", esoteric_core::VERSION)?;
    m.add_class::<PyEntropyPool>()?;
    m.add_function(wrap_pyfunction!(run_all_tests, m)?)?;
    m.add_function(wrap_pyfunction!(calculate_quality_score, m)?)?;
    m.add_function(wrap_pyfunction!(detect_available_sources, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
