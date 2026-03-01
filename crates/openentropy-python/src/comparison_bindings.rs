use pyo3::prelude::*;
use pythonize::pythonize;

#[pyfunction]
fn compare(
    py: Python<'_>,
    label_a: &str,
    data_a: &[u8],
    label_b: &str,
    data_b: &[u8],
) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::compare(label_a, data_a, label_b, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn aggregate_delta(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::aggregate_delta(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn two_sample_tests(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::two_sample_tests(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn cliffs_delta(data_a: &[u8], data_b: &[u8]) -> f64 {
    openentropy_core::comparison::cliffs_delta(data_a, data_b)
}

#[pyfunction]
#[pyo3(signature = (data_a, data_b, window_size=1024, z_threshold=3.0))]
fn temporal_analysis(
    py: Python<'_>,
    data_a: &[u8],
    data_b: &[u8],
    window_size: usize,
    z_threshold: f64,
) -> PyResult<PyObject> {
    let result =
        openentropy_core::comparison::temporal_analysis(data_a, data_b, window_size, z_threshold);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn digram_analysis(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::digram_analysis(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn markov_analysis(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::markov_analysis(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn multi_lag_analysis(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::multi_lag_analysis(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn run_length_comparison(py: Python<'_>, data_a: &[u8], data_b: &[u8]) -> PyResult<PyObject> {
    let result = openentropy_core::comparison::run_length_comparison(data_a, data_b);
    pythonize(py, &result)
        .map(Bound::unbind)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compare, m)?)?;
    m.add_function(wrap_pyfunction!(aggregate_delta, m)?)?;
    m.add_function(wrap_pyfunction!(two_sample_tests, m)?)?;
    m.add_function(wrap_pyfunction!(cliffs_delta, m)?)?;
    m.add_function(wrap_pyfunction!(temporal_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(digram_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(markov_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(multi_lag_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(run_length_comparison, m)?)?;
    Ok(())
}
