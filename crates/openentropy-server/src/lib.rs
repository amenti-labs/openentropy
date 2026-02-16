//! HTTP entropy server — ANU QRNG API compatible.
//!
//! Serves random bytes via HTTP, compatible with the ANU QRNG API format for easy integration with
//! QRNG backend and any client expecting the ANU API format.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::pool::EntropyPool;

/// Shared server state.
struct AppState {
    pool: Mutex<EntropyPool>,
    allow_raw: bool,
}

#[derive(Deserialize)]
struct RandomParams {
    length: Option<usize>,
    #[serde(rename = "type")]
    data_type: Option<String>,
    /// If true, return raw unconditioned entropy (no SHA-256/DRBG).
    raw: Option<bool>,
    /// Conditioning mode: raw, vonneumann, sha256 (overrides `raw` flag).
    conditioning: Option<String>,
    /// Request entropy from a specific source by name.
    source: Option<String>,
}

#[derive(Serialize)]
struct RandomResponse {
    #[serde(rename = "type")]
    data_type: String,
    length: usize,
    data: serde_json::Value,
    success: bool,
    /// Whether this output was conditioned (SHA-256) or raw.
    conditioned: bool,
    /// Which source was queried (null if mixed pool).
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    /// Error message if request failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    sources_healthy: usize,
    sources_total: usize,
    raw_bytes: u64,
    output_bytes: u64,
}

#[derive(Serialize)]
struct SourcesResponse {
    sources: Vec<SourceEntry>,
    total: usize,
}

#[derive(Serialize)]
struct SourceEntry {
    name: String,
    healthy: bool,
    bytes: u64,
    entropy: f64,
    time: f64,
    failures: u64,
}

