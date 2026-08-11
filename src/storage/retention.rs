//! Retention policy enforcement for MooFile collections.
//!
//! Deletes documents older than configured boundaries and triggers
//! compaction when dead space exceeds a threshold.

use crate::config::Config;
use crate::storage::Storage;
use bson::doc;
use tracing;

/// Run retention cleanup across all resolution collections.
pub fn run_retention(storage: &Storage, config: &Config) -> Result<(), String> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let policies = [
        ("granular", config.granular_retention_days),
        ("hourly", config.hourly_retention_days),
        ("daily", config.daily_retention_days),
        ("monthly", config.monthly_retention_days),
        ("yearly", config.yearly_retention_days),
    ];

    for (resolution, days) in &policies {
        if *days == 0 {
            continue; // indefinite
        }

        let cutoff_ms = now_ms - (*days as i64 * 86_400_000);
        delete_before(storage, resolution, cutoff_ms)?;
    }

    // Check compaction thresholds
    compact_if_needed(storage)?;

    Ok(())
}

/// Delete documents with timestamp_ms < cutoff_ms.
fn delete_before(storage: &Storage, resolution: &str, cutoff_ms: i64) -> Result<(), String> {
    let filter = doc! {
        "timestamp_ms": { "$lt": cutoff_ms },
    };

    let collection = match resolution {
        "granular" => storage.granular_collection(),
        "hourly" => storage.hourly_collection(),
        "daily" => storage.daily_collection(),
        "monthly" => storage.monthly_collection(),
        "yearly" => storage.yearly_collection(),
        _ => return Err(format!("Unknown resolution: {}", resolution)),
    };

    let deleted = collection
        .delete_many(filter)
        .map_err(|e| format!("retention delete {}: {}", resolution, e))?;

    if deleted > 0 {
        tracing::info!(
            "Retention: deleted {} documents from {} (cutoff: {} days ago)",
            deleted,
            resolution,
            (chrono::Utc::now().timestamp_millis() - cutoff_ms) / 86_400_000,
        );
    }

    Ok(())
}

/// Check dead_ratio and compact collections exceeding the threshold.
fn compact_if_needed(storage: &Storage) -> Result<(), String> {
    let threshold = 0.30; // 30% dead space

    let collections: [(&str, &moofile::Collection); 5] = [
        ("granular", storage.granular_collection()),
        ("hourly", storage.hourly_collection()),
        ("daily", storage.daily_collection()),
        ("monthly", storage.monthly_collection()),
        ("yearly", storage.yearly_collection()),
    ];

    for (name, col) in &collections {
        match col.stats() {
            Ok(stats) => {
                if stats.dead_ratio > threshold {
                    tracing::info!(
                        "Compacting {} (dead_ratio={:.2}%, {} dead / {} live)",
                        name,
                        stats.dead_ratio * 100.0,
                        stats.dead_records,
                        stats.documents,
                    );
                    if let Err(e) = col.compact() {
                        tracing::error!("Failed to compact {}: {}", name, e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get stats for {}: {}", name, e);
            }
        }
    }

    Ok(())
}
