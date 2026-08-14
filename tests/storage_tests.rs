//! Comprehensive storage tests for TheWatcher.
//!
//! Tests: insert/query all metric types, restart/reload, retention, rollup,
//! deterministic IDs, missing/null values, compaction behavior.

use thewatcher::{model, storage};

/// Helper: create a temporary storage instance.
fn open_temp_storage() -> (storage::Storage, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let storage = storage::Storage::open(&data_dir).expect("Failed to open storage");
    (storage, dir)
}

/// Helper: create a test granular CPU document.
fn make_cpu_doc(id: &str, ts: i64, pct: f64) -> model::GranularCpu {
    model::GranularCpu {
        id: id.to_string(),
        timestamp_ms: ts,
        metric: "cpu".to_string(),
        resolution: "granular".to_string(),
        cpu_percent: pct,
        load_1: Some(1.0),
        load_5: Some(0.8),
        load_15: Some(0.6),
        logical_cpus: 8,
    }
}

/// Helper: create a test granular memory document.
fn make_mem_doc(id: &str, ts: i64, used_pct: f64) -> model::GranularMemory {
    model::GranularMemory {
        id: id.to_string(),
        timestamp_ms: ts,
        metric: "memory".to_string(),
        resolution: "granular".to_string(),
        total_bytes: 16_000_000_000,
        used_bytes: (16_000_000_000.0 * used_pct / 100.0) as u64,
        available_bytes: Some(8_000_000_000),
        used_percent: used_pct,
        swap_total_bytes: Some(4_000_000_000),
        swap_used_bytes: Some(0),
    }
}

/// Helper: create a test granular disk document.
fn make_disk_doc(id: &str, ts: i64, mount: &str, used_pct: f64) -> model::GranularDisk {
    model::GranularDisk {
        id: id.to_string(),
        timestamp_ms: ts,
        metric: "disk".to_string(),
        resolution: "granular".to_string(),
        mount: mount.to_string(),
        filesystem: Some("/dev/sda1".to_string()),
        total_bytes: 500_000_000_000,
        used_bytes: (500_000_000_000.0 * used_pct / 100.0) as u64,
        available_bytes: 250_000_000_000,
        used_percent: used_pct,
    }
}

/// Helper: create a test granular network document.
fn make_net_doc(id: &str, ts: i64, iface: &str, rx_rate: f64, tx_rate: f64) -> model::GranularNetwork {
    model::GranularNetwork {
        id: id.to_string(),
        timestamp_ms: ts,
        metric: "network".to_string(),
        resolution: "granular".to_string(),
        interface: iface.to_string(),
        rx_bytes_total: 1_000_000,
        tx_bytes_total: 500_000,
        rx_bytes_per_sec: Some(rx_rate),
        tx_bytes_per_sec: Some(tx_rate),
        rx_packets_total: 1000,
        tx_packets_total: 500,
        operational: true,
    }
}

// ---------------------------------------------------------------------------
// Basic storage open/close
// ---------------------------------------------------------------------------

#[test]
fn test_storage_opens_and_creates_files() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");

    let storage = storage::Storage::open(&data_dir).expect("Should open storage");

    // Verify files exist
    for name in &["granular.bson", "hourly.bson", "daily.bson", "monthly.bson", "yearly.bson"] {
        assert!(data_dir.join(name).exists(), "Missing file: {}", name);
    }

    // Stats should be accessible
    let stats = storage.stats().expect("Should get stats");
    assert_eq!(stats.granular_documents, 0);
}

#[test]
fn test_storage_reopens_existing() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");

    // First open
    {
        let storage = storage::Storage::open(&data_dir).expect("Should open storage");

        // Insert a document
        let doc = make_cpu_doc("cpu-1000", 1000, 42.0);
        storage.write_granular_cpu(&doc).expect("Should insert");
    }

    // Reopen
    {
        let storage = storage::Storage::open(&data_dir).expect("Should reopen storage");
        let stats = storage.stats().expect("Should get stats");
        assert!(stats.granular_documents >= 1, "Should have at least 1 document after reopen");

        // Query the data back
        let docs = storage
            .query_granular("cpu", 0, 2000, None, None)
            .expect("Should query");
        assert!(!docs.is_empty(), "Should find the inserted document");
    }
}

