use pyo3::prelude::*;
use pythonize::pythonize;

/// Run all 5 chaos theory analyses on a byte stream.
///
/// Returns a dict with keys: hurst, lyapunov, correlation_dimension,
/// bientropy, epiplexity — each containing the analysis results.
#[pyfunction]
fn chaos_analysis(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::chaos_analysis(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Hurst exponent via R/S analysis.
///
/// H ≈ 0.5 indicates random walk (no long-range dependence).
#[pyfunction]
fn hurst_exponent(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::hurst_exponent(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Lyapunov exponent via Rosenstein algorithm.
///
/// λ ≈ 0 indicates no deterministic chaos.
#[pyfunction]
fn lyapunov_exponent(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::lyapunov_exponent(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Correlation dimension via Grassberger-Procaccia algorithm.
///
/// High D₂ indicates high-dimensional (random) attractor.
#[pyfunction]
fn correlation_dimension(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::correlation_dimension(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// BiEntropy / TBiEntropy binary derivative analysis.
///
/// High values indicate maximal binary entropy.
#[pyfunction]
fn bientropy(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::bientropy(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Epiplexity: compression-based structure detection.
///
/// Compression ratio ≈ 1.0 means incompressible (random).
#[pyfunction]
fn epiplexity(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::chaos::epiplexity(data);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(chaos_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(hurst_exponent, m)?)?;
    m.add_function(wrap_pyfunction!(lyapunov_exponent, m)?)?;
    m.add_function(wrap_pyfunction!(correlation_dimension, m)?)?;
    m.add_function(wrap_pyfunction!(bientropy, m)?)?;
    m.add_function(wrap_pyfunction!(epiplexity, m)?)?;
    Ok(())
}
