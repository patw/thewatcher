//! MooFile-based storage for TheWatcher.
//!
//! Uses separate MooFile collections by resolution:
//! - granular.bson
//! - hourly.bson
//! - daily.bson
//! - monthly.bson
//! - yearly.bson

pub mod rollup;
pub mod retention;

use crate::model::*;
use bson::{doc, Document};
use moofile::Collection;
use std::path::{Path, PathBuf};
use tracing;

/// Storage manager holding all MooFile collections.
pub struct Storage {
    data_dir: PathBuf,
    granular: Collection,
    hourly: Collection,
    daily: Collection,
    monthly: Collection,
    yearly: Collection,
}

impl Storage {
    /// Open or create all MooFile collections.
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("Failed to create data dir {}: {}", data_dir.display(), e))?;

        let granular = open_collection(&data_dir.join("granular.bson"))?;
        let hourly = open_collection(&data_dir.join("hourly.bson"))?;
        let daily = open_collection(&data_dir.join("daily.bson"))?;
        let monthly = open_collection(&data_dir.join("monthly.bson"))?;
        let yearly = open_collection(&data_dir.join("yearly.bson"))?;

        tracing::info!("Storage opened at {}", data_dir.display());
        tracing::info!(
            "Documents: granular={}, hourly={}, daily={}, monthly={}, yearly={}",
            granular.count(doc! {}).unwrap_or(0),
            hourly.count(doc! {}).unwrap_or(0),
            daily.count(doc! {}).unwrap_or(0),
            monthly.count(doc! {}).unwrap_or(0),
            yearly.count(doc! {}).unwrap_or(0),
        );

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            granular,
            hourly,
            daily,
            monthly,
            yearly,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    // -----------------------------------------------------------------------
    // Granular writes
    // -----------------------------------------------------------------------

    pub fn write_granular_cpu(&self, doc: &GranularCpu) -> Result<(), String> {
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;
        self.granular_insert(&bson_doc, "cpu")?;
        Ok(())
    }

    pub fn write_granular_memory(&self, doc: &GranularMemory) -> Result<(), String> {
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;
        self.granular_insert(&bson_doc, "memory")?;
        Ok(())
    }

    pub fn write_granular_disk(&self, doc: &GranularDisk) -> Result<(), String> {
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;
        self.granular_insert(&bson_doc, "disk")?;
        Ok(())
    }

    pub fn write_granular_network(&self, doc: &GranularNetwork) -> Result<(), String> {
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;
        self.granular_insert(&bson_doc, "network")?;
        Ok(())
    }

    pub fn write_granular_sockets(&self, doc: &GranularSockets) -> Result<(), String> {
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;
        self.granular_insert(&bson_doc, "sockets")?;
        Ok(())
    }

    /// Idempotent insert: duplicate key is OK, other errors propagate.
    fn granular_insert(&self, doc: &bson::Document, label: &str) -> Result<(), String> {
        match self.granular.insert(doc.clone()) {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("duplicate") || err_str.contains("DuplicateKey") {
                    Ok(())
                } else {
                    Err(format!("granular insert {}: {}", label, e))
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Rollup writes (idempotent via deterministic IDs)
    // -----------------------------------------------------------------------

    pub fn write_rollup(&self, resolution: &str, doc: &RollupDoc) -> Result<(), String> {
        let collection = self.collection_for_resolution(resolution)?;
        let bson_doc = bson::to_document(doc).map_err(|e| format!("bson serialize: {}", e))?;

        // Idempotent: try insert, ignore duplicate key errors
        match collection.insert(bson_doc) {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("duplicate") || err_str.contains("DuplicateKey") {
                    Ok(())
                } else {
                    Err(format!("rollup insert {}: {}", resolution, e))
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // History queries
    // -----------------------------------------------------------------------

    pub fn query_granular(
        &self,
        metric: &str,
        from_ms: i64,
        until_ms: i64,
        interface: Option<&str>,
        mount: Option<&str>,
    ) -> Result<Vec<Document>, String> {
        let mut filter = doc! {
            "metric": metric,
            "timestamp_ms": { "$gte": from_ms, "$lte": until_ms },
        };

        if let Some(iface) = interface {
            filter.insert("interface", iface);
        }
        if let Some(mnt) = mount {
            filter.insert("mount", mnt);
        }

        let docs = self
            .granular
            .find(filter.clone())
            .map_err(|e| format!("query granular: {}", e))?
            .sort("timestamp_ms", false) // ascending (chronological)
            .to_list()
            .map_err(|e| format!("query granular: {}", e))?;

        Ok(docs)
    }

    pub fn query_rollup(
        &self,
        resolution: &str,
        metric: &str,
        from_ms: i64,
        until_ms: i64,
        interface: Option<&str>,
        mount: Option<&str>,
    ) -> Result<Vec<Document>, String> {
        let collection = self.collection_for_resolution(resolution)?;

        let mut filter = doc! {
            "metric": metric,
            "timestamp_ms": { "$gte": from_ms, "$lte": until_ms },
        };

        if let Some(iface) = interface {
            filter.insert("interface", iface);
        }
        if let Some(mnt) = mount {
            filter.insert("mount", mnt);
        }

        let docs = collection
            .find(filter)
            .map_err(|e| format!("query {}: {}", resolution, e))?
            .sort("timestamp_ms", false) // ascending (chronological)
            .to_list()
            .map_err(|e| format!("query {}: {}", resolution, e))?;

        Ok(docs)
    }

    /// Get the most recent timestamp in a collection for a given metric
    pub fn last_timestamp(&self, resolution: &str, metric: &str) -> Result<Option<i64>, String> {
        let collection = self.collection_for_resolution(resolution)?;

        let docs = collection
            .find(doc! { "metric": metric })
            .map_err(|e| format!("last_timestamp query: {}", e))?
            .sort("timestamp_ms", true) // descending (newest first)
            .limit(1)
            .to_list()
            .map_err(|e| format!("last_timestamp query: {}", e))?;

        Ok(docs
            .first()
            .and_then(|d| d.get_i64("timestamp_ms").ok()))
    }

    /// Get storage stats
    pub fn stats(&self) -> Result<StorageStats, String> {
        let granular = self
            .granular
            .stats()
            .map_err(|e| format!("stats granular: {}", e))?;
        let hourly = self
            .hourly
            .stats()
            .map_err(|e| format!("stats hourly: {}", e))?;
        let daily = self
            .daily
            .stats()
            .map_err(|e| format!("stats daily: {}", e))?;
        let monthly = self
            .monthly
            .stats()
            .map_err(|e| format!("stats monthly: {}", e))?;
        let yearly = self
            .yearly
            .stats()
            .map_err(|e| format!("stats yearly: {}", e))?;

        Ok(StorageStats {
            granular_documents: granular.documents,
            granular_dead_ratio: granular.dead_ratio,
            granular_file_size: granular.file_size_bytes,
            hourly_documents: hourly.documents,
            hourly_dead_ratio: hourly.dead_ratio,
            daily_documents: daily.documents,
            monthly_documents: monthly.documents,
            yearly_documents: yearly.documents,
        })
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn collection_for_resolution(&self, resolution: &str) -> Result<&Collection, String> {
        match resolution {
            "granular" => Ok(&self.granular),
            "hourly" => Ok(&self.hourly),
            "daily" => Ok(&self.daily),
            "monthly" => Ok(&self.monthly),
            "yearly" => Ok(&self.yearly),
            _ => Err(format!("Unknown resolution: {}", resolution)),
        }
    }

    /// Expose collections for rollup/retention workers
    pub fn granular_collection(&self) -> &Collection {
        &self.granular
    }
    pub fn hourly_collection(&self) -> &Collection {
        &self.hourly
    }
    pub fn daily_collection(&self) -> &Collection {
        &self.daily
    }
    pub fn monthly_collection(&self) -> &Collection {
        &self.monthly
    }
    pub fn yearly_collection(&self) -> &Collection {
        &self.yearly
    }

    /// Delete documents matching a filter from the given resolution collection.
    pub fn delete_many(&self, resolution: &str, filter: bson::Document) -> Result<u64, String> {
        let collection = self.collection_for_resolution(resolution)?;
        let deleted = collection
            .delete_many(filter)
            .map_err(|e| format!("delete_many {}: {}", resolution, e))?;
        Ok(deleted as u64)
    }
}

fn open_collection(path: &Path) -> Result<Collection, String> {
    Collection::builder(path)
        .index("timestamp_ms")
        .index("metric")
        .index("interface")
        .index("mount")
        .open()
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StorageStats {
    pub granular_documents: u64,
    pub granular_dead_ratio: f64,
    pub granular_file_size: u64,
    pub hourly_documents: u64,
    pub hourly_dead_ratio: f64,
    pub daily_documents: u64,
    pub monthly_documents: u64,
    pub yearly_documents: u64,
}
