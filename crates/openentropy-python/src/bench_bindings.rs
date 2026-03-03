use pyo3::prelude::*;
use pyo3::types::PyDict;
use pythonize::pythonize;

use openentropy_core::benchmark::{BenchConfig, RankBy, benchmark_sources as bench_sources};

use super::PyEntropyPool;

#[pyfunction]
#[pyo3(signature = (pool, config=None))]
fn benchmark_sources(
    py: Python<'_>,
    pool: &PyEntropyPool,
    config: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let mut bench_config = BenchConfig::default();

    if let Some(d) = config {
        if let Ok(Some(v)) = d.get_item("samples_per_round") {
            bench_config.samples_per_round = v.extract::<usize>()?;
        }
        if let Ok(Some(v)) = d.get_item("rounds") {
            bench_config.rounds = v.extract::<usize>()?;
        }
        if let Ok(Some(v)) = d.get_item("warmup_rounds") {
            bench_config.warmup_rounds = v.extract::<usize>()?;
        }
        if let Ok(Some(v)) = d.get_item("timeout_sec") {
            bench_config.timeout_sec = v.extract::<f64>()?;
        }
        if let Ok(Some(v)) = d.get_item("rank_by") {
            let s = v.extract::<String>()?;
            bench_config.rank_by = match s.as_str() {
                "min_entropy" => RankBy::MinEntropy,
                "throughput" => RankBy::Throughput,
                _ => RankBy::Balanced,
            };
        }
        if let Ok(Some(v)) = d.get_item("include_pool_quality") {
            bench_config.include_pool_quality = v.extract::<bool>()?;
        }
        if let Ok(Some(v)) = d.get_item("pool_quality_bytes") {
            bench_config.pool_quality_bytes = v.extract::<usize>()?;
        }
        if let Ok(Some(v)) = d.get_item("conditioning") {
            let s = v.extract::<String>()?;
            bench_config.conditioning = super::parse_conditioning_mode(&s)?;
        }
    }

    let inner = &pool.inner;
    let result = py.allow_threads(|| bench_sources(inner, &bench_config));

    match result {
        Ok(report) => pythonize(py, &report)
            .map(|obj| obj.unbind())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string())),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
    }
}

#[pyfunction]
fn bench_config_defaults(py: Python<'_>) -> PyResult<PyObject> {
    let config = BenchConfig::default();
    pythonize(py, &config)
        .map(|obj| obj.unbind())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(benchmark_sources, m)?)?;
    m.add_function(wrap_pyfunction!(bench_config_defaults, m)?)?;
    Ok(())
}