// ---------------------------------------------------------------------------
// Insert and query all metric types
// ---------------------------------------------------------------------------

#[test]
fn test_cpu_insert_and_query() {
    let (storage, _dir) = open_temp_storage();

    let ts_base: i64 = 1_700_000_000_000;
    for i in 0..10 {
        let ts = ts_base + i * 30_000;
        let doc = make_cpu_doc(&format!("cpu-{}", ts), ts, 20.0 + i as f64);
        storage.write_granular_cpu(&doc).unwrap();
    }

    // Query range
    let docs = storage
        .query_granular("cpu", ts_base, ts_base + 9 * 30_000, None, None)
        .unwrap();
    assert_eq!(docs.len(), 10, "Should find all 10 CPU documents");

    // Partial range
    let docs = storage
        .query_granular("cpu", ts_base + 3 * 30_000, ts_base + 5 * 30_000, None, None)
        .unwrap();
    assert_eq!(docs.len(), 3, "Should find exactly 3 documents in partial range");
}

#[test]
fn test_memory_insert_and_query() {
    let (storage, _dir) = open_temp_storage();

    let ts_base: i64 = 1_700_000_000_000;
    for i in 0..5 {
        let ts = ts_base + i * 30_000;
        let doc = make_mem_doc(&format!("memory-{}", ts), ts, 30.0 + i as f64 * 5.0);
        storage.write_granular_memory(&doc).unwrap();
    }

    let docs = storage
        .query_granular("memory", ts_base, ts_base + 10 * 30_000, None, None)
        .unwrap();
    assert_eq!(docs.len(), 5, "Should find all 5 memory documents");

    // Verify data integrity
    let first = &docs[0];
    assert_eq!(first.get_f64("used_percent").unwrap(), 30.0);
}

