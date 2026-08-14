//! RRD-style rollup from granular → hourly → daily → monthly → yearly.

use crate::model::*;
use crate::storage::Storage;
use bson::Document;
use tracing;

/// Perform rollups for all completed buckets.
pub fn run_rollups(storage: &Storage) -> Result<i64, String> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Clean up stale empty rollup documents that may have been written by an
    // earlier buggy version (before network/sockets rollup was implemented).
    cleanup_stale_empty_rollups(storage)?;

    // Granular → Hourly
    rollup_resolution(storage, "granular", "hourly", Resolution::Hourly)?;

    // Hourly → Daily
    rollup_resolution(storage, "hourly", "daily", Resolution::Daily)?;

    // Daily → Monthly
    rollup_resolution(storage, "daily", "monthly", Resolution::Monthly)?;

    // Monthly → Yearly
    rollup_resolution(storage, "monthly", "yearly", Resolution::Yearly)?;

    Ok(now_ms)
}

/// Rollup from `src_resolution` to `dst_resolution`.
///
/// Only processes completed buckets: a bucket at the source resolution is
/// "complete" when its end time is in the past.
fn rollup_resolution(
    storage: &Storage,
    src_resolution: &str,
    dst_resolution: &str,
    dst: Resolution,
) -> Result<(), String> {
    let bucket_ms = dst.bucket_ms();
    if bucket_ms == 0 {
        return Ok(());
    }

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Find the most recent rollup for each metric, so we know where to resume
    let metrics = ["cpu", "memory", "disk", "network", "sockets"];

    for &metric in &metrics {
        let last_rollup = storage.last_timestamp(dst_resolution, metric)?;
        // Defensive bound: if we have no resume point (e.g. a metric's rollup
        // documents were lost), never scan back further than the granular
        // retention window. Resuming from 0 (Unix epoch) meant a single
        // missing/mis-serialized metric triggered ~500k per-bucket queries of
        // the granular collection every maintenance cycle, pegging a CPU core.
        // Granular retention defaults to 30 days, so this never drops data.
        const MAX_LOOKBACK_MS: i64 = 30 * 86_400_000; // 30 days
        let start_from = last_rollup.unwrap_or(now_ms - MAX_LOOKBACK_MS);

        // Align to bucket boundaries
        let mut bucket_start = align_down(start_from, bucket_ms);
        if bucket_start < start_from {
            bucket_start += bucket_ms;
        }

        // We need source data for this bucket. The bucket must be fully in the past.
        while bucket_start + bucket_ms <= now_ms {
            let bucket_end = bucket_start + bucket_ms;

            // Fetch source data in this bucket
            let source_docs = storage.query_rollup(
                src_resolution,
                metric,
                bucket_start,
                bucket_end,
                None, // no interface filter for rollup
                None, // no mount filter for rollup
            )?;

            if source_docs.is_empty() {
                bucket_start += bucket_ms;
                continue;
            }

            // Compute rollup(s)
            let rollups = compute_rollup(metric, &source_docs, bucket_start, bucket_end, dst);
            for rollup in rollups {
                storage.write_rollup(dst_resolution, &rollup)?;
                tracing::debug!(
                    "Rollup: {} {} bucket {} ({} samples)",
                    dst_resolution,
                    metric,
                    bucket_start,
                    rollup.sample_count,
                );
            }

            bucket_start += bucket_ms;
        }
    }

    Ok(())
}

