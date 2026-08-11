//! RRD-style rollup from granular → hourly → daily → monthly → yearly.

use crate::model::*;
use crate::storage::Storage;
use bson::Document;
use tracing;

/// Perform rollups for all completed buckets.
pub fn run_rollups(storage: &Storage) -> Result<i64, String> {
    let now_ms = chrono::Utc::now().timestamp_millis();

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
        let start_from = last_rollup.unwrap_or(0);

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
            )?;

            if source_docs.is_empty() {
                bucket_start += bucket_ms;
                continue;
            }

            // Compute rollup
            if let Some(rollup) = compute_rollup(metric, &source_docs, bucket_start, bucket_end, dst)
            {
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

/// Compute a single rollup document from source documents.
fn compute_rollup(
    metric: &str,
    source_docs: &[Document],
    bucket_start: i64,
    bucket_end: i64,
    resolution: Resolution,
) -> Option<RollupDoc> {
    if source_docs.is_empty() {
        return None;
    }

    let sample_count = source_docs.len() as u64;

    let mut rollup = RollupDoc {
        id: format!(
            "{}-{}-{}",
            metric,
            resolution.as_str(),
            bucket_start
        ),
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
    };

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
        }
        "memory" => {
            let values: Vec<f64> = source_docs
                .iter()
                .filter_map(|d| d.get_f64("used_percent").ok())
                .collect();
            if !values.is_empty() {
                rollup.mem_used_min = min(&values);
                rollup.mem_used_mean = Some(mean(&values));
                rollup.mem_used_max = max(&values);
            }
        }
        _ => {}
    }

    Some(rollup)
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
}
