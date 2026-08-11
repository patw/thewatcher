//! HTTP API route handlers for TheWatcher.

use crate::model::*;
use crate::storage::Storage;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use bson::Document;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// Shared application state
pub struct AppState {
    pub storage: Storage,
    pub config: crate::config::Config,
    pub current_snapshot: RwLock<Option<CurrentSnapshot>>,
    pub start_time_ms: i64,
    pub last_collection_ms: RwLock<Option<i64>>,
    pub last_rollup_ms: RwLock<Option<i64>>,
    pub boot_time_ms: Option<i64>,
    pub host_os: String,
    pub host_arch: String,
}

impl AppState {
    pub fn new(storage: Storage, config: crate::config::Config) -> Self {
        Self {
            storage,
            config,
            current_snapshot: RwLock::new(None),
            start_time_ms: chrono::Utc::now().timestamp_millis(),
            last_collection_ms: RwLock::new(None),
            last_rollup_ms: RwLock::new(None),
            boot_time_ms: get_boot_time_ms(),
            host_os: std::env::consts::OS.to_string(),
            host_arch: std::env::consts::ARCH.to_string(),
        }
    }
}

fn get_boot_time_ms() -> Option<i64> {
    let uptime = sysinfo::System::uptime();
    let now_ms = chrono::Utc::now().timestamp_millis();
    Some(now_ms - (uptime as i64 * 1000))
}

// ---------------------------------------------------------------------------
// GET /api/current
// ---------------------------------------------------------------------------

pub async fn api_current(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CurrentSnapshot>, (StatusCode, String)> {
    let snapshot = state.current_snapshot.read().await;
    match &*snapshot {
        Some(s) => Ok(Json(s.clone())),
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "No metrics collected yet".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// GET /api/history
// ---------------------------------------------------------------------------

pub async fn api_history(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, (StatusCode, String)> {
    // Validate metric
    let valid_metrics = ["cpu", "memory", "disk", "network", "sockets"];
    if !valid_metrics.contains(&params.metric.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown metric: {}. Valid: {:?}",
                params.metric, valid_metrics
            ),
        ));
    }

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Parse range
    let (from_ms, until_ms) = if let Some(range_str) = &params.range {
        let duration_ms = parse_range_ms(range_str).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid range: {}. Use e.g. 1h, 24h, 7d, 30d, 1y", range_str),
            )
        })?;
        (now_ms - duration_ms, now_ms)
    } else {
        let from = params.from.unwrap_or(now_ms - 3_600_000); // default 1h
        let until = params.until.unwrap_or(now_ms);
        (from, until)
    };

    if from_ms >= until_ms {
        return Err((
            StatusCode::BAD_REQUEST,
            "from must be before until".to_string(),
        ));
    }

    // Determine resolution
    let duration_ms = until_ms - from_ms;
    let resolution = if let Some(ref res_str) = params.resolution {
        Resolution::from_str(res_str).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid resolution: {}", res_str),
            )
        })?
    } else {
        Resolution::auto_select(duration_ms)
    };

    // Fetch data
    let docs: Vec<Document> = match resolution {
        Resolution::Granular => {
            state
                .storage
                .query_granular(
                    &params.metric,
                    from_ms,
                    until_ms,
                    params.interface.as_deref(),
                    params.mount.as_deref(),
                )
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        }
        _ => state
            .storage
            .query_rollup(resolution.as_str(), &params.metric, from_ms, until_ms)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?,
    };

    // Limit results
    let max_points = 10_000usize;
    let docs: Vec<&Document> = if docs.len() > max_points {
        let step = docs.len() / max_points;
        docs.iter()
            .step_by(step.max(1))
            .take(max_points)
            .collect()
    } else {
        docs.iter().collect()
    };

    // Build response series
    let response = build_history_response(
        &params.metric,
        resolution.as_str(),
        from_ms,
        until_ms,
        &docs,
    );

    Ok(Json(response))
}

