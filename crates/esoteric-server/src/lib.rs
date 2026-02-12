//! HTTP entropy server — ANU QRNG API compatible.
//!
//! Serves random bytes via HTTP, compatible with quantum-llama.cpp's
//! QRNG backend and any client expecting the ANU API format.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    response::Json,
    routing::get,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use esoteric_core::pool::EntropyPool;

/// Shared server state.
struct AppState {
    pool: Mutex<EntropyPool>,
}

#[derive(Deserialize)]
struct RandomParams {
    length: Option<usize>,
    #[serde(rename = "type")]
    data_type: Option<String>,
}

#[derive(Serialize)]
struct RandomResponse {
    #[serde(rename = "type")]
    data_type: String,
    length: usize,
    data: serde_json::Value,
    success: bool,
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
) -> Json<RandomResponse> {
    let length = params.length.unwrap_or(1024).max(1).min(65536);
    let data_type = params.data_type.unwrap_or_else(|| "hex16".to_string());

    let pool = state.pool.lock().await;
    let raw = pool.get_random_bytes(length);

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

    Json(RandomResponse {
        data_type,
        length: len,
        data,
        success: true,
    })
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

async fn handle_pool_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
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

/// Build the axum router.
fn build_router(pool: EntropyPool) -> Router {
    let state = Arc::new(AppState {
        pool: Mutex::new(pool),
    });

    Router::new()
        .route("/api/v1/random", get(handle_random))
        .route("/health", get(handle_health))
        .route("/sources", get(handle_sources))
        .route("/pool/status", get(handle_pool_status))
        .with_state(state)
}

/// Run the HTTP entropy server.
pub async fn run_server(pool: EntropyPool, host: &str, port: u16) {
    let app = build_router(pool);
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
