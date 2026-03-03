//! Python bindings for session recording.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use openentropy_core::conditioning::condition;
use openentropy_core::session::{SessionConfig, SessionWriter};

use super::PyEntropyPool;

// ---------------------------------------------------------------------------
// PySessionWriter class
// ---------------------------------------------------------------------------

/// Session writer for recording entropy samples to disk.
#[pyclass(name = "SessionWriter")]
pub struct PySessionWriter {
    inner: Option<SessionWriter>,
}

#[pymethods]
impl PySessionWriter {
    #[new]
    #[pyo3(signature = (sources, output_dir, conditioning="raw", tags=None, note=None, analyze=false))]
    fn new(
        sources: Vec<String>,
        output_dir: &str,
        conditioning: &str,
        tags: Option<HashMap<String, String>>,
        note: Option<String>,
        analyze: bool,
    ) -> PyResult<Self> {
        let mode = super::parse_conditioning_mode(conditioning)?;

        let config = SessionConfig {
            sources,
            conditioning: mode,
            output_dir: PathBuf::from(output_dir),
            tags: tags.unwrap_or_default(),
            note,
            sample_size: 1000,
            include_analysis: analyze,
            ..Default::default()
        };

        let writer =
            SessionWriter::new(config).map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(Self {
            inner: Some(writer),
        })
    }

    /// Write a single sample from a named source.
    fn write_sample(&mut self, source_name: &str, raw: &[u8], conditioned: &[u8]) -> PyResult<()> {
        let writer = self
            .inner
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("session already finished"))?;
        writer
            .write_sample(source_name, raw, conditioned)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Finalize the session and return the session directory path.
    fn finish(&mut self) -> PyResult<String> {
        let writer = self
            .inner
            .take()
            .ok_or_else(|| PyValueError::new_err("session already finished"))?;
        let path = writer
            .finish()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Total samples recorded so far.
    fn total_samples(&self) -> PyResult<u64> {
        let writer = self
            .inner
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("session already finished"))?;
        Ok(writer.total_samples())
    }

    /// Elapsed seconds since recording started.
    fn elapsed_secs(&self) -> PyResult<f64> {
        let writer = self
            .inner
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("session already finished"))?;
        Ok(writer.elapsed().as_secs_f64())
    }

    /// Path to the session directory.
    fn session_dir(&self) -> PyResult<String> {
        let writer = self
            .inner
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("session already finished"))?;
        Ok(writer.session_dir().to_string_lossy().to_string())
    }
}

// ---------------------------------------------------------------------------
// record() convenience function
// ---------------------------------------------------------------------------

/// Record entropy from a pool for a given duration, returning session metadata as a dict.
#[pyfunction]
#[pyo3(signature = (pool, sources, duration_secs, conditioning="raw", output_dir="sessions", analyze=false))]
fn record(
    py: Python<'_>,
    pool: &PyEntropyPool,
    sources: Vec<String>,
    duration_secs: f64,
    conditioning: &str,
    output_dir: &str,
    analyze: bool,
) -> PyResult<PyObject> {
    let mode = super::parse_conditioning_mode(conditioning)?;

    let config = SessionConfig {
        sources: sources.clone(),
        conditioning: mode,
        output_dir: PathBuf::from(output_dir),
        sample_size: 1000,
        include_analysis: analyze,
        ..Default::default()
    };

    let writer = SessionWriter::new(config).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let pool_ref = &pool.inner;

    let result = py.allow_threads(|| -> Result<PathBuf, std::io::Error> {
        let mut writer = writer;
        let start = Instant::now();
        let max_dur = Duration::from_secs_f64(duration_secs);

        while start.elapsed() < max_dur {
            for source in &sources {
                if start.elapsed() >= max_dur {
                    break;
                }
                let raw = pool_ref
                    .get_source_raw_bytes(source, 1000)
                    .unwrap_or_default();
                if raw.is_empty() {
                    continue;
                }
                let conditioned = condition(&raw, raw.len(), mode);
                writer.write_sample(source, &raw, &conditioned)?;
            }
        }

        writer.finish()
    });

    let session_dir = result.map_err(|e| PyValueError::new_err(e.to_string()))?;

    let json_str = std::fs::read_to_string(session_dir.join("session.json"))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let json_module = py.import("json")?;
    let parsed = json_module.call_method1("loads", (json_str,))?;
    Ok(parsed.into())
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySessionWriter>()?;
    m.add_function(wrap_pyfunction!(record, m)?)?;
    Ok(())
}