fn build_history_response(
    metric: &str,
    resolution: &str,
    from_ms: i64,
    until_ms: i64,
    docs: &[&Document],
) -> HistoryResponse {
    let mut series = Vec::new();

    match metric {
        "cpu" => {
            series.push(build_series_cpu(docs, resolution));
            series.push(build_series_load(docs, resolution));
        }
        "memory" => {
            series.push(build_series_memory(docs, resolution));
        }
        "disk" => {
            series.push(build_series_disk(docs, resolution));
        }
        "network" => {
            series.push(build_series_net_rx(docs, resolution));
            series.push(build_series_net_tx(docs, resolution));
        }
        "sockets" => {
            series.push(build_series_sockets(docs, resolution, "process_count", "processes", "count"));
            series.push(build_series_sockets(docs, resolution, "tcp_inuse", "TCP", "count"));
        }
        _ => {}
    }

    HistoryResponse {
        metric: metric.to_string(),
        resolution: resolution.to_string(),
        from_ms,
        until_ms,
        series,
    }
}

fn build_series_cpu(docs: &[&Document], resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            if resolution == "granular" {
                d.get_f64("cpu_percent").ok().map(|v| HistoryPoint {
                    timestamp_ms: ts,
                    min: None,
                    mean: None,
                    max: None,
                    value: Some(v),
                })
            } else {
                let mean = d.get_f64("cpu_mean").ok();
                let min = d.get_f64("cpu_min").ok();
                let max = d.get_f64("cpu_max").ok();
                if mean.is_some() || min.is_some() {
                    Some(HistoryPoint {
                        timestamp_ms: ts,
                        min,
                        mean,
                        max,
                        value: None,
                    })
                } else {
                    None
                }
            }
        })
        .collect();

    HistorySeries {
        name: "cpu_percent".to_string(),
        unit: "percent".to_string(),
        points,
    }
}

fn build_series_load(docs: &[&Document], resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            if resolution == "granular" {
                d.get_f64("load_1").ok().map(|v| HistoryPoint {
                    timestamp_ms: ts,
                    min: None,
                    mean: None,
                    max: None,
                    value: Some(v),
                })
            } else {
                let mean = d.get_f64("load_1_mean").ok();
                let min = d.get_f64("load_1_min").ok();
                let max = d.get_f64("load_1_max").ok();
                if mean.is_some() || min.is_some() {
                    Some(HistoryPoint {
                        timestamp_ms: ts,
                        min,
                        mean,
                        max,
                        value: None,
                    })
                } else {
                    None
                }
            }
        })
        .collect();

    HistorySeries {
        name: "load_1".to_string(),
        unit: "load".to_string(),
        points,
    }
}

fn build_series_memory(docs: &[&Document], resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            if resolution == "granular" {
                d.get_f64("used_percent").ok().map(|v| HistoryPoint {
                    timestamp_ms: ts,
                    min: None,
                    mean: None,
                    max: None,
                    value: Some(v),
                })
            } else {
                let mean = d.get_f64("mem_used_mean").ok();
                let min = d.get_f64("mem_used_min").ok();
                let max = d.get_f64("mem_used_max").ok();
                if mean.is_some() || min.is_some() {
                    Some(HistoryPoint {
                        timestamp_ms: ts,
                        min,
                        mean,
                        max,
                        value: None,
                    })
                } else {
                    None
                }
            }
        })
        .collect();

    HistorySeries {
        name: "used_percent".to_string(),
        unit: "percent".to_string(),
        points,
    }
}