async fn handle_random(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RandomParams>,
) -> (StatusCode, Json<RandomResponse>) {
    let length = params.length.unwrap_or(1024).clamp(1, 65536);
    let data_type = params.data_type.unwrap_or_else(|| "hex16".to_string());

    // Determine conditioning mode: ?conditioning= takes priority, then ?raw=true
    let mode = if let Some(ref c) = params.conditioning {
        match c.as_str() {
            "raw" if state.allow_raw => ConditioningMode::Raw,
            "vonneumann" | "von_neumann" | "vn" => ConditioningMode::VonNeumann,
            "raw" => ConditioningMode::Sha256, // raw not allowed
            _ => ConditioningMode::Sha256,
        }
    } else if params.raw.unwrap_or(false) && state.allow_raw {
        ConditioningMode::Raw
    } else {
        ConditioningMode::Sha256
    };

    let pool = state.pool.lock().await;
    let raw = if let Some(ref source_name) = params.source {
        match pool.get_source_bytes(source_name, length, mode) {
            Some(bytes) => bytes,
            None => {
                let err_msg = format!(
                    "Unknown source: {source_name}. Use /sources to list available sources."
                );
                return Json(RandomResponse {
                    data_type,
                    length: 0,
                    data: serde_json::Value::Array(vec![]),
                    success: false,
                    conditioned: mode != ConditioningMode::Raw,
                    source: Some(source_name.clone()),
                    error: Some(err_msg),
                })
                .with_status(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        pool.get_bytes(length, mode)
    };
    let use_raw = mode == ConditioningMode::Raw;

    let data = match data_type.as_str() {
        "hex16" => {
            let hex_pairs: Vec<String> = raw
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
                .collect();
            serde_json::Value::Array(
                hex_pairs
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            )
        }
        "uint8" => {
            serde_json::Value::Array(raw.iter().map(|&b| serde_json::Value::from(b)).collect())
        }
        "uint16" => {
            let vals: Vec<u16> = raw
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            serde_json::Value::Array(vals.into_iter().map(serde_json::Value::from).collect())
        }
        _ => serde_json::Value::String(hex::encode(&raw)),
    };

    let len = match &data {
        serde_json::Value::Array(a) => a.len(),
        _ => length,
    };

    (
        StatusCode::OK,
        Json(RandomResponse {
            data_type,
            length: len,
            data,
            success: true,
            conditioned: !use_raw,
            source: params.source,
            error: None,
        }),
    )
}

trait JsonWithStatus<T> {
    fn with_status(self, status: StatusCode) -> (StatusCode, Json<T>);
}

impl<T> JsonWithStatus<T> for Json<T> {
    fn with_status(self, status: StatusCode) -> (StatusCode, Json<T>) {
        (status, self)
    }
}

async fn handle_health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let pool = state.pool.lock().await;
    let report = pool.health_report();
    Json(HealthResponse {
        status: if report.healthy > 0 {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        sources_healthy: report.healthy,
        sources_total: report.total,
        raw_bytes: report.raw_bytes,
        output_bytes: report.output_bytes,
    })
}

async fn handle_sources(State(state): State<Arc<AppState>>) -> Json<SourcesResponse> {
    let pool = state.pool.lock().await;
    let report = pool.health_report();
    let sources: Vec<SourceEntry> = report
        .sources
        .iter()
        .map(|s| SourceEntry {
            name: s.name.clone(),
            healthy: s.healthy,
            bytes: s.bytes,
            entropy: s.entropy,
            time: s.time,
            failures: s.failures,
        })
        .collect();
    let total = sources.len();
    Json(SourcesResponse { sources, total })
}

async fn handle_pool_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let pool = state.pool.lock().await;
    let report = pool.health_report();
    Json(serde_json::json!({
        "healthy": report.healthy,
        "total": report.total,
        "raw_bytes": report.raw_bytes,
        "output_bytes": report.output_bytes,
        "buffer_size": report.buffer_size,
        "sources": report.sources.iter().map(|s| serde_json::json!({
            "name": s.name,
            "healthy": s.healthy,
            "bytes": s.bytes,
            "entropy": s.entropy,
            "time": s.time,
            "failures": s.failures,
        })).collect::<Vec<_>>(),
    }))
}

async fn handle_index(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let pool = state.pool.lock().await;
    let source_names = pool.source_names();
    drop(pool);

    Json(serde_json::json!({
        "name": "OpenEntropy Server",
        "version": openentropy_core::VERSION,
        "sources": source_names.len(),
        "endpoints": {
            "/": "This API index",
            "/api/v1/random": {
                "method": "GET",
                "description": "Get random entropy bytes",
                "params": {
                    "length": "Number of bytes (1-65536, default: 1024)",
                    "type": "Output format: hex16, uint8, uint16 (default: hex16)",
                    "source": format!("Request from a specific source by name. Available: {}", source_names.join(", ")),
                    "conditioning": "Conditioning mode: sha256 (default), vonneumann, raw",
                }
            },
            "/sources": "List all active entropy sources with health metrics",
            "/pool/status": "Detailed pool status",
            "/health": "Health check",
        },
        "examples": {
            "mixed_pool": "/api/v1/random?length=32&type=uint8",
            "single_source": format!("/api/v1/random?length=32&source={}", source_names.first().map(|s| s.as_str()).unwrap_or("clock_jitter")),
            "raw_output": "/api/v1/random?length=32&conditioning=raw",
        }
    }))
}

/// Build the axum router.
fn build_router(pool: EntropyPool, allow_raw: bool) -> Router {
    let state = Arc::new(AppState {
        pool: Mutex::new(pool),
        allow_raw,
    });

    Router::new()
        .route("/", get(handle_index))
        .route("/api/v1/random", get(handle_random))
        .route("/health", get(handle_health))
        .route("/sources", get(handle_sources))
        .route("/pool/status", get(handle_pool_status))
        .with_state(state)
}

/// Run the HTTP entropy server.
pub async fn run_server(pool: EntropyPool, host: &str, port: u16, allow_raw: bool) {
    let app = build_router(pool, allow_raw);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Simple hex encoding without external dep
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
}
