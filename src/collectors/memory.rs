//! Memory metric collector.

use crate::model::*;

use super::{ok_status, CollectorContext};

pub fn collect(ctx: &mut CollectorContext, statuses: &mut Vec<CollectorStatus>) -> MemorySnapshot {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let total_bytes = ctx.system.total_memory();
    let used_bytes = ctx.system.used_memory();
    let available_bytes = Some(ctx.system.available_memory());
    let used_percent = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let swap_total_bytes = Some(ctx.system.total_swap());
    let swap_used_bytes = Some(ctx.system.used_swap());
    let swap_used_percent = if let (Some(total), Some(used)) = (swap_total_bytes, swap_used_bytes) {
        if total > 0 {
            Some((used as f64 / total as f64) * 100.0)
        } else {
            Some(0.0)
        }
    } else {
        None
    };

    statuses.push(ok_status("memory", now_ms));

    MemorySnapshot {
        total_bytes,
        used_bytes,
        available_bytes,
        used_percent,
        swap_total_bytes,
        swap_used_bytes,
        swap_used_percent,
    }
}