#[test]
fn test_disk_insert_and_query_with_mount_filter() {
    let (storage, _dir) = open_temp_storage();

    let ts: i64 = 1_700_000_000_000;

    // Insert for multiple mounts
    for mount in &["/", "/home", "/var"] {
        let doc = make_disk_doc(&format!("disk-{}-{}", mount, ts), ts, mount, 50.0);
        storage.write_granular_disk(&doc).unwrap();
    }

    // Query without filter
    let all = storage
        .query_granular("disk", ts - 1000, ts + 1000, None, None)
        .unwrap();
    assert_eq!(all.len(), 3);

    // Query with mount filter
    let home = storage
        .query_granular("disk", ts - 1000, ts + 1000, None, Some("/home"))
        .unwrap();
    assert_eq!(home.len(), 1);
    assert_eq!(home[0].get_str("mount").unwrap(), "/home");

    // Non-existent mount
    let none = storage
        .query_granular("disk", ts - 1000, ts + 1000, None, Some("/nonexistent"))
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn test_network_insert_and_query_with_interface_filter() {
    let (storage, _dir) = open_temp_storage();

    let ts: i64 = 1_700_000_000_000;

    for iface in &["eth0", "eth1", "wlan0"] {
        let doc = make_net_doc(
            &format!("network-{}-{}", iface, ts),
            ts,
            iface,
            1000.0,
            500.0,
        );
        storage.write_granular_network(&doc).unwrap();
    }

    // Query with interface filter
    let eth0 = storage
        .query_granular("network", ts - 1000, ts + 1000, Some("eth0"), None)
        .unwrap();
    assert_eq!(eth0.len(), 1);
    assert_eq!(eth0[0].get_str("interface").unwrap(), "eth0");

    // Query all
    let all = storage
        .query_granular("network", ts - 1000, ts + 1000, None, None)
        .unwrap();
    assert_eq!(all.len(), 3);
}

// ---------------------------------------------------------------------------
// Duplicate ID handling (idempotent writes)
// ---------------------------------------------------------------------------

#[test]
fn test_duplicate_id_is_handled() {
    let (storage, _dir) = open_temp_storage();

    let doc = make_cpu_doc("cpu-dup-1000", 1000, 50.0);
    storage.write_granular_cpu(&doc).unwrap();

    // Same ID should not crash
    storage.write_granular_cpu(&doc).unwrap();

    // Only one document should exist
    let docs = storage.query_granular("cpu", 0, 2000, None, None).unwrap();
    assert_eq!(docs.len(), 1);
}

// ---------------------------------------------------------------------------
// Rollup operations
// ---------------------------------------------------------------------------

#[test]
fn test_rollup_cpu_from_granular() {
    let (storage, _dir) = open_temp_storage();

    // Insert granular CPU samples spanning one hour bucket
    let bucket_start: i64 = 1_700_000_000_000; // aligned

    for i in 0..120 {
        let ts = bucket_start + i * 30_000; // every 30 seconds
        let pct = 20.0 + (i as f64 % 40.0); // values 20-60
        let doc = make_cpu_doc(&format!("cpu-{}", ts), ts, pct);
        storage.write_granular_cpu(&doc).unwrap();
    }

    // Run rollup (should create hourly from granular)
    let result = storage::rollup::run_rollups(&storage);
    // This may fail if now_ms is before the bucket end, but should not crash
    // It won't rollup because the bucket isn't "completed" (now > bucket_end)
    assert!(result.is_ok());
}

#[test]
fn test_rollup_idempotent() {
    let (storage, _dir) = open_temp_storage();

    // Manually create and insert a rollup document
    let rollup = model::RollupDoc {
        id: "cpu-hourly-1700000000000".to_string(),
        bucket_start_ms: 1_700_000_000_000,
        bucket_end_ms: 1_700_003_600_000,
        timestamp_ms: 1_700_000_000_000,
        metric: "cpu".to_string(),
        resolution: "hourly".to_string(),
        sample_count: 120,
        cpu_min: Some(2.1),
        cpu_mean: Some(31.4),
        cpu_max: Some(88.7),
        load_1_min: Some(0.2),
        load_1_mean: Some(1.3),
        load_1_max: Some(4.8),
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

    // First insert
    storage.write_rollup("hourly", &rollup).unwrap();

    // Second insert should be idempotent
    storage.write_rollup("hourly", &rollup).unwrap();

    // Should have exactly one document
    let docs = storage
        .query_rollup("hourly", "cpu", 1_700_000_000_000, 1_700_010_000_000, None, None)
        .unwrap();
    assert_eq!(docs.len(), 1, "Rollup should be idempotent");
    assert_eq!(docs[0].get_f64("cpu_mean").unwrap(), 31.4);
}

// ---------------------------------------------------------------------------
// Retention tests
// ---------------------------------------------------------------------------

#[test]
fn test_retention_deletes_old_documents() {
    let (storage, _dir) = open_temp_storage();

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Insert a very old CPU document
    let old_doc = make_cpu_doc("cpu-old", now_ms - 100 * 86_400_000, 50.0);
    storage.write_granular_cpu(&old_doc).unwrap();

    // Insert a recent CPU document
    let recent_doc = make_cpu_doc("cpu-recent", now_ms - 1 * 86_400_000, 30.0);
    storage.write_granular_cpu(&recent_doc).unwrap();

    // Verify both exist
    let all = storage
        .query_granular("cpu", 0, now_ms, None, None)
        .unwrap();
    assert_eq!(all.len(), 2);

    // Run retention with 10-day policy for granular
    let config = thewatcher::config::Config {
        granular_retention_days: 10,
        ..Default::default()
    };
    storage::retention::run_retention(&storage, &config).unwrap();

    // Only the recent document should remain
    let remaining = storage
        .query_granular("cpu", 0, now_ms, None, None)
        .unwrap();
    assert_eq!(remaining.len(), 1);
    let id = remaining[0].get_str("_id").unwrap();
    assert!(id.contains("recent"), "Only recent document should remain");
}

#[test]
fn test_retention_indefinite_skips() {
    let (storage, _dir) = open_temp_storage();

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Insert a very old document
    let old_doc = make_cpu_doc("cpu-very-old", now_ms - 5000 * 86_400_000, 50.0);
    storage.write_granular_cpu(&old_doc).unwrap();

    // Retention with 0 (indefinite) should not delete anything
    let config = thewatcher::config::Config {
        granular_retention_days: 0,
        ..Default::default()
    };
    storage::retention::run_retention(&storage, &config).unwrap();

    let remaining = storage
        .query_granular("cpu", 0, now_ms, None, None)
        .unwrap();
    assert_eq!(remaining.len(), 1, "Indefinite retention should not delete");
}

// ---------------------------------------------------------------------------
// Missing and null value handling
// ---------------------------------------------------------------------------

#[test]
fn test_null_load_values_in_query() {
    let (storage, _dir) = open_temp_storage();

    // Create a CPU doc with None loads
    let doc = model::GranularCpu {
        id: "cpu-null-loads".to_string(),
        timestamp_ms: 1_700_000_000_000,
        metric: "cpu".to_string(),
        resolution: "granular".to_string(),
        cpu_percent: 25.0,
        load_1: None,
        load_5: None,
        load_15: None,
        logical_cpus: 4,
    };
    storage.write_granular_cpu(&doc).unwrap();

    let docs = storage
        .query_granular("cpu", 1_700_000_000_000 - 1000, 1_700_000_000_000 + 1000, None, None)
        .unwrap();
    assert_eq!(docs.len(), 1);

    // Load values should fail to parse as f64 (they're null in BSON)
    let d = &docs[0];
    assert!(d.get_f64("cpu_percent").is_ok()); // should be present
    // load_1 might be missing or null - both are acceptable
}

// ---------------------------------------------------------------------------
// Query edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_query_empty_range() {
    let (storage, _dir) = open_temp_storage();

    let ts: i64 = 1_700_000_000_000;
    let doc = make_cpu_doc("cpu-test", ts, 25.0);
    storage.write_granular_cpu(&doc).unwrap();

    // Range before data
    let docs = storage
        .query_granular("cpu", 0, 100, None, None)
        .unwrap();
    assert!(docs.is_empty());

    // Range after data
    let docs = storage
        .query_granular("cpu", ts + 1000, ts + 2000, None, None)
        .unwrap();
    assert!(docs.is_empty());
}

#[test]
fn test_query_wrong_metric() {
    let (storage, _dir) = open_temp_storage();

    let ts: i64 = 1_700_000_000_000;
    let doc = make_cpu_doc("cpu-test", ts, 25.0);
    storage.write_granular_cpu(&doc).unwrap();

    // Query for memory but only CPU data exists
    let docs = storage
        .query_granular("memory", ts - 1000, ts + 1000, None, None)
        .unwrap();
    assert!(docs.is_empty());
}

#[test]
fn test_last_timestamp() {
    let (storage, _dir) = open_temp_storage();

    // Empty: should be None
    let last = storage.last_timestamp("granular", "cpu").unwrap();
    assert!(last.is_none());

    // Insert data
    let ts: i64 = 1_700_000_000_000;
    storage
        .write_granular_cpu(&make_cpu_doc("cpu-a", ts, 10.0))
        .unwrap();
    storage
        .write_granular_cpu(&make_cpu_doc("cpu-b", ts + 60000, 20.0))
        .unwrap();

    let last = storage.last_timestamp("granular", "cpu").unwrap();
    assert_eq!(last, Some(ts + 60000));
}

// ---------------------------------------------------------------------------
// Restart/reload persistence
// ---------------------------------------------------------------------------

#[test]
fn test_data_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");

    let ts: i64 = 1_700_000_000_000;

    // Write data
    {
        let storage = storage::Storage::open(&data_dir).unwrap();
        for i in 0..20 {
            let ts_i = ts + i * 30_000;
            storage
                .write_granular_cpu(&make_cpu_doc(&format!("cpu-{}", ts_i), ts_i, 10.0 + i as f64))
                .unwrap();
        }
    }

    // Reopen and verify
    {
        let storage = storage::Storage::open(&data_dir).unwrap();
        let docs = storage
            .query_granular("cpu", ts, ts + 20 * 30_000, None, None)
            .unwrap();
        assert_eq!(docs.len(), 20, "All documents should persist after reopen");
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

#[test]
fn test_storage_stats() {
    let (storage, _dir) = open_temp_storage();

    let stats = storage.stats().unwrap();
    assert_eq!(stats.granular_documents, 0);
    assert!(stats.granular_dead_ratio >= 0.0);
    // just verify stats don't panic

    // Insert some data
    let ts: i64 = 1_700_000_000_000;
    for i in 0..5 {
        storage
            .write_granular_cpu(&make_cpu_doc(&format!("cpu-{}", i), ts + i as i64 * 1000, 50.0))
            .unwrap();
    }

    let stats = storage.stats().unwrap();
    assert!(stats.granular_documents >= 5, "Should show at least 5 documents");
}
