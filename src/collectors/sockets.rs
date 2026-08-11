//! Sockets + process count collector.
//!
//! On Linux, reads /proc/net/sockstat for socket summary (unprivileged).
//! Process count comes from sysinfo and works on all platforms.
//! On non-Linux, socket fields return None with status "unavailable".

use crate::model::*;

use super::{ok_status, CollectorContext};

pub fn collect(
    ctx: &mut CollectorContext,
    statuses: &mut Vec<CollectorStatus>,
) -> (ProcessSnapshot, SocketsSnapshot) {
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Process count — portable via sysinfo
    let process_count = ctx.system.processes().len() as u64;

    // Sockets — Linux only
    #[cfg(target_os = "linux")]
    let sockets = read_sockstat();
    #[cfg(not(target_os = "linux"))]
    let sockets: SocketsSnapshot = SocketsSnapshot {
        tcp_inuse: None,
        udp_inuse: None,
        total_sockets: None,
    };

    // Status for sockets
    if sockets.tcp_inuse.is_some() {
        statuses.push(ok_status("sockets", now_ms));
    } else {
        statuses.push(CollectorStatus {
            component: "sockets".to_string(),
            status: "unavailable".to_string(),
            message: Some("socket stats not available on this platform".to_string()),
            last_success_ms: None,
        });
    }

    statuses.push(ok_status("processes", now_ms));

    (
        ProcessSnapshot { count: process_count },
        sockets,
    )
}

/// Parse /proc/net/sockstat into a SocketsSnapshot.
/// Format is simple: "sockets: used N", then "TCP: inuse N ...", "UDP: inuse N ..."
#[cfg(target_os = "linux")]
fn read_sockstat() -> SocketsSnapshot {
    let content = match std::fs::read_to_string("/proc/net/sockstat") {
        Ok(c) => c,
        Err(_) => {
            return SocketsSnapshot {
                tcp_inuse: None,
                udp_inuse: None,
                total_sockets: None,
            };
        }
    };

    let mut total: Option<u32> = None;
    let mut tcp_inuse: Option<u32> = None;
    let mut udp_inuse: Option<u32> = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("sockets: used ") {
            total = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(rest) = line.strip_prefix("TCP: inuse ") {
            tcp_inuse = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(rest) = line.strip_prefix("UDP: inuse ") {
            udp_inuse = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        }
    }

    SocketsSnapshot {
        tcp_inuse,
        udp_inuse,
        total_sockets: total,
    }
}
