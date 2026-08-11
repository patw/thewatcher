//! Integration tests for TheWatcher API.
//!
//! These tests exercise all API endpoints using the tower ServiceExt trait.

use std::sync::Arc;
use tower::util::ServiceExt;

use thewatcher::{api, config, server, storage};

/// Build a test app state with a temporary data directory.
async fn setup_test_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");

    let config = config::Config {
        listen: "127.0.0.1".to_string(),
        port: 8080,
        interval_secs: 30,
        data_dir: data_dir.clone(),
        granular_retention_days: 30,
        hourly_retention_days: 365,
        daily_retention_days: 1825,
        monthly_retention_days: 3650,
        yearly_retention_days: 0,
        log_level: "error".to_string(),
    };

    // Suppress logging during tests
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("error"))
        .try_init();

    let storage = storage::Storage::open(&data_dir).expect("Failed to open storage");

    let state = Arc::new(api::AppState::new(storage, config));

    let router = server::build_router(state);

    (router, dir)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (router, _dir) = setup_test_app().await;

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

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], "0.1.0");
    assert_eq!(json["storage"], "ok");
    assert!(json["last_collection_ms"].is_null());
}

#[tokio::test]
async fn test_info_endpoint() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/info")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["version"], "0.1.0");
    assert!(json["hostname"].is_string());
    assert!(json["os"].is_string());
    assert!(json["arch"].is_string());
}

#[tokio::test]
async fn test_dashboard_loads() {
    let (router, _dir) = setup_test_app().await;

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
    assert!(html.contains("TheWatcher"));
}

#[tokio::test]
async fn test_static_assets() {
    let (router, _dir) = setup_test_app().await;

    // CSS
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/static/styles.css")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // JS
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/static/app.js")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // 404 for unknown
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/static/nonexistent.txt")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_current_without_data() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/current")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 503 when no data collected yet
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn test_history_invalid_metric() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=invalid&range=1h")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_history_invalid_range() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&range=invalid")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_history_valid_request_empty_data() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&range=1h")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["metric"], "cpu");
    assert!(json["resolution"].is_string());
    assert!(json["series"].is_array());
}

#[tokio::test]
async fn test_history_with_data_for_all_metrics() {
    let (router, _dir) = setup_test_app().await;

    for metric in &["cpu", "memory", "disk", "network"] {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&format!("/api/history?metric={}&range=24h", metric))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            axum::http::StatusCode::OK,
            "Metric {} should return OK",
            metric
        );

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["metric"], *metric);
    }
}

#[tokio::test]
async fn test_history_with_explicit_resolution() {
    let (router, _dir) = setup_test_app().await;

    for res in &["granular", "hourly", "daily", "monthly", "yearly"] {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&format!(
                        "/api/history?metric=cpu&range=1y&resolution={}",
                        res
                    ))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}

#[tokio::test]
async fn test_history_invalid_resolution() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&range=1h&resolution=century")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_history_from_must_be_before_until() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&from=1000&until=500")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_history_network_with_interface_filter() {
    let (router, _dir) = setup_test_app().await;

    // Interface filter should still work (returns OK even with no data)
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=network&range=30d&interface=eth0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed even though no data matches
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_history_disk_with_mount_filter() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=disk&range=7d&mount=%2F")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Rendering regression tests: verify API response shapes
// ---------------------------------------------------------------------------

/// Build a router with pre-seeded data in the storage.
async fn setup_seeded_app(
    cpu_values: &[f64],
    net_rx_rates: &[f64],
) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");

    let config = config::Config {
        listen: "127.0.0.1".to_string(),
        port: 8080,
        interval_secs: 30,
        data_dir: data_dir.clone(),
        granular_retention_days: 30,
        hourly_retention_days: 365,
        daily_retention_days: 1825,
        monthly_retention_days: 3650,
        yearly_retention_days: 0,
        log_level: "error".to_string(),
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("error"))
        .try_init();

    let storage = storage::Storage::open(&data_dir).expect("Failed to open storage");

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Seed CPU data
    for (i, &pct) in cpu_values.iter().enumerate() {
        let ts = now_ms - (cpu_values.len() as i64 - i as i64) * 30_000;
        let doc = thewatcher::model::GranularCpu {
            id: format!("cpu-seed-{}", i),
            timestamp_ms: ts,
            metric: "cpu".to_string(),
            resolution: "granular".to_string(),
            cpu_percent: pct,
            load_1: Some(pct / 10.0),
            load_5: Some(pct / 20.0),
            load_15: Some(pct / 30.0),
            logical_cpus: 8,
        };
        storage.write_granular_cpu(&doc).unwrap();
    }

    // Seed network data
    for (i, &rate) in net_rx_rates.iter().enumerate() {
        let ts = now_ms - (net_rx_rates.len() as i64 - i as i64) * 30_000;
        let doc = thewatcher::model::GranularNetwork {
            id: format!("network-seed-{}", i),
            timestamp_ms: ts,
            metric: "network".to_string(),
            resolution: "granular".to_string(),
            interface: "eth0".to_string(),
            rx_bytes_total: (rate * 30.0) as u64 * (i as u64 + 1),
            tx_bytes_total: (rate * 15.0) as u64 * (i as u64 + 1),
            rx_bytes_per_sec: Some(rate),
            tx_bytes_per_sec: Some(rate * 0.5),
            rx_packets_total: 1000,
            tx_packets_total: 500,
            operational: true,
        };
        storage.write_granular_network(&doc).unwrap();
    }

    let state = Arc::new(api::AppState::new(storage, config));
    let router = server::build_router(state);
    (router, dir)
}

