use pyo3::prelude::*;
use pyo3::types::PyList;
use pythonize::{depythonize, pythonize};

#[pyfunction]
#[pyo3(signature = (data, bits_per_trial=200))]
fn trial_analysis(py: Python<'_>, data: &[u8], bits_per_trial: usize) -> PyResult<PyObject> {
    let config = openentropy_core::trials::TrialConfig { bits_per_trial };
    let result = openentropy_core::trials::trial_analysis(data, &config);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn stouffer_combine(py: Python<'_>, analyses: &Bound<'_, PyList>) -> PyResult<PyObject> {
    let mut trial_analyses: Vec<openentropy_core::trials::TrialAnalysis> = Vec::new();
    for item in analyses.iter() {
        let ta: openentropy_core::trials::TrialAnalysis = depythonize(&item)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        trial_analyses.push(ta);
    }
    let refs: Vec<&openentropy_core::trials::TrialAnalysis> = trial_analyses.iter().collect();
    let result = openentropy_core::trials::stouffer_combine(&refs);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(signature = (data, bits_per_trial=200))]
fn calibration_check(py: Python<'_>, data: &[u8], bits_per_trial: usize) -> PyResult<PyObject> {
    let config = openentropy_core::trials::TrialConfig { bits_per_trial };
    let result = openentropy_core::trials::calibration_check(data, &config);
    pythonize(py, &result)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(trial_analysis, m)?)?;
    m.add_function(wrap_pyfunction!(stouffer_combine, m)?)?;
    m.add_function(wrap_pyfunction!(calibration_check, m)?)?;
    Ok(())
}