/// Compute rollup document(s) from source documents.
///
/// Returns one document per metric (or per interface for network).
fn compute_rollup(
    metric: &str,
    source_docs: &[Document],
    bucket_start: i64,
    bucket_end: i64,
    resolution: Resolution,
) -> Vec<RollupDoc> {
    if source_docs.is_empty() {
        return vec![];
    }

    let sample_count = source_docs.len() as u64;

    match metric {
        "cpu" => {
            let values: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| d.get_f64("cpu_percent").ok())
                .collect();
            let loads_1: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| d.get_f64("load_1").ok())
                .collect();

            let mut rollup = RollupDoc {
                id: format!("{}-{}-{}", metric, resolution.as_str(), bucket_start),
                bucket_start_ms: bucket_start,
                bucket_end_ms: bucket_end,
                timestamp_ms: bucket_start,
                metric: metric.to_string(),
                resolution: resolution.as_str().to_string(),
                sample_count,
                cpu_min: None,
                cpu_mean: None,
                cpu_max: None,
                load_1_min: None,
                load_1_mean: None,
                load_1_max: None,
                mem_used_min: None,
                mem_used_mean: None,
                mem_used_max: None,
                interface: None,
                net_rx_min: None,
                net_rx_mean: None,
                net_rx_max: None,
                net_tx_min: None,
                net_tx_mean: None,
                net_tx_max: None,
                process_count_min: None,
                process_count_mean: None,
                process_count_max: None,
                tcp_inuse_min: None,
                tcp_inuse_mean: None,
                tcp_inuse_max: None,
                udp_inuse_min: None,
                udp_inuse_mean: None,
                udp_inuse_max: None,
                total_sockets_min: None,
                total_sockets_mean: None,
                total_sockets_max: None,
            };

            if !values.is_empty() {
                rollup.cpu_min = min(&values);
                rollup.cpu_mean = Some(mean(&values));
                rollup.cpu_max = max(&values);
            }
            if !loads_1.is_empty() {
                rollup.load_1_min = min(&loads_1);
                rollup.load_1_mean = Some(mean(&loads_1));
                rollup.load_1_max = max(&loads_1);
            }

            vec![rollup]
        }
        "memory" => {
            let values: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| d.get_f64("used_percent").ok())
                .collect();

            let mut rollup = RollupDoc {
                id: format!("{}-{}-{}", metric, resolution.as_str(), bucket_start),
                bucket_start_ms: bucket_start,
                bucket_end_ms: bucket_end,
                timestamp_ms: bucket_start,
                metric: metric.to_string(),
                resolution: resolution.as_str().to_string(),
                sample_count,
                cpu_min: None,
                cpu_mean: None,
                cpu_max: None,
                load_1_min: None,
                load_1_mean: None,
                load_1_max: None,
                mem_used_min: None,
                mem_used_mean: None,
                mem_used_max: None,
                interface: None,
                net_rx_min: None,
                net_rx_mean: None,
                net_rx_max: None,
                net_tx_min: None,
                net_tx_mean: None,
                net_tx_max: None,
                process_count_min: None,
                process_count_mean: None,
                process_count_max: None,
                tcp_inuse_min: None,
                tcp_inuse_mean: None,
                tcp_inuse_max: None,
                udp_inuse_min: None,
                udp_inuse_mean: None,
                udp_inuse_max: None,
                total_sockets_min: None,
                total_sockets_mean: None,
                total_sockets_max: None,
            };

            if !values.is_empty() {
                rollup.mem_used_min = min(&values);
                rollup.mem_used_mean = Some(mean(&values));
                rollup.mem_used_max = max(&values);
            }

            vec![rollup]
        }
        "disk" => {
            // Rollup disk usage into the mem_used_* fields. The history API
            // already reads mem_used_* for disk rollup documents (see
            // build_series_disk), so this keeps disk history working without
            // adding dedicated disk fields. Disk rollups were accidentally
            // dropped entirely in 0.1.2 (fell through to `_ => vec![]`).
            let values: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| d.get_f64("used_percent").ok())
                .collect();

            let mut rollup = RollupDoc {
                id: format!("{}-{}-{}", metric, resolution.as_str(), bucket_start),
                bucket_start_ms: bucket_start,
                bucket_end_ms: bucket_end,
                timestamp_ms: bucket_start,
                metric: metric.to_string(),
                resolution: resolution.as_str().to_string(),
                sample_count,
                cpu_min: None,
                cpu_mean: None,
                cpu_max: None,
                load_1_min: None,
                load_1_mean: None,
                load_1_max: None,
                mem_used_min: None,
                mem_used_mean: None,
                mem_used_max: None,
                interface: None,
                net_rx_min: None,
                net_rx_mean: None,
                net_rx_max: None,
                net_tx_min: None,
                net_tx_mean: None,
                net_tx_max: None,
                process_count_min: None,
                process_count_mean: None,
                process_count_max: None,
                tcp_inuse_min: None,
                tcp_inuse_mean: None,
                tcp_inuse_max: None,
                udp_inuse_min: None,
                udp_inuse_mean: None,
                udp_inuse_max: None,
                total_sockets_min: None,
                total_sockets_mean: None,
                total_sockets_max: None,
            };

            if !values.is_empty() {
                rollup.mem_used_min = min(&values);
                rollup.mem_used_mean = Some(mean(&values));
                rollup.mem_used_max = max(&values);
            }

            vec![rollup]
        }
        "network" => {
            // Group source documents by interface
            let mut by_interface: std::collections::BTreeMap<String, Vec<&Document>> =
                std::collections::BTreeMap::new();
            for doc in source_docs {
                if let Ok(iface) = doc.get_str("interface") {
                    by_interface
                        .entry(iface.to_string())
                        .or_default()
                        .push(doc);
                }
            }

            by_interface
                .into_iter()
                .map(|(iface, docs)| {
                    let rx_vals: Vec<f64> = docs
                        .iter()
                        .filter_map(|d| d.get_f64("rx_bytes_per_sec").ok())
                        .collect();
                    let tx_vals: Vec<f64> = docs
                        .iter()
                        .filter_map(|d| d.get_f64("tx_bytes_per_sec").ok())
                        .collect();

                    RollupDoc {
                        id: format!(
                            "{}-{}-{}-{}",
                            metric,
                            resolution.as_str(),
                            bucket_start,
                            iface
                        ),
                        bucket_start_ms: bucket_start,
                        bucket_end_ms: bucket_end,
                        timestamp_ms: bucket_start,
                        metric: metric.to_string(),
                        resolution: resolution.as_str().to_string(),
                        sample_count: docs.len() as u64,
                        cpu_min: None,
                        cpu_mean: None,
                        cpu_max: None,
                        load_1_min: None,
                        load_1_mean: None,
                        load_1_max: None,
                        mem_used_min: None,
                        mem_used_mean: None,
                        mem_used_max: None,
                        interface: Some(iface),
                        net_rx_min: if rx_vals.is_empty() { None } else { min(&rx_vals) },
                        net_rx_mean: if rx_vals.is_empty() { None } else { Some(mean(&rx_vals)) },
                        net_rx_max: if rx_vals.is_empty() { None } else { max(&rx_vals) },
                        net_tx_min: if tx_vals.is_empty() { None } else { min(&tx_vals) },
                        net_tx_mean: if tx_vals.is_empty() { None } else { Some(mean(&tx_vals)) },
                        net_tx_max: if tx_vals.is_empty() { None } else { max(&tx_vals) },
                        process_count_min: None,
                        process_count_mean: None,
                        process_count_max: None,
                        tcp_inuse_min: None,
                        tcp_inuse_mean: None,
                        tcp_inuse_max: None,
                        udp_inuse_min: None,
                        udp_inuse_mean: None,
                        udp_inuse_max: None,
                        total_sockets_min: None,
                        total_sockets_mean: None,
                        total_sockets_max: None,
                    }
                })
                .collect()
        }
        "sockets" => {
            // NOTE: granular sockets fields are `Option<u32>`, which BSON
            // stores as int64. The 0.1.2 code read them with `get_i32()`, which
            // silently failed, so every sockets rollup was written without
            // tcp/udp/total values. `num_as_f64` handles int32, int64, and
            // double, so this can't regress.
            let proc_vals: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| num_as_f64(d, "process_count"))
                .collect();
            let tcp_vals: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| num_as_f64(d, "tcp_inuse"))
                .collect();
            let udp_vals: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| num_as_f64(d, "udp_inuse"))
                .collect();
            let sock_vals: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| num_as_f64(d, "total_sockets"))
                .collect();

            let rollup = RollupDoc {
                id: format!("{}-{}-{}", metric, resolution.as_str(), bucket_start),
                bucket_start_ms: bucket_start,
                bucket_end_ms: bucket_end,
                timestamp_ms: bucket_start,
                metric: metric.to_string(),
                resolution: resolution.as_str().to_string(),
                sample_count,
                cpu_min: None,
                cpu_mean: None,
                cpu_max: None,
                load_1_min: None,
                load_1_mean: None,
                load_1_max: None,
                mem_used_min: None,
                mem_used_mean: None,
                mem_used_max: None,
                interface: None,
                net_rx_min: None,
                net_rx_mean: None,
                net_rx_max: None,
                net_tx_min: None,
                net_tx_mean: None,
                net_tx_max: None,
                process_count_min: if proc_vals.is_empty() { None } else { min(&proc_vals) },
                process_count_mean: if proc_vals.is_empty() { None } else { Some(mean(&proc_vals)) },
                process_count_max: if proc_vals.is_empty() { None } else { max(&proc_vals) },
                tcp_inuse_min: if tcp_vals.is_empty() { None } else { min(&tcp_vals) },
                tcp_inuse_mean: if tcp_vals.is_empty() { None } else { Some(mean(&tcp_vals)) },
                tcp_inuse_max: if tcp_vals.is_empty() { None } else { max(&tcp_vals) },
                udp_inuse_min: if udp_vals.is_empty() { None } else { min(&udp_vals) },
                udp_inuse_mean: if udp_vals.is_empty() { None } else { Some(mean(&udp_vals)) },
                udp_inuse_max: if udp_vals.is_empty() { None } else { max(&udp_vals) },
                total_sockets_min: if sock_vals.is_empty() { None } else { min(&sock_vals) },
                total_sockets_mean: if sock_vals.is_empty() { None } else { Some(mean(&sock_vals)) },
                total_sockets_max: if sock_vals.is_empty() { None } else { max(&sock_vals) },
            };

            vec![rollup]
        }
        _ => vec![],
    }
}

