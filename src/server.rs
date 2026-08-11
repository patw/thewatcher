//! HTTP server setup for TheWatcher.

use crate::api::{self, AppState};
use crate::config::Config;
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::timeout::TimeoutLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing;

/// Embedded web assets
#[derive(RustEmbed)]
#[folder = "src/web/"]
struct WebAssets;

/// Build the Axum router with all routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    // Serve embedded static assets
    let static_handler = axum::routing::get(static_handler);

    Router::new()
        // Dashboard
        .route("/", get(dashboard_handler))
        // Static assets
        .route("/static/{*path}", static_handler)
        // API routes
        .route("/api/current", get(api::api_current))
        .route("/api/history", get(api::api_history))
        .route("/api/health", get(api::api_health))
        .route("/api/info", get(api::api_info))
        // Middleware
        .layer((
            RequestBodyLimitLayer::new(1024 * 1024), // 1MB body limit
            TimeoutLayer::with_status_code(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                std::time::Duration::from_secs(30),
            ),
        ))
        .with_state(state)
}

/// Serve the dashboard HTML.
async fn dashboard_handler() -> axum::response::Response {
    match WebAssets::get("index.html") {
        Some(file) => {
            let html = String::from_utf8_lossy(&file.data).into_owned();
            // Inject version into script src for cache busting
            let html = html.replace(
                "app.js\"",
                &format!("app.js?v={}\"", env!("CARGO_PKG_VERSION")),
            );
            axum::response::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(axum::body::Body::from(html))
                .unwrap()
        }
        None => axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(axum::body::Body::from("Dashboard not found"))
            .unwrap(),
    }
}

/// Serve static assets.
async fn static_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> axum::response::Response {
    match WebAssets::get(&path) {
        Some(file) => {
            let content_type = mime_type(&path);
            axum::response::Response::builder()
                .header("Content-Type", content_type)
                .header("Cache-Control", "no-cache")
                .body(axum::body::Body::from(file.data.to_vec()))
                .unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::empty())
                        .unwrap()
                })
        }
        None => axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Not found"))
            .unwrap(),
    }
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

/// Start the HTTP server.
pub async fn start_server(
    config: &Config,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = config.listener_addr().parse()?;
    let router = build_router(state);

    tracing::info!("Listening on http://{}", addr);

    // Security warning for broad binding
    if config.is_all_interfaces() {
        let warning = format!(
            "WARNING: TheWatcher is listening on all IPv4 interfaces ({}).\n\
             System metrics and the read-only API will be reachable from the network.\n\
             Use --listen 127.0.0.1 for local-only access, or bind to a specific management address.\n\
             TheWatcher does not provide TLS or authentication in this release.",
            addr,
        );
        tracing::warn!("{}", warning);
        eprintln!("{}", warning);
    } else if !config.is_loopback() {
        tracing::warn!(
            "TheWatcher is listening on non-loopback address ({}). \
             Network access control is the operator's responsibility.",
            addr,
        );
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
