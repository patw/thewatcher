//! Network interface metric collector.

use crate::model::*;

use super::{ok_status, CollectorContext};

pub fn collect(
    ctx: &mut CollectorContext,
    statuses: &mut Vec<CollectorStatus>,
) -> Vec<NetworkSnapshot> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut networks = Vec::new();

    // Collect current counters - sysinfo 0.34 uses Networks
    let net_data = sysinfo::Networks::new_with_refreshed_list();

    let mut current: Vec<(String, u64, u64, u64, u64, bool)> = Vec::new();

    for (iface_name, data) in net_data.iter() {
        let rx_bytes = data.total_received();
        let tx_bytes = data.total_transmitted();
        let rx_packets = data.total_packets_received();
        let tx_packets = data.total_packets_transmitted();
        let operational = rx_bytes > 0 || tx_bytes > 0;

        current.push((
            iface_name.clone(),
            rx_bytes,
            tx_bytes,
            rx_packets,
            tx_packets,
            operational,
        ));
    }

    // Calculate rates
    let elapsed = ctx
        .last_collection
        .map(|last| last.elapsed().as_secs_f64())
        .unwrap_or(1.0);

    for (name, rx, tx, rx_pkt, tx_pkt, operational) in &current {
        let (rx_rate, tx_rate) = if let Some(prev) = ctx
            .prev_network
            .iter()
            .find(|(n, _, _, _, _)| n == name)
        {
            let rx_delta = if rx >= &prev.1 {
                (*rx - prev.1) as f64
            } else {
                *rx as f64
            };
            let tx_delta = if tx >= &prev.2 {
                (*tx - prev.2) as f64
            } else {
                *tx as f64
            };

            let rx_rate = if elapsed > 0.0 {
                Some(rx_delta / elapsed)
            } else {
                None
            };
            let tx_rate = if elapsed > 0.0 {
                Some(tx_delta / elapsed)
            } else {
                None
            };

            (rx_rate, tx_rate)
        } else {
            (None, None)
        };

        networks.push(NetworkSnapshot {
            interface: name.clone(),
            operational: Some(*operational),
            rx_bytes_total: *rx,
            tx_bytes_total: *tx,
            rx_packets_total: *rx_pkt,
            tx_packets_total: *tx_pkt,
            rx_bytes_per_sec: rx_rate,
            tx_bytes_per_sec: tx_rate,
            rx_errors: None,
            tx_errors: None,
            rx_dropped: None,
            tx_dropped: None,
        });
    }

    // Update previous counters
    ctx.prev_network = current
        .iter()
        .map(|(n, rx, tx, rxp, txp, _)| (n.clone(), *rx, *tx, *rxp, *txp))
        .collect();

    statuses.push(ok_status("network", now_ms));

    networks
}
