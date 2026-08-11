//! System metrics collectors.
//!
//! Each collector gathers a specific type of metric and returns structured data.
//! Failures in one collector must not affect others.

pub mod cpu;
pub mod disk;
pub mod memory;
pub mod network;
pub mod sockets;
pub mod uptime;

use crate::model::*;
use std::time::Instant;
use sysinfo::System;

/// Shared system state for collectors that need to track deltas.
pub struct CollectorContext {
    pub system: System,
    /// Previous network counters: (interface_name, rx_bytes, tx_bytes, rx_packets, tx_packets)
    pub prev_network: Vec<(String, u64, u64, u64, u64)>,
    /// Previous CPU counters: (name, user, system, idle)
    pub prev_cpu: Vec<(String, f64, f64, f64)>,
    pub last_collection: Option<Instant>,
}

impl CollectorContext {
    pub fn new() -> Self {
        let mut system = System::new_all();
        // First refresh to populate initial values
        system.refresh_all();
        // Small delay for CPU delta calculation
        std::thread::sleep(std::time::Duration::from_millis(100));
        system.refresh_all();

        Self {
            system,
            prev_network: Vec::new(),
            prev_cpu: Vec::new(),
            last_collection: None,
        }
    }

    pub fn refresh(&mut self) {
        self.system.refresh_all();
    }
}

/// Collect all metrics and return a current snapshot.
pub fn collect_all(ctx: &mut CollectorContext) -> CurrentSnapshot {
    ctx.refresh();

    let mut statuses = Vec::new();

    let cpu = cpu::collect(ctx, &mut statuses);
    let memory = memory::collect(ctx, &mut statuses);
    let disks = disk::collect(ctx, &mut statuses);
    let networks = network::collect(ctx, &mut statuses);
    let (processes, sockets) = sockets::collect(ctx, &mut statuses);
    let (hostname, uptime_seconds) = uptime::collect(ctx, &mut statuses);

    let now_ms = chrono::Utc::now().timestamp_millis();

    ctx.last_collection = Some(Instant::now());

    CurrentSnapshot {
        timestamp_ms: now_ms,
        hostname,
        uptime_seconds,
        cpu,
        memory,
        disks,
        networks,
        processes,
        sockets,
        collector_status: statuses,
    }
}

/// Helper to record a successful collector status.
pub fn ok_status(component: &str, now_ms: i64) -> CollectorStatus {
    CollectorStatus {
        component: component.to_string(),
        status: "ok".to_string(),
        message: None,
        last_success_ms: Some(now_ms),
    }
}

/// Helper to record a degraded collector status.
#[allow(dead_code)]
pub fn unavailable_status(
    component: &str,
    message: String,
    last_success_ms: Option<i64>,
) -> CollectorStatus {
    CollectorStatus {
        component: component.to_string(),
        status: "unavailable".to_string(),
        message: Some(message),
        last_success_ms,
    }
}
