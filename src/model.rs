//! Data model types for TheWatcher.
//!
//! All persisted timestamps use UTC epoch milliseconds.

use serde::{Deserialize, Serialize};

/// Current system snapshot, served at GET /api/current
#[derive(Debug, Clone, Serialize)]
pub struct CurrentSnapshot {
    pub timestamp_ms: i64,
    pub hostname: String,
    pub uptime_seconds: u64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetworkSnapshot>,
    pub processes: ProcessSnapshot,
    pub sockets: SocketsSnapshot,
    pub collector_status: Vec<CollectorStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuSnapshot {
    pub percent: Option<f64>,
    pub per_core: Option<Vec<f64>>,
    pub load_1: Option<f64>,
    pub load_5: Option<f64>,
    pub load_15: Option<f64>,
    pub logical_cpus: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: Option<u64>,
    pub used_percent: f64,
    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub swap_used_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskSnapshot {
    pub mount: String,
    pub filesystem: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkSnapshot {
    pub interface: String,
    pub operational: Option<bool>,
    pub rx_bytes_total: u64,
    pub tx_bytes_total: u64,
    pub rx_packets_total: u64,
    pub tx_packets_total: u64,
    pub rx_bytes_per_sec: Option<f64>,
    pub tx_bytes_per_sec: Option<f64>,
    pub rx_errors: Option<u64>,
    pub tx_errors: Option<u64>,
    pub rx_dropped: Option<u64>,
    pub tx_dropped: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SocketsSnapshot {
    pub tcp_inuse: Option<u32>,
    pub udp_inuse: Option<u32>,
    pub total_sockets: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessSnapshot {
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorStatus {
    pub component: String,
    pub status: String, // "ok", "degraded", "unavailable"
    pub message: Option<String>,
    pub last_success_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Granular sample documents (stored in MooFile)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularCpu {
    #[serde(rename = "_id")]
    pub id: String,
    pub timestamp_ms: i64,
    pub metric: String, // "cpu"
    pub resolution: String, // "granular"
    pub cpu_percent: f64,
    pub load_1: Option<f64>,
    pub load_5: Option<f64>,
    pub load_15: Option<f64>,
    pub logical_cpus: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularMemory {
    #[serde(rename = "_id")]
    pub id: String,
    pub timestamp_ms: i64,
    pub metric: String,
    pub resolution: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: Option<u64>,
    pub used_percent: f64,
    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularDisk {
    #[serde(rename = "_id")]
    pub id: String,
    pub timestamp_ms: i64,
    pub metric: String,
    pub resolution: String,
    pub mount: String,
    pub filesystem: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularNetwork {
    #[serde(rename = "_id")]
    pub id: String,
    pub timestamp_ms: i64,
    pub metric: String,
    pub resolution: String,
    pub interface: String,
    pub rx_bytes_total: u64,
    pub tx_bytes_total: u64,
    pub rx_bytes_per_sec: Option<f64>,
    pub tx_bytes_per_sec: Option<f64>,
    pub rx_packets_total: u64,
    pub tx_packets_total: u64,
    pub operational: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularSockets {
    #[serde(rename = "_id")]
    pub id: String,
    pub timestamp_ms: i64,
    pub metric: String,
    pub resolution: String,
    pub process_count: u64,
    pub tcp_inuse: Option<u32>,
    pub udp_inuse: Option<u32>,
    pub total_sockets: Option<u32>,
}

// ---------------------------------------------------------------------------
// Rollup documents
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollupDoc {
    #[serde(rename = "_id")]
    pub id: String,
    pub bucket_start_ms: i64,
    pub bucket_end_ms: i64,
    pub timestamp_ms: i64,
    pub metric: String,
    pub resolution: String,
    pub sample_count: u64,
    pub cpu_min: Option<f64>,
    pub cpu_mean: Option<f64>,
    pub cpu_max: Option<f64>,
    pub load_1_min: Option<f64>,
    pub load_1_mean: Option<f64>,
    pub load_1_max: Option<f64>,
    pub mem_used_min: Option<f64>,
    pub mem_used_mean: Option<f64>,
    pub mem_used_max: Option<f64>,
}

// ---------------------------------------------------------------------------
// History API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub metric: String,
    pub range: Option<String>,
    pub from: Option<i64>,
    pub until: Option<i64>,
    pub resolution: Option<String>,
    #[serde(rename = "interface")]
    pub interface: Option<String>,
    pub mount: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryResponse {
    pub metric: String,
    pub resolution: String,
    pub from_ms: i64,
    pub until_ms: i64,
    pub series: Vec<HistorySeries>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySeries {
    pub name: String,
    pub unit: String,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryPoint {
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub last_collection_ms: Option<i64>,
    pub last_rollup_ms: Option<i64>,
    pub storage: String,
}

// ---------------------------------------------------------------------------
// Info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InfoResponse {
    pub version: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub start_time_ms: i64,
    pub boot_time_ms: Option<i64>,
    pub data_dir: String,
    pub listen_addr: String,
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Resolution levels for the RRD-style storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resolution {
    Granular,
    Hourly,
    Daily,
    Monthly,
    Yearly,
}

impl Resolution {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::Granular => "granular",
            Resolution::Hourly => "hourly",
            Resolution::Daily => "daily",
            Resolution::Monthly => "monthly",
            Resolution::Yearly => "yearly",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "granular" => Some(Resolution::Granular),
            "hourly" => Some(Resolution::Hourly),
            "daily" => Some(Resolution::Daily),
            "monthly" => Some(Resolution::Monthly),
            "yearly" => Some(Resolution::Yearly),
            _ => None,
        }
    }

    /// How many milliseconds in one bucket of this resolution
    pub fn bucket_ms(&self) -> i64 {
        match self {
            Resolution::Granular => 0, // not bucketed
            Resolution::Hourly => 3_600_000,
            Resolution::Daily => 86_400_000,
            Resolution::Monthly => 2_592_000_000, // 30 days
            Resolution::Yearly => 31_536_000_000, // 365 days
        }
    }

    /// Auto-select resolution based on time range duration in ms
    pub fn auto_select(duration_ms: i64) -> Self {
        if duration_ms <= 3_600_000 {
            // <= 1 hour
            Resolution::Granular
        } else if duration_ms <= 86_400_000 {
            // <= 1 day
            Resolution::Hourly
        } else if duration_ms <= 2_592_000_000 {
            // <= 30 days
            Resolution::Daily
        } else if duration_ms <= 31_536_000_000 {
            // <= 365 days
            Resolution::Monthly
        } else {
            Resolution::Yearly
        }
    }
}