#[tokio::test]
async fn test_history_response_has_correct_series_names() {
    let (router, _dir) = setup_seeded_app(&[10.0, 20.0, 30.0, 40.0, 50.0], &[1000.0, 2000.0]).await;

    // CPU history should have cpu_percent and load_1 series
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&range=1h")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let series_names: Vec<&str> = json["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(
        series_names.contains(&"cpu_percent"),
        "CPU history must contain cpu_percent series, got: {:?}",
        series_names
    );
    assert!(
        series_names.contains(&"load_1"),
        "CPU history must contain load_1 series"
    );

    // Network history should have rx_bytes_per_sec and tx_bytes_per_sec
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=network&range=1h&interface=eth0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let series_names: Vec<&str> = json["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(
        series_names.contains(&"rx_bytes_per_sec"),
        "Network history must contain rx_bytes_per_sec series, got: {:?}",
        series_names
    );
    assert!(
        series_names.contains(&"tx_bytes_per_sec"),
        "Network history must contain tx_bytes_per_sec series"
    );
}

#[tokio::test]
async fn test_history_cpu_points_have_values() {
    let (router, _dir) =
        setup_seeded_app(&[10.0, 20.0, 30.0, 40.0, 50.0], &[]).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=cpu&range=1h")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let cpu_series = json["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cpu_percent")
        .expect("cpu_percent series must exist");

    let points = cpu_series["points"].as_array().unwrap();
    assert!(!points.is_empty(), "CPU series must have points");
    for point in points {
        assert!(
            point["value"].is_f64(),
            "CPU point must have numeric value, got: {:?}",
            point
        );
    }
}

#[tokio::test]
async fn test_history_network_points_have_values() {
    let (router, _dir) =
        setup_seeded_app(&[], &[1000.0, 2000.0, 3000.0]).await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/history?metric=network&range=1h&interface=eth0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let rx_series = json["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "rx_bytes_per_sec")
        .expect("rx_bytes_per_sec series must exist");

    let points = rx_series["points"].as_array().unwrap();
    assert!(!points.is_empty(), "Network RX series must have points");
    for point in points {
        assert!(
            point["value"].is_f64() || point["value"].is_null(),
            "Network point must have numeric or null value"
        );
    }
}

#[tokio::test]
async fn test_dashboard_html_has_cache_busting() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let html = String::from_utf8_lossy(&body);

    assert!(
        html.contains("app.js?v="),
        "Dashboard HTML must include versioned JS src for cache busting"
    );
}

#[tokio::test]
async fn test_dashboard_html_contains_all_chart_sections() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let html = String::from_utf8_lossy(&body);

    assert!(html.contains("cpu-chart"), "Dashboard must have CPU chart container");
    assert!(html.contains("mem-chart"), "Dashboard must have memory chart container");
    assert!(html.contains("net-chart"), "Dashboard must have network chart container");
    assert!(html.contains("cpu-gauge"), "Dashboard must have CPU gauge");
    assert!(html.contains("mem-gauge"), "Dashboard must have memory gauge");
    assert!(html.contains("network-cards"), "Dashboard must have network cards container");
}

#[tokio::test]
async fn test_static_js_contains_rendering_functions() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/static/app.js")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let js = String::from_utf8_lossy(&body);

    assert!(js.contains("function updateGauge"), "JS must have updateGauge");
    assert!(js.contains("function drawChart"), "JS must have drawChart");
    assert!(js.contains("function updateDashboard"), "JS must have updateDashboard");
    assert!(js.contains("function friendlyName"), "JS must have friendlyName");
    assert!(js.contains("fmtRate"), "JS must have fmtRate");
}

#[tokio::test]
async fn test_static_css_contains_theme_variables() {
    let (router, _dir) = setup_test_app().await;

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/static/styles.css")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let css = String::from_utf8_lossy(&body);

    assert!(css.contains("--bg:"), "CSS must have theme variables");
    assert!(css.contains("[data-theme=\"dark\"]"), "CSS must have dark theme");
    assert!(css.contains("gauge-fill"), "CSS must have gauge styles");
}
