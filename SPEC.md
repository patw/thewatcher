# TheWatcher Specification

**Version:** 0.1.0  
**Date:** 2026-08-11  
**Status:** Released

## Table of Contents

1. [Overview](#1-overview)
2. [Security Model](#2-security-model)
3. [Command-Line Interface](#3-command-line-interface)
4. [Architecture](#4-architecture)
5. [Metric Collection](#5-metric-collection)
6. [Data Model](#6-data-model)
7. [MooFile Storage](#7-moofile-storage)
8. [Collection & Rollup Scheduling](#8-collection--rollup-scheduling)
9. [HTTP API Reference](#9-http-api-reference)
10. [Web Dashboard](#10-web-dashboard)
11. [Error Handling & Degraded Operation](#11-error-handling--degraded-operation)
12. [Cross-Platform Design](#12-cross-platform-design)
13. [Build & Distribution](#13-build--distribution)
14. [Operational Guidance](#14-operational-guidance)

---

## 1. Overview

TheWatcher is a cross-platform Rust application that:

- Collects current machine statistics (CPU, memory, disk, network, uptime,
  host info).
- Persists historical observations locally using
  [MooFile](https://github.com/beholder/moofile), an append-only embedded
  document store.
- Performs RRD-style rollups from granular observations into hourly, daily,
  monthly, and yearly summaries.
- Serves a browser-based dashboard with gauges and interactive line charts.
- Exposes a read-only JSON API for programmatic access.
- Runs as one executable with embedded web assets.
- Requires no cloud account, external database, telemetry service, or runtime
  installation.

### 1.1 Design Principles

1. **Boring and predictable.** TheWatcher does one thing — show and record
   system metrics — and does it without surprises.
2. **Safe by default.** Loopback-only binding, read-only HTTP surface, no
   privilege escalation.
3. **Self-contained.** One binary. No runtime dependencies. No CDN.
4. **Durable.** Append-only storage means writes are never destructive.
   Deterministic document IDs make every operation idempotent.
5. **Degrade gracefully.** A failed collector (e.g. network counters
   unavailable) must not break the dashboard or stop other collectors.

### 1.2 Target Platforms

| Platform | Architectures |
|---|---|
| Linux | x86_64, aarch64 |
| Windows | x86_64, aarch64 |
| macOS | aarch64 (development validation only) |

---

## 2. Security Model

### 2.1 Binding Defaults

The default listener is **loopback only**: `127.0.0.1:8080`.

TheWatcher must not bind to all interfaces unless explicitly requested via
`--listen 0.0.0.0` (IPv4) or `--listen ::` (IPv6).

### 2.2 Broad-Binding Warning

When `--listen 0.0.0.0` is used, TheWatcher prints to stderr and the
application log:

```
WARNING: TheWatcher is listening on all IPv4 interfaces (0.0.0.0:8080).
System metrics and the read-only API will be reachable from the network.
Use --listen 127.0.0.1 for local-only access, or bind to a specific management address.
TheWatcher does not provide TLS or authentication in this release.
```

For any non-loopback address, the startup log includes the effective listener
address and a reminder that network access control is the operator's
responsibility.

### 2.3 No Authentication (v0.1)

TheWatcher v0.1 intentionally omits:

- Password authentication
- Bearer tokens
- Sessions
- TLS
- Client certificates

This avoids creating an incomplete or misleading security layer. Operators
requiring encrypted remote access should use an SSH tunnel, VPN, firewall
rules, or a reverse proxy.

### 2.4 Read-Only HTTP Surface

All API endpoints are read-only. The server must **never** expose endpoints
that:

- Execute commands or modify system settings
- Modify TheWatcher configuration at runtime
- Delete historical data
- Trigger arbitrary compaction or maintenance
- Read arbitrary files
- Follow or serve filesystem paths supplied by clients

### 2.5 Process Privileges

TheWatcher runs as an unprivileged user. If a collector cannot access a metric
(e.g. requires root to read certain `/proc` entries), it returns `null` with an
explanatory collector status message rather than requiring elevated privileges.

---

## 3. Command-Line Interface

### 3.1 Syntax

```
thewatcher [OPTIONS]
```

### 3.2 Options

| Option | Type | Default | Description |
|---|---|---|---|
| `--listen` | string | `127.0.0.1` | Bind address |
| `--port` | u16 | `8080` | TCP port |
| `--interval` | duration | `30s` | Collection interval (e.g. `5s`, `1m`, `5m`) |
| `--data-dir` | path | platform default | Directory for MooFiles |
| `--granular-retention` | duration | `30d` | Granular sample retention |
| `--hourly-retention` | duration | `365d` | Hourly summary retention |
| `--daily-retention` | duration | `5y` | Daily summary retention |
| `--monthly-retention` | duration | `10y` | Monthly summary retention |
| `--yearly-retention` | duration | `0` | Yearly summary retention (`0` = indefinite) |
| `--log-level` | string | `info` | `error`, `warn`, `info`, `debug`, `trace` |
| `--version` | flag | — | Print version and exit |
| `--help` | flag | — | Print help and exit |

### 3.3 Duration Parsing

Durations accept a number followed by a single-character unit:

| Unit | Meaning | Example |
|---|---|---|
| `s` | seconds | `30s` |
| `m` | minutes | `5m` |
| `h` | hours | `1h` |
| `d` | days | `30d` |
| `w` | weeks | `2w` (retention only) |
| `y` | years | `5y` (retention only) |

The collection interval must be at least 1 second.

### 3.4 Platform Defaults

| Platform | Data directory |
|---|---|
| Linux | `~/.local/share/thewatcher` |
| Windows | `%LOCALAPPDATA%\TheWatcher` |

The data directory is created if it does not exist. TheWatcher never creates
files outside this directory except for normal OS-managed logs.

---

## 4. Architecture

### 4.1 Component Diagram

```
                 ┌──────────────────────┐
                 │   Platform collectors  │
                 │   (cpu, mem, disk,     │
                 │    net, uptime)        │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ In-memory current     │
                 │ snapshot              │
                 │ (RwLock<Option<…>>)   │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ Granular MooFile      │
                 │ writer                │
                 │ (append-only insert)  │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ Background rollups    │
                 │ and retention worker  │
                 │ (tokio::select loop)  │
                 └──────────┬───────────┘
                            │
          ┌─────────────────┴─────────────────┐
          ▼                                   ▼
 ┌──────────────────────┐             ┌──────────────────────┐
 │ HTTP JSON API         │             │ Embedded web UI       │
 │ (Axum, /api/*)       │             │ (rust-embed, vanilla  │
 │ current + history     │             │  JS, SVG charts)      │
 └──────────────────────┘             └──────────────────────┘
```

### 4.2 Concurrency Model

- **Collection loop** — A single `tokio::spawn` task that sleeps for the
  configured interval, then atomically refreshes sysinfo, computes deltas, and
  updates the `Arc<RwLock<Option<CurrentSnapshot>>>`.
- **Maintenance loop** — A single `tokio::spawn` task using `tokio::select!`
  over two intervals: rollup (5 minutes) and retention (1 hour).
- **HTTP server** — Axum with shared `Arc<AppState>`. All read endpoints
  acquire the `RwLock` for reads. Writes (from the collection loop) acquire it
  for writes.
- **MooFile** — All collections are `Send + Sync`. Reads and writes from
  different tasks are safe; MooFile uses internal locking for multi-thread
  access.

### 4.3 Startup Sequence

1. Parse CLI arguments → `Config`.
2. Initialize `tracing-subscriber` with the configured log level.
3. Print startup banner: version, platform, data directory, collection
   interval, retention policy.
4. Open MooFile storage (creates files and indexes if needed).
5. Spawn collection task (runs first collection immediately, then at interval).
6. Spawn maintenance task (first rollup after 60s, then every 5m/1h).
7. Bind TCP listener and start Axum server.
8. On shutdown, abort background tasks.

---

## 5. Metric Collection

### 5.1 Collector Framework

All collectors use the [sysinfo](https://crates.io/crates/sysinfo) crate
(version 0.34). A `CollectorContext` holds a `sysinfo::System` instance and
previous counter snapshots for delta calculations:

```rust
pub struct CollectorContext {
    pub system: System,
    pub prev_network: Vec<(String, u64, u64, u64, u64)>,
    pub prev_cpu: Vec<(String, f64, f64, f64)>,
    pub last_collection: Option<Instant>,
}
```

The `collect_all` function refreshes the system, calls each collector, and
assembles a `CurrentSnapshot`:

```rust
pub fn collect_all(ctx: &mut CollectorContext) -> CurrentSnapshot
```

Each collector returns its data and appends a `CollectorStatus` entry. A
collector failure must not prevent other collectors from running.

### 5.2 CPU

**Data sources:** `sysinfo::System::cpus()`, `System::load_average()`

- **Total utilization (%)** — Average of per-core `cpu_usage()` values from
  sysinfo. This is a delta-based metric computed by sysinfo internally between
  refreshes.
- **Per-core utilization** — Individual `cpu_usage()` values for each logical
  CPU.
- **Load average** — `System::load_average()` returning 1/5/15 minute values
  where supported (Linux, macOS). Unavailable on Windows (returns 0.0).
- **Logical CPU count** — `cpus().len()`.

The first collection after startup may report `null` for percent and per-core
values because sysinfo needs one refresh cycle to establish deltas. Subsequent
collections return valid percentages.

### 5.3 Memory

**Data sources:** `sysinfo::System` memory methods

| Field | Source | Unit |
|---|---|---|
| `total_bytes` | `System::total_memory()` | bytes |
| `used_bytes` | `System::used_memory()` | bytes |
| `available_bytes` | `System::available_memory()` | bytes |
| `used_percent` | computed: `used / total * 100` | percent |
| `swap_total_bytes` | `System::total_swap()` | bytes |
| `swap_used_bytes` | `System::used_swap()` | bytes |
| `swap_used_percent` | computed: `swap_used / swap_total * 100` | percent |

Available memory and swap information may be `null` on platforms where they
are unsupported.

### 5.4 Disk / Filesystems

**Data sources:** `sysinfo::Disks::new_with_refreshed_list()`, Linux fallback
via `libc::statvfs`

For each discovered disk:

| Field | Source |
|---|---|
| `mount` | `Disk::mount_point()` |
| `filesystem` | `Disk::name()` (device name) |
| `total_bytes` | `Disk::total_space()` |
| `available_bytes` | `Disk::available_space()` |
| `used_bytes` | `total - available` |
| `used_percent` | `used / total * 100` |

**Pseudo-filesystem filtering:** Mounts with prefixes `/sys`, `/proc`, `/dev`,
`/run`, `/snap` are excluded. On Linux, a `/proc/mounts` fallback further
filters out `tmpfs`, `devtmpfs`, `squashfs`, and non-device mounts.

### 5.5 Network Interfaces

**Data sources:** `sysinfo::Networks::new_with_refreshed_list()`

For each interface:

| Field | Source |
|---|---|
| `interface` | Network name |
| `rx_bytes_total` | `NetworkData::received()` |
| `tx_bytes_total` | `NetworkData::transmitted()` |
| `rx_packets_total` | `NetworkData::packets_received()` |
| `tx_packets_total` | `NetworkData::packets_transmitted()` |
| `rx_bytes_per_sec` | Delta from previous `rx_bytes_total` ÷ elapsed seconds |
| `tx_bytes_per_sec` | Delta from previous `tx_bytes_total` ÷ elapsed seconds |
| `operational` | `true` if `rx_bytes_total > 0 \|\| tx_bytes_total > 0` |

**Rate calculation:** The collector stores previous cumulative counters in
`prev_network`. On each collection, the difference between current and previous
counters is divided by elapsed time. Counter resets (current < previous) are
detected and the delta is computed from zero rather than producing a negative
rate. The first collection always reports `null` rates.

**Dynamic interface discovery:** Interfaces are discovered on every collection
cycle. Added or removed interfaces are handled naturally — new interfaces
appear with `null` rates on first sight; removed interfaces simply stop
appearing.

### 5.6 Host & Uptime

| Field | Source |
|---|---|
| `hostname` | `System::host_name()` |
| `uptime_seconds` | `System::uptime()` |
| `boot_time_ms` | `now_ms - uptime_seconds * 1000` |

---

## 6. Data Model

### 6.1 Timestamps

All persisted timestamps use **UTC epoch milliseconds** as a signed 64-bit
integer (`i64`). Epoch milliseconds avoid timezone ambiguity and simplify
cross-platform serialization.

Every document has a deterministic string `_id` as required by MooFile.

### 6.2 Granular Sample Document

Granular samples are collected at the configured interval. Each metric has its
own document shape.

#### CPU

```json
{
  "_id": "cpu-1786468200000",
  "timestamp_ms": 1786468200000,
  "metric": "cpu",
  "resolution": "granular",
  "cpu_percent": 34.2,
  "load_1": 1.42,
  "load_5": 1.11,
  "load_15": 0.98,
  "logical_cpus": 8
}
```

#### Memory

```json
{
  "_id": "memory-1786468200000",
  "timestamp_ms": 1786468200000,
  "metric": "memory",
  "resolution": "granular",
  "total_bytes": 17179869184,
  "used_bytes": 8589934592,
  "available_bytes": 8589934592,
  "used_percent": 50.0,
  "swap_total_bytes": 4294967296,
  "swap_used_bytes": 0
}
```

#### Disk

```json
{
  "_id": "disk-root-1786468200000",
  "timestamp_ms": 1786468200000,
  "metric": "disk",
  "resolution": "granular",
  "mount": "/",
  "filesystem": "/dev/sda1",
  "total_bytes": 500000000000,
  "used_bytes": 250000000000,
  "available_bytes": 250000000000,
  "used_percent": 50.0
}
```

The `_id` for disk documents uses the mount path with `/` replaced by `-`
(e.g. `disk--1786468200000` for root, `disk--home-1786468200000` for `/home`).

#### Network

```json
{
  "_id": "network-en0-1786468200000",
  "timestamp_ms": 1786468200000,
  "metric": "network",
  "resolution": "granular",
  "interface": "en0",
  "rx_bytes_total": 123456789,
  "tx_bytes_total": 987654321,
  "rx_bytes_per_sec": 18234.0,
  "tx_bytes_per_sec": 9412.0,
  "rx_packets_total": 1000,
  "tx_packets_total": 900,
  "operational": true
}
```

### 6.3 Rollup Document

Rollups summarize completed time buckets using min/mean/max aggregation.
Deterministic `_id` values make retries idempotent:

```
_id = "{metric}-{resolution}-{bucket_start_ms}"
```

Example: `cpu-hourly-1786464000000`

```json
{
  "_id": "cpu-hourly-1786464000000",
  "bucket_start_ms": 1786464000000,
  "bucket_end_ms": 1786467600000,
  "timestamp_ms": 1786464000000,
  "metric": "cpu",
  "resolution": "hourly",
  "sample_count": 120,
  "cpu_min": 2.1,
  "cpu_mean": 31.4,
  "cpu_max": 88.7,
  "load_1_min": 0.2,
  "load_1_mean": 1.3,
  "load_1_max": 4.8,
  "mem_used_min": null,
  "mem_used_mean": null,
  "mem_used_max": null
}
```

Fields not relevant to the metric (e.g. `mem_used_*` in a CPU rollup) are
`null`. The `sample_count` records how many source documents were aggregated.

### 6.4 Resolution Hierarchy

```
granular (raw, ~30s) ──► hourly (1h buckets)
hourly                ──► daily (1d buckets)
daily                 ──► monthly (30d buckets)
monthly               ──► yearly (365d buckets)
```

Rollups always summarize the next-lower resolution, not the raw granular data.
This means an hourly rollup reads from granular documents, a daily rollup reads
from hourly documents, and so on.

### 6.5 History API Response

```json
{
  "metric": "cpu",
  "resolution": "hourly",
  "from_ms": 1786381800000,
  "until_ms": 1786468200000,
  "series": [
    {
      "name": "cpu_percent",
      "unit": "percent",
      "points": [
        {"timestamp_ms": 1786381800000, "min": 12.1, "mean": 25.4, "max": 61.2},
        {"timestamp_ms": 1786385400000, "min": 8.3, "mean": 22.1, "max": 55.0}
      ]
    },
    {
      "name": "load_1",
      "unit": "load",
      "points": [
        {"timestamp_ms": 1786381800000, "min": 0.5, "mean": 1.3, "max": 4.8}
      ]
    }
  ]
}
```

Points in rollup series use `min`/`mean`/`max`. Points in granular series use
`value`. Empty series are possible if no data exists in the requested range.

---

## 7. MooFile Storage

### 7.1 Collection Layout

Five separate MooFile collections, one per resolution:

```
<data-dir>/
├── granular.bson
├── hourly.bson
├── daily.bson
├── monthly.bson
└── yearly.bson
```

MooFile also creates `.bson.cache` (disposable index cache) and `.bson.lock`
(process lock) files alongside each BSON file.

### 7.2 Indexes

Each collection is opened with indexes on:

- `timestamp_ms` — for time-range queries
- `metric` — for metric-type filtering
- `interface` — for network interface filtering
- `mount` — for disk mount filtering

Indexes are rebuilt from the cache file on cold start (or from a full BSON
scan if the cache is missing). They are preserved across restarts.

### 7.3 Write Model

- **Granular writes:** Each collection cycle inserts one document per metric
  (plus one per disk mount and one per network interface). Documents are
  immutable — they are never updated in-place.
- **Duplicate handling:** All insert operations are idempotent. If a document
  with the same `_id` already exists, the error is silently ignored. This
  handles edge cases like clock adjustment or process restart at the same
  second boundary.
- **Batch writes:** Not yet implemented (individual inserts per document).
  Future optimization.

### 7.4 Rollup Idempotency

Rollup documents use deterministic IDs (`{metric}-{resolution}-{bucket_start}`).
The rollup worker checks the last rollup timestamp for each metric/resolution
pair to determine where to resume. If a rollup is retried (e.g. after a crash),
the same `_id` is computed and the insert is silently ignored.

### 7.5 Retention Algorithm

1. For each resolution with a non-zero retention days value, compute
   `cutoff_ms = now_ms - retention_days * 86_400_000`.
2. Delete all documents where `timestamp_ms < cutoff_ms`.
3. After deletion, check each collection's `dead_ratio` (fraction of dead
   records from deletions). If it exceeds 30%, trigger `compact()`.

Retention runs every hour. Compaction rewrites the BSON file without dead
records.

### 7.6 Multi-Process Behavior

TheWatcher is expected to have one process writing and serving reads. MooFile's
multi-process support is still useful for operational tools, backups, or future
external readers. The application never exposes MooFile files directly through
HTTP.

---

## 8. Collection & Rollup Scheduling

### 8.1 Collection Loop

```
┌─ Initial collection (immediate) ─┐
│  refresh_all()                   │
│  collect_all() → snapshot        │
│  persist_granular(snapshot)      │
│  update in-memory snapshot        │
└──────────────────────────────────┘
         │
         ▼
┌─ Interval loop ──────────────────┐
│  sleep(interval)                 │
│  refresh_all()                   │
│  collect_all() → snapshot        │
│  persist_granular(snapshot)      │
│  update in-memory snapshot        │
└──────────────────────────────────┘
         │
         └──► (repeat)
```

The first tick of `tokio::time::interval` is skipped to enforce a full interval
between the initial collection and the first periodic one.

### 8.2 Rollup Scheduling

Rollups run every 5 minutes. The worker:

1. For each metric (`cpu`, `memory`, `disk`, `network`):
   - Queries the last rollup timestamp for the target resolution.
   - Aligns to bucket boundaries.
   - For each completed bucket (bucket end < now):
     - Fetches source documents from the lower resolution.
     - Computes min/mean/max.
     - Writes an idempotent rollup document.
   - Moves to the next bucket.
2. Repeats for each resolution pair: granular→hourly, hourly→daily,
   daily→monthly, monthly→yearly.

A bucket is considered **complete** when `bucket_start + bucket_ms < now_ms`.
A partial bucket is never finalized early.

### 8.3 Resolution Auto-Selection

When the history API is called without an explicit `resolution` parameter,
the resolution is chosen based on the time range duration:

| Duration | Resolution |
|---|---|
| 0 – 3,600,000 ms (≤ 1 hour) | `granular` |
| 3,600,001 – 86,400,000 ms (≤ 1 day) | `hourly` |
| 86,400,001 – 2,592,000,000 ms (≤ 30 days) | `daily` |
| 2,592,000,001 – 31,536,000,000 ms (≤ 365 days) | `monthly` |
| > 31,536,000,000 ms (> 365 days) | `yearly` |

### 8.4 Result Limiting

History queries return at most 10,000 points. If more data is available, the
result set is uniformly downsampled using step-by iteration. This prevents
unbounded response sizes.

---

## 9. HTTP API Reference

### 9.1 Server Configuration

- **Framework:** Axum 0.8 (async Rust HTTP framework)
- **Middleware:**
  - `RequestBodyLimitLayer` — 1 MB maximum request body
  - `TimeoutLayer` — 30-second request timeout (returns 504 Gateway Timeout)
- **Asset serving:** `rust-embed` compiles `src/web/` into the binary at build
  time. No filesystem serving — `GET /static/*` routes serve from the embedded
  asset map.

### 9.2 Route Table

| Method | Path | Handler | Description |
|---|---|---|---|
| `GET` | `/` | `dashboard_handler` | Dashboard HTML |
| `GET` | `/static/{*path}` | `static_handler` | Embedded CSS/JS assets |
| `GET` | `/api/current` | `api_current` | Current snapshot |
| `GET` | `/api/history` | `api_history` | Historical data |
| `GET` | `/api/health` | `api_health` | Health status |
| `GET` | `/api/info` | `api_info` | Host & app info |

### 9.3 `GET /api/current`

Returns the most recent in-memory `CurrentSnapshot`. If no collection has
occurred yet (snapshot is `None`), returns HTTP 503.

### 9.4 `GET /api/history`

**Query parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `metric` | string | **yes** | `cpu`, `memory`, `disk`, `network` |
| `range` | string | no¹ | e.g. `1h`, `24h`, `7d`, `30d`, `1y` |
| `from` | i64 | no¹ | Start epoch milliseconds |
| `until` | i64 | no¹ | End epoch milliseconds |
| `resolution` | string | no | `granular`, `hourly`, `daily`, `monthly`, `yearly`, or `auto` |
| `interface` | string | no | Network interface name filter |
| `mount` | string | no | Disk mount path filter |

¹ Either `range` or `from`/`until` must be provided. If neither is given, a
default range of 1 hour is used.

**Validation rules:**

- Unknown metrics → HTTP 400 with message listing valid metrics.
- Invalid range format → HTTP 400.
- `from ≥ until` → HTTP 400.
- Invalid resolution → HTTP 400.
- `interface` parameter is only meaningful with `metric=network` but is
  accepted (and ignored) for other metrics.
- `mount` parameter is only meaningful with `metric=disk`.

**Response:** See §6.5 History API Response.

### 9.5 `GET /api/health`

| Condition | HTTP Status | `status` field |
|---|---|---|
| All systems operational | 200 | `"ok"` |
| Storage degraded but serving | 200 | `"degraded"` |
| Storage completely unavailable | 503 | — |

The health endpoint is designed for simple monitoring: `curl -s
http://localhost:8080/api/health | jq .status` should return `"ok"`.

### 9.6 `GET /api/info`

Returns static host and application metadata. Does not expose environment
variables, user lists, process command lines, or file contents.

### 9.7 HTTP Hardening

- **Request timeouts:** Read/write/idle timeouts via Tower layers (30s).
- **Body size limits:** 1 MB via `RequestBodyLimitLayer`.
- **Parameter validation:** All query parameters are validated before use.
- **Error responses:** Client-facing errors are generic. Diagnostic details
  are logged server-side only.
- **Content-Type:** Set correctly for all responses:
  - `application/json` for API endpoints
  - `text/html; charset=utf-8` for the dashboard
  - `text/css; charset=utf-8` for stylesheets
  - `application/javascript; charset=utf-8` for scripts
- **No path traversal:** Static asset serving uses an exact match against
  embedded assets — it never touches the filesystem.

---

## 10. Web Dashboard

### 10.1 Technology

- **HTML:** Single page with semantic structure.
- **CSS:** Custom properties (variables) for light/dark/system theme switching.
  No CSS framework.
- **JavaScript:** Vanilla JS (no React, no npm, no build step). ~400 lines.
- **Charts:** Hand-drawn SVG elements — line paths, area fills, grid lines,
  axis labels. No charting library.
- **Assets:** Compiled into the binary via `rust-embed`. No CDN, no external
  resources.

### 10.2 Layout

```
┌─────────────────────────────────────────────┐
│ TheWatcher              hostname  last  🌓  │  ← top bar
├─────────────────────────────────────────────┤
│  ┌──────┐  ┌──────┐  ┌──────────────────┐  │
│  │ CPU  │  │ MEM  │  │ System Info       │  │  ← gauge row
│  │ 34%  │  │ 50%  │  │ Host: server01    │  │
│  │Load: │  │8G/16G│  │ OS: linux         │  │
│  │1.4/… │  │      │  │ Uptime: 1d 2h 3m  │  │
│  └──────┘  └──────┘  │ CPUs: 8 logical   │  │
│                      └──────────────────┘  │
├─────────────────────────────────────────────┤
│ Disks                                       │
│  ┌──────┐  ┌──────┐                         │
│  │ /    │  │ /home│                         │
│  │ ████ │  │ ██   │                         │
│  └──────┘  └──────┘                         │
├─────────────────────────────────────────────┤
│ Network                                     │
│  ┌──────┐  ┌──────┐                         │
│  │ eth0 │  │wlan0 │                         │
│  │↓1.2M │  │↓45K  │                         │
│  └──────┘  └──────┘                         │
├─────────────────────────────────────────────┤
│ CPU History               [Last 24h ▾]     │
│  ┌──────────────────────────────────────┐   │
│  │  ╱╲    ╱╲                            │   │  ← SVG chart
│  │ ╱  ╲──╱  ╲───╱╲                     │   │
│  │          ╲──╱    ╲                   │   │
│  └──────────────────────────────────────┘   │
├─────────────────────────────────────────────┤
│ Memory History             [Last 24h ▾]    │
│  ┌──────────────────────────────────────┐   │
│  │  ───────╮    ╭──────                 │   │
│  │         ╰────╯                       │   │
│  └──────────────────────────────────────┘   │
├─────────────────────────────────────────────┤
│ Network History            [Last 24h ▾]    │
│  ┌──────────────────────────────────────┐   │
│  │  ↓ rx_bytes  ↑ tx_bytes              │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

### 10.3 Real-Time Updates

The dashboard polls `GET /api/current` every 5 seconds for gauge updates and
`GET /api/history` every 60 seconds for chart updates. The polling intervals
are independent of the storage collection interval.

### 10.4 Themes

Three themes are supported:

- **Light** — High contrast, white background.
- **Dark** — Low light, dark background.
- **System** — Follows `prefers-color-scheme` media query.

A theme switcher button (🌓) cycles through the three options. The choice is
stored in a cookie (`thewatcher_theme`) with `Path=/; SameSite=Lax` and a
1-year expiry. The theme is applied via a `data-theme` attribute on `<html>`,
which CSS custom properties target.

### 10.5 Chart Interaction

- **Tooltips:** Hovering over a chart shows the nearest data point's value
  and timestamp.
- **Range selector:** Each chart has a `<select>` dropdown for time range
  (1h, 24h, 7d, 30d).
- **Responsive:** Charts redraw on window resize.

---

## 11. Error Handling & Degraded Operation

### 11.1 Collector Isolation

Each collector is called independently within `collect_all`. If one collector
panics or returns an error, it does not affect other collectors. The framework
catches errors and records them as collector status entries.

### 11.2 Collector Status

The `collector_status` array in `/api/current` tracks per-collector health:

```json
{
  "component": "network",
  "status": "unavailable",
  "message": "interface counters could not be read",
  "last_success_ms": 1786467900000
}
```

Possible status values: `"ok"`, `"degraded"`, `"unavailable"`.

### 11.3 Storage Failures

- **Startup failure:** If required storage cannot be opened at startup
  (e.g. permission denied, disk full), TheWatcher exits with a clear error
  message and non-zero exit code.
- **Runtime failure:** A failed write is logged at `error` level. The current
  dashboard remains available. History queries that touch the affected
  collection return empty results.
- **Retry:** The collection loop does not retry failed writes with backoff
  (it simply tries again on the next interval). The maintenance loop retries
  rollups and retention on their next scheduled tick.

### 11.4 HTTP Error Responses

| Condition | HTTP Status | Body |
|---|---|---|
| Unknown metric | 400 | `"Unknown metric: foo. Valid: […]"` |
| Invalid range | 400 | `"Invalid range: …"` |
| from ≥ until | 400 | `"from must be before until"` |
| Invalid resolution | 400 | `"Invalid resolution: …"` |
| No data collected yet | 503 | `"No metrics collected yet"` |
| Storage unavailable | 503 | `"Storage unavailable: …"` |
| Request timeout | 504 | (empty, from TimeoutLayer) |
| Unknown route | 404 | (Axum default) |

---

## 12. Cross-Platform Design

### 12.1 Abstraction Strategy

Platform-specific code is confined to the `collectors/` module. Each collector
uses `sysinfo` for the primary data path, which handles the platform
abstraction. Linux-specific fallback code (e.g. `/proc/mounts` parsing via
`libc::statvfs`) is behind `#[cfg(target_os = "linux")]` gates.

Storage, HTTP, and model code are completely platform-agnostic.

### 12.2 Path Handling

The `--data-dir` option accepts native paths. Defaults use platform conventions:

| Platform | Default |
|---|---|
| Linux | `$HOME/.local/share/thewatcher` |
| Windows | `%LOCALAPPDATA%\TheWatcher` |

All path construction uses `std::path::PathBuf` — no hardcoded separators.

### 12.3 Linux-Specific Considerations

- Disk discovery falls back to `/proc/mounts` with `libc::statvfs` if sysinfo
  returns no disks.
- Load average is available via `System::load_average()`.
- Interface names follow Linux conventions (e.g. `eth0`, `enp3s0`, `wlp1s0`)
  but the code makes no assumptions about naming.

### 12.4 Windows-Specific Considerations

- `sysinfo` provides CPU, memory, disk, and network data through Windows APIs.
- Load average is not available on Windows (returns 0.0).
- Interface names use Windows conventions (GUID-based or friendly names
  depending on the sysinfo version).
- Path separators use `\`; `LOCALAPPDATA` is used for the default data
  directory.

---

## 13. Build & Distribution

### 13.1 Build Commands

```bash
# Debug build
cargo build

# Release build (optimized, LTO)
cargo build --release

# Run tests
cargo test

# Cross-compile for Linux ARM64
cargo build --release --target aarch64-unknown-linux-gnu

# Cross-compile for Windows
cargo build --release --target x86_64-pc-windows-gnu
```

### 13.2 Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `axum` | 0.8 | HTTP server and routing |
| `tokio` | 1 | Async runtime |
| `tower-http` | 0.6 | Middleware (timeout, body limit) |
| `tower` | 0.5 | Service trait |
| `clap` | 4 | CLI argument parsing |
| `serde` / `serde_json` | 1 | JSON/BSON serialization |
| `sysinfo` | 0.34 | System metrics |
| `chrono` | 0.4 | UTC timestamps |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | Structured logging |
| `rust-embed` | 8 | Compile-time asset embedding |
| `moofile-core` | — | Embedded document store |
| `bson` | 2 | BSON document type |
| `libc` | 0.2 | Linux statvfs (Linux only) |
| `reqwest` | 0.12 | (dev only) HTTP client for tests |
| `tempfile` | 3 | (dev only) Temporary directories for tests |

### 13.3 Release Artifacts

```
thewatcher-linux-amd64
thewatcher-linux-arm64
thewatcher-windows-amd64.exe
thewatcher-windows-arm64.exe
```

Each binary is self-contained: HTTP server, collectors, MooFile linkage, and
embedded web assets (HTML, CSS, JS) are all compiled in.

### 13.4 Binary Size

| Build | Size | Notes |
|---|---|---|
| Debug | ~106 MB | Unoptimized, debug symbols |
| Release | ~4.5 MB | LTO enabled, opt-level=2 |

---

## 14. Operational Guidance

### 14.1 Local Access (Default)

```bash
thewatcher
# Open http://127.0.0.1:8080/
```

### 14.2 Remote Access over SSH (Recommended)

**Server:**
```bash
thewatcher --listen 127.0.0.1 --port 8080
```

**Workstation:**
```bash
ssh -L 8080:127.0.0.1:8080 admin@server
# Open http://127.0.0.1:8080/
```

### 14.3 Management Network Access

```bash
thewatcher --listen 192.168.10.25 --port 8080
```

Use `iptables`, `nftables`, or Windows Firewall to restrict access to the
management subnet.

### 14.4 All-Interface Access (Not Recommended)

```bash
thewatcher --listen 0.0.0.0 --port 8080
```

A prominent warning is emitted. No TLS or authentication is provided. Use only
on trusted networks behind a firewall.

### 14.5 Custom Data Directory

```bash
thewatcher --data-dir /mnt/metrics/thewatcher
```

Useful for storing data on a dedicated volume or for testing.

### 14.6 Health Check for Monitoring

```bash
curl -sf http://127.0.0.1:8080/api/health | jq -e '.status == "ok"'
```

Exits 0 if healthy, non-zero otherwise. Suitable for cron, systemd health
checks, or external monitoring.

### 14.7 systemd Service Unit

```ini
[Unit]
Description=TheWatcher System Metrics
After=network.target

[Service]
Type=simple
User=nobody
ExecStart=/usr/local/bin/thewatcher
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

---

## Appendix A: Default Retention Policy

| Resolution | Default Retention | Rationale |
|---|---|---|
| Granular | 30 days | Raw 30-second data is voluminous; 30 days gives a month of detail |
| Hourly | 365 days | One year of hourly precision for seasonal comparisons |
| Daily | 5 years | Long-term trend visibility |
| Monthly | 10 years | Capacity planning over a decade |
| Yearly | Indefinite | Yearly summaries are tiny; keep forever |

## Appendix B: Test Coverage

| Suite | Tests | Scope |
|---|---|---|
| Unit tests (lib) | 6 | CLI parsing, rollup math |
| API integration | 14 | All endpoints, error cases, static assets |
| CLI & binding | 20 | Config, address detection, serialization, resolution logic |
| Storage | 17 | CRUD, rollup, retention, idempotency, persistence across restart |
| **Total** | **57** | |

Tests use `tempfile` for isolation — no test touches the real filesystem or
running system state.
