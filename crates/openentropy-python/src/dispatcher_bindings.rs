use pyo3::prelude::*;
use pyo3::types::PyDict;
use pythonize::pythonize;

use openentropy_core::dispatcher::{AnalysisConfig, AnalysisProfile};
use openentropy_core::trials::TrialConfig;

#[pyfunction]
#[pyo3(signature = (sources, config=None, profile=None))]
fn analyze(
    py: Python<'_>,
    sources: Vec<(String, Vec<u8>)>,
    config: Option<&Bound<'_, PyDict>>,
    profile: Option<&str>,
) -> PyResult<PyObject> {
    let analysis_config = if let Some(dict) = config {
        AnalysisConfig {
            forensic: extract_bool(dict, "forensic", true)?,
            entropy: extract_bool(dict, "entropy", false)?,
            chaos: extract_bool(dict, "chaos", false)?,
            trials: if extract_bool(dict, "trials", false)? {
                Some(TrialConfig::default())
            } else {
                None
            },
            cross_correlation: extract_bool(dict, "cross_correlation", false)?,
        }
    } else if let Some(p) = profile {
        AnalysisProfile::parse(p).to_config()
    } else {
        AnalysisConfig::default()
    };

    let source_refs: Vec<(&str, &[u8])> = sources
        .iter()
        .map(|(l, d)| (l.as_str(), d.as_slice()))
        .collect();
    let report = openentropy_core::dispatcher::analyze(&source_refs, &analysis_config);

    pythonize(py, &report)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(signature = (profile=None))]
fn analysis_config(py: Python<'_>, profile: Option<&str>) -> PyResult<PyObject> {
    let config = if let Some(p) = profile {
        AnalysisProfile::parse(p).to_config()
    } else {
        AnalysisConfig::default()
    };
    pythonize(py, &config)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

fn extract_bool(dict: &Bound<'_, PyDict>, key: &str, default: bool) -> PyResult<bool> {
    match dict.get_item(key)? {
        Some(v) => v.extract::<bool>(),
        None => Ok(default),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    m.add_function(wrap_pyfunction!(analysis_config, m)?)?;
    Ok(())
}
