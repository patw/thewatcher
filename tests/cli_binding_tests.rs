//! CLI parsing and binding tests for TheWatcher.

use std::sync::Arc;
use tower::util::ServiceExt;

use clap::Parser;
use thewatcher::{api, cli::Cli, config, server, storage};

// ---------------------------------------------------------------------------
// CLI parsing tests
// ---------------------------------------------------------------------------

#[test]
fn test_default_config() {
    let config = config::Config::default();
    assert_eq!(config.listen, "127.0.0.1");
    assert_eq!(config.port, 8080);
    assert_eq!(config.interval_secs, 30);
    assert!(config.is_loopback());
    assert!(!config.is_all_interfaces());
}

#[test]
fn test_listener_addr() {
    let mut config = config::Config::default();
    config.listen = "127.0.0.1".to_string();
    config.port = 8080;
    assert_eq!(config.listener_addr(), "127.0.0.1:8080");
}

#[test]
fn test_loopback_detection() {
    for addr in &["127.0.0.1", "::1", "localhost"] {
        let cfg = config::Config {
            listen: addr.to_string(),
            ..Default::default()
        };
        assert!(cfg.is_loopback(), "{} should be loopback", addr);
    }
}

#[test]
fn test_all_interfaces_detection() {
    assert!(config::Config {
        listen: "0.0.0.0".to_string(),
        ..Default::default()
    }
    .is_all_interfaces());

    assert!(config::Config {
        listen: "::".to_string(),
        ..Default::default()
    }
    .is_all_interfaces());
}

#[test]
fn test_non_loopback_management_address() {
    let cfg = config::Config {
        listen: "192.168.10.25".to_string(),
        ..Default::default()
    };
    assert!(!cfg.is_loopback());
    assert!(!cfg.is_all_interfaces());
}

// ---------------------------------------------------------------------------
// CLI duration parsing
// ---------------------------------------------------------------------------

#[test]
fn test_cli_parse_duration_secs() {
    assert_eq!(
        Cli::try_parse_from(["thewatcher", "--interval", "5s"])
            .unwrap()
            .into_config()
            .unwrap()
            .interval_secs,
        5
    );
    assert_eq!(
        Cli::try_parse_from(["thewatcher", "--interval", "2m"])
            .unwrap()
            .into_config()
            .unwrap()
            .interval_secs,
        120
    );
    assert_eq!(
        Cli::try_parse_from(["thewatcher", "--interval", "1h"])
            .unwrap()
            .into_config()
            .unwrap()
            .interval_secs,
        3600
    );
}

#[test]
fn test_cli_rejects_below_1s() {
    let result = Cli::try_parse_from(["thewatcher", "--interval", "0s"])
        .unwrap()
        .into_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("at least 1 second"));
}

#[test]
fn test_cli_rejects_invalid_interval() {
    assert!(Cli::try_parse_from(["thewatcher", "--interval", "xyz"])
        .unwrap()
        .into_config()
        .is_err());
}

#[test]
fn test_cli_parse_retention() {
    let config = Cli::try_parse_from([
        "thewatcher",
        "--granular-retention", "30d",
        "--hourly-retention", "365d",
        "--daily-retention", "5y",
        "--monthly-retention", "10y",
        "--yearly-retention", "0",
    ])
    .unwrap()
    .into_config()
    .unwrap();

    assert_eq!(config.granular_retention_days, 30);
    assert_eq!(config.hourly_retention_days, 365);
    assert_eq!(config.daily_retention_days, 1825); // 5 * 365
    assert_eq!(config.monthly_retention_days, 3650); // 10 * 365
    assert_eq!(config.yearly_retention_days, 0); // indefinite
}

#[test]
fn test_cli_default_listen_port() {
    let cli = Cli::try_parse_from(["thewatcher"]).unwrap();
    assert_eq!(cli.listen, "127.0.0.1");
    assert_eq!(cli.port, 8080);
}

// ---------------------------------------------------------------------------
// Binding tests - verify the server can be built with different configs
// ---------------------------------------------------------------------------