fn build_series_disk(docs: &[&Document], resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            if resolution == "granular" {
                d.get_f64("used_percent").ok().map(|v| HistoryPoint {
                    timestamp_ms: ts,
                    min: None,
                    mean: None,
                    max: None,
                    value: Some(v),
                })
            } else {
                let mean = d.get_f64("mem_used_mean").ok();
                let min = d.get_f64("mem_used_min").ok();
                let max = d.get_f64("mem_used_max").ok();
                if mean.is_some() || min.is_some() {
                    Some(HistoryPoint {
                        timestamp_ms: ts,
                        min,
                        mean,
                        max,
                        value: None,
                    })
                } else {
                    None
                }
            }
        })
        .collect();

    HistorySeries {
        name: "used_percent".to_string(),
        unit: "percent".to_string(),
        points,
    }
}

fn build_series_net_rx(docs: &[&Document], _resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            d.get_f64("rx_bytes_per_sec").ok().map(|v| HistoryPoint {
                timestamp_ms: ts,
                min: None,
                mean: None,
                max: None,
                value: Some(v),
            })
        })
        .collect();

    HistorySeries {
        name: "rx_bytes_per_sec".to_string(),
        unit: "bytes/sec".to_string(),
        points,
    }
}

fn build_series_net_tx(docs: &[&Document], _resolution: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            d.get_f64("tx_bytes_per_sec").ok().map(|v| HistoryPoint {
                timestamp_ms: ts,
                min: None,
                mean: None,
                max: None,
                value: Some(v),
            })
        })
        .collect();

    HistorySeries {
        name: "tx_bytes_per_sec".to_string(),
        unit: "bytes/sec".to_string(),
        points,
    }
}

fn build_series_sockets(docs: &[&Document], resolution: &str, field: &str, name: &str, unit: &str) -> HistorySeries {
    let points: Vec<HistoryPoint> = docs
        .iter()
        .filter_map(|d| {
            let ts = d.get_i64("timestamp_ms").ok()?;
            if resolution == "granular" {
                d.get_i32(field).ok().map(|v| HistoryPoint {
                    timestamp_ms: ts,
                    min: None,
                    mean: None,
                    max: None,
                    value: Some(v as f64),
                })
                    .or_else(|| {
                        d.get_i64(field).ok().map(|v| HistoryPoint {
                            timestamp_ms: ts,
                            min: None,
                            mean: None,
                            max: None,
                            value: Some(v as f64),
                        })
                    })
            } else {
                None
            }
        })
        .collect();

    HistorySeries {
        name: name.to_string(),
        unit: unit.to_string(),
        points,
    }
}

// ---------------------------------------------------------------------------
// GET /api/health
// ---------------------------------------------------------------------------

pub async fn api_health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthResponse>, (StatusCode, String)> {
    let last_collection = *state.last_collection_ms.read().await;
    let last_rollup = *state.last_rollup_ms.read().await;

    // Check storage is accessible
    let storage_status = match state.storage.stats() {
        Ok(_) => "ok".to_string(),
        Err(e) => {
            tracing::error!("Storage health check failed: {}", e);
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Storage unavailable: {}", e),
            ));
        }
    };

    let status = if storage_status == "ok" {
        "ok".to_string()
    } else {
        "degraded".to_string()
    };

    Ok(Json(HealthResponse {
        status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        last_collection_ms: last_collection,
        last_rollup_ms: last_rollup,
        storage: storage_status,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/info
// ---------------------------------------------------------------------------

pub async fn api_info(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        hostname: sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os: state.host_os.clone(),
        arch: state.host_arch.clone(),
        start_time_ms: state.start_time_ms,
        boot_time_ms: state.boot_time_ms,
        data_dir: state.storage.data_dir().display().to_string(),
        listen_addr: state.config.listener_addr(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_range_ms(range: &str) -> Option<i64> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    let (num_str, unit) = range.split_at(range.len() - 1);
    let num: i64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(num * 1000),
        "m" => Some(num * 60 * 1000),
        "h" => Some(num * 3600 * 1000),
        "d" => Some(num * 86400 * 1000),
        "w" => Some(num * 7 * 86400 * 1000),
        "y" => Some(num * 365 * 86400 * 1000),
        _ => None,
    }
}
