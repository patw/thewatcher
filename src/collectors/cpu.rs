//! CPU metric collector.

use crate::model::*;
use sysinfo::System;

use super::{ok_status, CollectorContext};

pub fn collect(ctx: &mut CollectorContext, statuses: &mut Vec<CollectorStatus>) -> CpuSnapshot {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let logical_cpus = ctx.system.cpus().len() as u32;

    // CPU utilization via delta calculation
    let (cpu_percent, per_core) = if ctx.prev_cpu.is_empty() {
        // First sample: no delta available
        (None, None)
    } else {
        let cpus = ctx.system.cpus();
        let mut core_percents = Vec::with_capacity(cpus.len());

        for cpu in cpus {
            let curr_usage = cpu.cpu_usage() as f64;
            core_percents.push(curr_usage);
        }

        let avg_percent = if !core_percents.is_empty() {
            core_percents.iter().sum::<f64>() / core_percents.len() as f64
        } else {
            0.0
        };

        (Some(avg_percent), Some(core_percents))
    };

    // Update prev_cpu
    ctx.prev_cpu = ctx
        .system
        .cpus()
        .iter()
        .map(|cpu| {
            (
                cpu.name().to_string(),
                cpu.cpu_usage() as f64,
                0.0,
                0.0,
            )
        })
        .collect();

    // Load average
    let (load_1, load_5, load_15) = {
        let load_avg = System::load_average();
        (Some(load_avg.one), Some(load_avg.five), Some(load_avg.fifteen))
    };

    statuses.push(ok_status("cpu", now_ms));

    CpuSnapshot {
        percent: cpu_percent,
        per_core,
        load_1,
        load_5,
        load_15,
        logical_cpus,
    }
}
