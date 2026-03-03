use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use pythonize::pythonize;

#[pyfunction]
fn list_sessions(py: Python<'_>, dir: &str) -> PyResult<PyObject> {
    let sessions = openentropy_core::session::list_sessions(std::path::Path::new(dir))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let list = PyList::empty(py);
    for (path, meta) in &sessions {
        let obj = pythonize(py, meta)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let dict = obj.downcast::<PyDict>()?;
        dict.set_item("path", path.to_string_lossy().to_string())?;
        list.append(dict)?;
    }
    Ok(list.unbind().into_any())
}

#[pyfunction]
fn load_session_meta(py: Python<'_>, session_dir: &str) -> PyResult<PyObject> {
    let path = std::path::Path::new(session_dir);
    let json_path = path.join("session.json");
    let contents = std::fs::read_to_string(&json_path)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let meta: openentropy_core::session::SessionMeta = serde_json::from_str(&contents)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    pythonize(py, &meta)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn load_session_raw_data(py: Python<'_>, session_dir: &str) -> PyResult<PyObject> {
    let data = openentropy_core::session::load_session_raw_data(std::path::Path::new(session_dir))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let py_dict = PyDict::new(py);
    for (k, v) in &data {
        py_dict.set_item(k, PyBytes::new(py, v))?;
    }
    Ok(py_dict.unbind().into_any())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(list_sessions, m)?)?;
    m.add_function(wrap_pyfunction!(load_session_meta, m)?)?;
    m.add_function(wrap_pyfunction!(load_session_raw_data, m)?)?;
    Ok(())
}