async fn build_router_with_listen(listen: &str, port: u16) -> axum::Router {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let cfg = config::Config {
        listen: listen.to_string(),
        port,
        interval_secs: 30,
        data_dir: data_dir.clone(),
        ..Default::default()
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("error"))
        .try_init();

    let s = storage::Storage::open(&data_dir).expect("Failed to open storage");
    let state = Arc::new(api::AppState::new(s, cfg));
    server::build_router(state)
}

#[tokio::test]
async fn test_api_with_loopback_binding() {
    let router = build_router_with_listen("127.0.0.1", 8080).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_info_returns_listen_addr() {
    let router = build_router_with_listen("10.0.0.1", 9090).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/info")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["listen_addr"], "10.0.0.1:9090");
}

// ---------------------------------------------------------------------------
// Dashboard availability
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dashboard_returns_html() {
    let router = build_router_with_listen("127.0.0.1", 8080).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("TheWatcher"));
}

// ---------------------------------------------------------------------------
// API error formatting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_invalid_metric_returns_error_message() {
    let router = build_router_with_listen("127.0.0.1", 8080).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=foobar&range=1h")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let err_msg = String::from_utf8_lossy(&body);
    assert!(err_msg.contains("Unknown metric"));
}

#[tokio::test]
async fn test_missing_range_and_from_returns_default() {
    let router = build_router_with_listen("127.0.0.1", 8080).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

// ---------------------------------------------------------------------------
// JSON serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_cpu_snapshot_serialization() {
    let snap = thewatcher::model::CpuSnapshot {
        percent: Some(42.5),
        per_core: Some(vec![40.0, 45.0, 42.0, 43.0]),
        load_1: Some(1.2),
        load_5: Some(0.9),
        load_15: Some(0.7),
        logical_cpus: 4,
    };
    let json = serde_json::to_string(&snap).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["percent"], 42.5);
    assert_eq!(parsed["logical_cpus"], 4);
}

#[test]
fn test_memory_snapshot_serialization_with_nulls() {
    let snap = thewatcher::model::MemorySnapshot {
        total_bytes: 16000000000,
        used_bytes: 8000000000,
        available_bytes: None,
        used_percent: 50.0,
        swap_total_bytes: None,
        swap_used_bytes: None,
        swap_used_percent: None,
    };
    let json = serde_json::to_string(&snap).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["total_bytes"].as_u64().unwrap(), 16000000000);
    assert!(parsed["available_bytes"].is_null());
}

#[test]
fn test_collector_status_serialization() {
    let status = thewatcher::model::CollectorStatus {
        component: "cpu".to_string(),
        status: "ok".to_string(),
        message: None,
        last_success_ms: Some(1_700_000_000_000i64),
    };
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("cpu"));
    assert!(json.contains("ok"));
}

#[test]
fn test_resolution_auto_select() {
    use thewatcher::model::Resolution;

    // < 1 hour -> granular
    assert_eq!(Resolution::auto_select(1_800_000), Resolution::Granular);
    // 12 hours -> < 1 day -> hourly
    assert_eq!(Resolution::auto_select(43_200_000), Resolution::Hourly);
    // 15 days -> < 30 days -> daily
    assert_eq!(Resolution::auto_select(1_296_000_000), Resolution::Daily);
    // 200 days -> < 365 days -> monthly
    assert_eq!(Resolution::auto_select(17_280_000_000i64), Resolution::Monthly);
    // 2 years -> > 365 days -> yearly
    assert_eq!(
        Resolution::auto_select(63_072_000_000i64),
        Resolution::Yearly
    );
}

#[test]
fn test_resolution_bucket_sizes() {
    use thewatcher::model::Resolution;

    assert_eq!(Resolution::Granular.bucket_ms(), 0);
    assert_eq!(Resolution::Hourly.bucket_ms(), 3_600_000);
    assert_eq!(Resolution::Daily.bucket_ms(), 86_400_000);
    assert_eq!(Resolution::Monthly.bucket_ms(), 2_592_000_000);
    assert_eq!(Resolution::Yearly.bucket_ms(), 31_536_000_000);
}