/// Extract a numeric field as f64 regardless of whether BSON stored it as
/// int32, int64, or double.
fn num_as_f64(doc: &Document, key: &str) -> Option<f64> {
    if let Ok(v) = doc.get_i32(key) {
        Some(v as f64)
    } else if let Ok(v) = doc.get_i64(key) {
        Some(v as f64)
    } else if let Ok(v) = doc.get_f64(key) {
        Some(v)
    } else {
        None
    }
}

fn align_down(timestamp_ms: i64, bucket_ms: i64) -> i64 {
    (timestamp_ms / bucket_ms) * bucket_ms
}

fn min(values: &[f64]) -> Option<f64> {
    values.iter().cloned().fold(None, |acc, v| {
        Some(acc.map_or(v, |a| a.min(v)))
    })
}

fn max(values: &[f64]) -> Option<f64> {
    values.iter().cloned().fold(None, |acc, v| {
        Some(acc.map_or(v, |a| a.max(v)))
    })
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Remove stale empty rollup documents for network and sockets that were
/// written by a previous buggy version of the code. These have no meaningful
/// data (all rollup fields are None) and would prevent new rollups from being
/// written due to duplicate key errors.
///
/// These filters are safe against the fixed rollup writer:
/// - network rollups always carry an `interface` field, so
///   `interface: {$exists: false}` only matches pre-0.1.2 documents;
/// - sockets rollups now always carry a `tcp_inuse_mean` value (the granular
///   collector reliably reports TCP sockets), so the filter only matches the
///   old empty documents and the 0.1.2 ones written by the get_i32 bug.
///
/// Even if a pathological platform ever produced sockets docs without
/// `tcp_inuse_mean`, rollup_resolution()'s bounded lookback (see above) means
/// a missing resume point can never trigger a from-epoch scan again.
fn cleanup_stale_empty_rollups(storage: &Storage) -> Result<(), String> {
    // Only clean up the hourly resolution, since that's what had the bug.
    // Old network rollup docs have ID format "network-hourly-{ts}" (no interface).
    // We identify them by: metric=network, interface field missing.
    let net_filter = bson::doc! {
        "metric": "network",
        "interface": { "$exists": false },
    };
    let deleted_net = storage.delete_many("hourly", net_filter)?;
    if deleted_net > 0 {
        tracing::info!(
            "Cleaned up {} stale empty network rollup document(s)",
            deleted_net
        );
    }

    // Old sockets rollup docs have ID format "sockets-hourly-{ts}" and have no
    // tcp_inuse_mean field (since the old code never set it).
    let sock_filter = bson::doc! {
        "metric": "sockets",
        "tcp_inuse_mean": { "$exists": false },
    };
    let deleted_sock = storage.delete_many("hourly", sock_filter)?;
    if deleted_sock > 0 {
        tracing::info!(
            "Cleaned up {} stale empty sockets rollup document(s)",
            deleted_sock
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_down() {
        assert_eq!(align_down(3_700_000, 3_600_000), 3_600_000);
        assert_eq!(align_down(7_200_000, 3_600_000), 7_200_000);
        assert_eq!(align_down(0, 3_600_000), 0);
    }

    #[test]
    fn test_mean() {
        assert_eq!(mean(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(mean(&[]), 0.0);
    }

    #[test]
    fn test_min_max() {
        assert_eq!(min(&[1.0, 2.0, 3.0]), Some(1.0));
        assert_eq!(max(&[1.0, 2.0, 3.0]), Some(3.0));
        assert_eq!(min(&[]), None);
        assert_eq!(max(&[]), None);
    }

    // Regression: v0.1.2 read granular sockets fields (stored as u32 → BSON
    // int64) with get_i32(), silently producing empty rollups and ultimately
    // the from-epoch rollup hot loop.
    #[test]
    fn test_compute_rollup_sockets_reads_int64_fields() {
        use bson::{Bson, Document};

        let mut d1 = Document::new();
        d1.insert("tcp_inuse", Bson::Int64(100));
        d1.insert("udp_inuse", Bson::Int64(40));
        d1.insert("total_sockets", Bson::Int64(300));
        d1.insert("process_count", Bson::Int64(42));

        let mut d2 = Document::new();
        d2.insert("tcp_inuse", Bson::Int64(120));
        d2.insert("udp_inuse", Bson::Int64(60));
        d2.insert("total_sockets", Bson::Int64(360));
        d2.insert("process_count", Bson::Int64(46));

        let rollups = compute_rollup(
            "sockets",
            &[d1, d2],
            1_700_000_000_000,
            1_700_003_600_000,
            Resolution::Hourly,
        );
        assert_eq!(rollups.len(), 1);
        let r = &rollups[0];
        assert_eq!(r.tcp_inuse_mean, Some(110.0));
        assert_eq!(r.tcp_inuse_min, Some(100.0));
        assert_eq!(r.tcp_inuse_max, Some(120.0));
        assert_eq!(r.udp_inuse_mean, Some(50.0));
        assert_eq!(r.total_sockets_mean, Some(330.0));
        assert_eq!(r.process_count_mean, Some(44.0));
    }

    // int32 values must keep working too (some collectors may emit them).
    #[test]
    fn test_compute_rollup_sockets_reads_int32_fields() {
        use bson::{Bson, Document};

        let mut d1 = Document::new();
        d1.insert("tcp_inuse", Bson::Int32(100));
        d1.insert("udp_inuse", Bson::Int32(40));
        d1.insert("total_sockets", Bson::Int32(300));
        d1.insert("process_count", Bson::Int64(42));

        let rollups = compute_rollup(
            "sockets",
            &[d1],
            1_700_000_000_000,
            1_700_003_600_000,
            Resolution::Hourly,
        );
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].tcp_inuse_mean, Some(100.0));
        assert_eq!(rollups[0].udp_inuse_mean, Some(40.0));
        assert_eq!(rollups[0].total_sockets_mean, Some(300.0));
    }

    #[test]
    fn test_compute_rollup_network_groups_by_interface() {
        use bson::{Bson, Document};

        let mut d1 = Document::new();
        d1.insert("interface", Bson::String("eth0".to_string()));
        d1.insert("rx_bytes_per_sec", Bson::Double(1000.0));
        d1.insert("tx_bytes_per_sec", Bson::Double(500.0));

        let mut d2 = d1.clone();
        d2.insert("rx_bytes_per_sec", Bson::Double(2000.0));
        d2.insert("tx_bytes_per_sec", Bson::Double(600.0));

        let mut d3 = Document::new();
        d3.insert("interface", Bson::String("wlan0".to_string()));
        d3.insert("rx_bytes_per_sec", Bson::Double(50.0));
        d3.insert("tx_bytes_per_sec", Bson::Double(25.0));

        let rollups = compute_rollup("network", &[d1, d2, d3], 0, 3_600_000, Resolution::Hourly);
        assert_eq!(rollups.len(), 2);
        // BTreeMap ordering: eth0 sorts before wlan0.
        assert_eq!(rollups[0].interface.as_deref(), Some("eth0"));
        assert_eq!(rollups[0].net_rx_mean, Some(1500.0));
        assert_eq!(rollups[0].net_tx_mean, Some(550.0));
        assert_eq!(rollups[1].interface.as_deref(), Some("wlan0"));
        assert_eq!(rollups[1].net_rx_mean, Some(50.0));
        assert_eq!(rollups[1].net_tx_mean, Some(25.0));
    }

    // Regression: 0.1.2 dropped the disk arm entirely (`_ => vec![]`), so disk
    // rollups silently stopped being written.
    #[test]
    fn test_compute_rollup_disk_uses_used_percent() {
        use bson::{Bson, Document};

        let mut d1 = Document::new();
        d1.insert("used_percent", Bson::Double(40.0));
        let mut d2 = Document::new();
        d2.insert("used_percent", Bson::Double(60.0));

        let rollups = compute_rollup("disk", &[d1, d2], 0, 3_600_000, Resolution::Hourly);
        assert_eq!(rollups.len(), 1);
        let r = &rollups[0];
        assert_eq!(r.mem_used_mean, Some(50.0));
        assert_eq!(r.mem_used_min, Some(40.0));
        assert_eq!(r.mem_used_max, Some(60.0));
    }
}
