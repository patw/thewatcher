//! Uptime and host information collector.

use crate::model::*;

use super::{ok_status, CollectorContext};

pub fn collect(
    ctx: &mut CollectorContext,
    statuses: &mut Vec<CollectorStatus>,
) -> (String, u64) {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let hostname = hostname();
    let uptime_seconds = system_uptime(ctx);

    statuses.push(ok_status("uptime", now_ms));

    (hostname, uptime_seconds)
}

fn hostname() -> String {
    // Use sysinfo
    use sysinfo::System;
    System::host_name().unwrap_or_else(|| "unknown".to_string())
}

fn system_uptime(_ctx: &CollectorContext) -> u64 {
    use sysinfo::System;
    System::uptime()
}
