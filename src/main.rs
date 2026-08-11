//! TheWatcher — Self-hosted, single-binary system metrics viewer.
//!
//! Collects system metrics, persists to MooFile, performs RRD-style rollups,
//! and serves a browser-based dashboard with a read-only JSON API.

use clap::Parser;
use std::sync::Arc;
use tracing;

use thewatcher::{api, cli, collectors, model, server, storage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse CLI
    let cli = cli::Cli::parse();
    let config = cli.into_config().unwrap_or_else(|e| {
        eprintln!("Configuration error: {}", e);
        std::process::exit(1);
    });

    // Setup logging
    init_logging(&config.log_level);

    // Print startup banner
    tracing::info!("TheWatcher v{} starting", env!("CARGO_PKG_VERSION"));
    tracing::info!(
        "Platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    tracing::info!("Data directory: {}", config.data_dir.display());
    tracing::info!("Collection interval: {}s", config.interval_secs);
    tracing::info!(
        "Retention: granular={}d, hourly={}d, daily={}d, monthly={}d, yearly={}",
        config.granular_retention_days,
        config.hourly_retention_days,
        config.daily_retention_days,
        config.monthly_retention_days,
        if config.yearly_retention_days == 0 {
            "indefinite".to_string()
        } else {
            format!("{}d", config.yearly_retention_days)
        },
    );

    // Open storage
    let storage = storage::Storage::open(&config.data_dir).unwrap_or_else(|e| {
        tracing::error!("Failed to open storage: {}", e);
        eprintln!("Fatal: Failed to open storage: {}", e);
        std::process::exit(1);
    });

    // Create shared state
    let state = Arc::new(api::AppState::new(storage, config.clone()));

    // Start background collection task
    let collection_state = state.clone();
    let collection_interval = config.interval_secs;
    let collection_handle = tokio::spawn(async move {
        collection_loop(collection_state, collection_interval).await;
    });

    // Start background rollup/retention task
    let maintenance_state = state.clone();
    let maintenance_handle = tokio::spawn(async move {
        maintenance_loop(maintenance_state).await;
    });

    // Start HTTP server (this blocks until shutdown)
    let server_result = server::start_server(&config, state).await;

    // Cancel background tasks on shutdown
    collection_handle.abort();
    maintenance_handle.abort();

    server_result?;
    Ok(())
}

/// Collection loop: runs at the configured interval.
async fn collection_loop(state: Arc<api::AppState>, interval_secs: u64) {
    let mut ctx = collectors::CollectorContext::new();

    // Do an initial collection immediately
    {
        let snapshot = collectors::collect_all(&mut ctx);
        let now_ms = snapshot.timestamp_ms;

        // Persist granular samples
        if let Err(e) = persist_granular(&state, &snapshot) {
            tracing::error!("Failed to persist granular samples: {}", e);
        }

        // Update in-memory snapshot
        {
            let mut current = state.current_snapshot.write().await;
            *current = Some(snapshot);
        }
        {
            let mut last = state.last_collection_ms.write().await;
            *last = Some(now_ms);
        }
    }

    // Then loop at the configured interval
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    // Skip the first tick (it fires immediately)
    interval.tick().await;

    loop {
        interval.tick().await;

        let snapshot = collectors::collect_all(&mut ctx);
        let now_ms = snapshot.timestamp_ms;

        // Persist granular samples
        if let Err(e) = persist_granular(&state, &snapshot) {
            tracing::error!("Failed to persist granular samples: {}", e);
        }

        // Update in-memory snapshot
        {
            let mut current = state.current_snapshot.write().await;
            *current = Some(snapshot);
        }
        {
            let mut last = state.last_collection_ms.write().await;
            *last = Some(now_ms);
        }

        tracing::debug!("Collection complete at {}", now_ms);
    }
}

/// Persist granular samples to MooFile.
fn persist_granular(
    state: &api::AppState,
    snapshot: &model::CurrentSnapshot,
) -> Result<(), String> {
    let ts = snapshot.timestamp_ms;

    // CPU
    if let Some(cpu_pct) = snapshot.cpu.percent {
        let doc = model::GranularCpu {
            id: format!("cpu-{}", ts),
            timestamp_ms: ts,
            metric: "cpu".to_string(),
            resolution: "granular".to_string(),
            cpu_percent: cpu_pct,
            load_1: snapshot.cpu.load_1,
            load_5: snapshot.cpu.load_5,
            load_15: snapshot.cpu.load_15,
            logical_cpus: snapshot.cpu.logical_cpus,
        };
        state.storage.write_granular_cpu(&doc)?;
    }

    // Memory
    {
        let doc = model::GranularMemory {
            id: format!("memory-{}", ts),
            timestamp_ms: ts,
            metric: "memory".to_string(),
            resolution: "granular".to_string(),
            total_bytes: snapshot.memory.total_bytes,
            used_bytes: snapshot.memory.used_bytes,
            available_bytes: snapshot.memory.available_bytes,
            used_percent: snapshot.memory.used_percent,
            swap_total_bytes: snapshot.memory.swap_total_bytes,
            swap_used_bytes: snapshot.memory.swap_used_bytes,
        };
        state.storage.write_granular_memory(&doc)?;
    }

    // Disks
    for disk in &snapshot.disks {
        let mount_id = disk.mount.replace('/', "-").replace('\\', "-");
        let doc = model::GranularDisk {
            id: format!("disk-{}-{}", mount_id, ts),
            timestamp_ms: ts,
            metric: "disk".to_string(),
            resolution: "granular".to_string(),
            mount: disk.mount.clone(),
            filesystem: disk.filesystem.clone(),
            total_bytes: disk.total_bytes,
            used_bytes: disk.used_bytes,
            available_bytes: disk.available_bytes,
            used_percent: disk.used_percent,
        };
        if let Err(e) = state.storage.write_granular_disk(&doc) {
            tracing::warn!("Failed to persist disk {}: {}", disk.mount, e);
        }
    }

    // Networks
    for net in &snapshot.networks {
        let doc = model::GranularNetwork {
            id: format!("network-{}-{}", net.interface, ts),
            timestamp_ms: ts,
            metric: "network".to_string(),
            resolution: "granular".to_string(),
            interface: net.interface.clone(),
            rx_bytes_total: net.rx_bytes_total,
            tx_bytes_total: net.tx_bytes_total,
            rx_bytes_per_sec: net.rx_bytes_per_sec,
            tx_bytes_per_sec: net.tx_bytes_per_sec,
            rx_packets_total: net.rx_packets_total,
            tx_packets_total: net.tx_packets_total,
            operational: net.operational.unwrap_or(false),
        };
        if let Err(e) = state.storage.write_granular_network(&doc) {
            tracing::warn!("Failed to persist network {}: {}", net.interface, e);
        }
    }

    // Sockets + processes
    {
        let doc = model::GranularSockets {
            id: format!("sockets-{}", ts),
            timestamp_ms: ts,
            metric: "sockets".to_string(),
            resolution: "granular".to_string(),
            process_count: snapshot.processes.count,
            tcp_inuse: snapshot.sockets.tcp_inuse,
            udp_inuse: snapshot.sockets.udp_inuse,
            total_sockets: snapshot.sockets.total_sockets,
        };
        if let Err(e) = state.storage.write_granular_sockets(&doc) {
            tracing::warn!("Failed to persist sockets: {}", e);
        }
    }

    Ok(())
}

/// Background maintenance loop: rollups every 5 minutes, retention every hour.
async fn maintenance_loop(state: Arc<api::AppState>) {
    // Wait a bit before first maintenance to let some data accumulate
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

    let mut rollup_interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 min
    let mut retention_interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // 1 hour

    // Skip first immediate tick
    rollup_interval.tick().await;
    retention_interval.tick().await;

    loop {
        tokio::select! {
            _ = rollup_interval.tick() => {
                match storage::rollup::run_rollups(&state.storage) {
                    Ok(now_ms) => {
                        let mut last = state.last_rollup_ms.write().await;
                        *last = Some(now_ms);
                        tracing::debug!("Rollups complete");
                    }
                    Err(e) => {
                        tracing::error!("Rollup failed: {}", e);
                    }
                }
            }
            _ = retention_interval.tick() => {
                if let Err(e) = storage::retention::run_retention(&state.storage, &state.config) {
                    tracing::error!("Retention failed: {}", e);
                }
            }
        }
    }
}

/// Initialize tracing/logging.
fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = match level {
        "error" => "error",
        "warn" => "warn",
        "info" => "info",
        "debug" => "debug",
        "trace" => "trace",
        _ => "info",
    };

    let filter = EnvFilter::try_new(format!("{}", filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}
