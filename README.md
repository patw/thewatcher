# TheWatcher

**Self-hosted, single-binary system metrics viewer.**

[![CI](https://github.com/patw/thewatcher/actions/workflows/ci.yml/badge.svg)](https://github.com/patw/thewatcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80+-orange.svg)](https://www.rust-lang.org)

TheWatcher collects CPU, memory, disk, and network metrics, persists them
locally with RRD-style rollups, and serves a browser dashboard — all from one
static binary. No cloud account, no external database, no telemetry.

<p align="center">
  <em><!-- TODO: screenshot of dashboard --></em>
  <br>
  <sub><strong>📸 Screenshot placeholder — coming soon</strong></sub>
</p>

---

## Why?

You want to see what your server is doing **right now** and **over time**
without shipping metrics to a SaaS platform or standing up a full monitoring
stack. TheWatcher gives you:

- One binary to copy and run.
- A browser dashboard with gauges and line charts.
- A read-only JSON API for scripting.
- Durable local history with predictable retention.
- **No** cloud, **no** telemetry, **no** accounts.

## Quick start

```bash
# Build (see "Building" below for the moofile dependency)
cargo build --release

# Run with defaults — listens on 127.0.0.1:8080
./target/release/thewatcher

# Open the dashboard
open http://127.0.0.1:8080
```

See **[DEPLOYING.md](DEPLOYING.md)** for running as a service on Linux (systemd,
OpenRC, runit), macOS (launchd), and Windows.

### Remote access over SSH (recommended)

```bash
# On the server:
thewatcher --listen 127.0.0.1 --port 8080

# On your workstation:
ssh -L 8080:127.0.0.1:8080 admin@server
# Then open http://127.0.0.1:8080
```

### Direct management-network access

```bash
thewatcher --listen 192.168.10.25 --port 8080
```

Use your host firewall to restrict access to the management network.
TheWatcher does **not** provide TLS or authentication in v0.1.

## Command-line options

```
thewatcher [OPTIONS]

Options:
  --listen ADDRESS          Bind address; default: 127.0.0.1
  --port PORT               HTTP port; default: 8080
  --interval DURATION       Collection interval (e.g. 5s, 30s, 1m); default: 30s
  --data-dir PATH           Directory for MooFiles; platform default
  --granular-retention DUR  Retain granular samples; default: 30d
  --hourly-retention DUR    Retain hourly summaries; default: 365d
  --daily-retention DUR     Retain daily summaries; default: 5y
  --monthly-retention DUR   Retain monthly summaries; default: 10y
  --yearly-retention DUR    Retain yearly summaries; default: 0 (indefinite)
  --log-level LEVEL         error, warn, info, debug, trace; default: info
  --version                 Print version and exit
  --help                    Print help and exit
```

### Defaults

| Setting | Value |
|---|---|
| Listen address | `127.0.0.1` |
| Port | `8080` |
| Collection interval | `30s` |
| Data directory (Linux) | `~/.local/share/thewatcher` |
| Data directory (Windows) | `%LOCALAPPDATA%\TheWatcher` |

## Security

TheWatcher is **safe by default**:

- Binds to `127.0.0.1` (loopback only) unless you explicitly override it.
- Prints a prominent warning when `--listen 0.0.0.0` is used.
- All API endpoints are read-only — no configuration changes, no command
  execution, no file browsing.
- Runs as an unprivileged user; degrades gracefully when a metric is
  unavailable instead of requiring root.

TheWatcher v0.1 does **not** provide TLS or authentication. For encrypted
remote access use an SSH tunnel, VPN, or reverse proxy.

## HTTP API

All endpoints are read-only. Responses are JSON.

### `GET /api/health`

Basic application health.

```bash
curl -s http://127.0.0.1:8080/api/health | jq
```

```json
{
  "status": "ok",
  "version": "0.1.0",
  "last_collection_ms": 1786468200000,
  "last_rollup_ms": 1786467600000,
  "storage": "ok"
}
```

Returns `200` when healthy or degraded-but-serving. Returns `503` only when
storage is completely unavailable.

### `GET /api/info`

Host and application information.

```bash
curl -s http://127.0.0.1:8080/api/info | jq
```

```json
{
  "version": "0.1.0",
  "hostname": "server01",
  "os": "linux",
  "arch": "x86_64",
  "start_time_ms": 1786460000000,
  "boot_time_ms": 1786200000000,
  "data_dir": "/home/user/.local/share/thewatcher",
  "listen_addr": "127.0.0.1:8080"
}
```

### `GET /api/current`

In-memory snapshot of all current metrics.

```bash
curl -s http://127.0.0.1:8080/api/current | jq
```

```json
{
  "timestamp_ms": 1786468200000,
  "hostname": "server01",
  "uptime_seconds": 86400,
  "cpu": {
    "percent": 34.2,
    "per_core": [30.1, 38.4, 35.2, 33.1],
    "load_1": 1.42,
    "load_5": 1.11,
    "load_15": 0.98,
    "logical_cpus": 4
  },
  "memory": {
    "total_bytes": 17179869184,
    "used_bytes": 8589934592,
    "available_bytes": 8589934592,
    "used_percent": 50.0,
    "swap_total_bytes": 4294967296,
    "swap_used_bytes": 0,
    "swap_used_percent": 0.0
  },
  "disks": [
    {
      "mount": "/",
      "filesystem": "/dev/sda1",
      "total_bytes": 500000000000,
      "used_bytes": 250000000000,
      "available_bytes": 250000000000,
      "used_percent": 50.0
    }
  ],
  "networks": [
    {
      "interface": "eth0",
      "operational": true,
      "rx_bytes_total": 123456789,
      "tx_bytes_total": 987654321,
      "rx_packets_total": 1000,
      "tx_packets_total": 900,
      "rx_bytes_per_sec": 18234.0,
      "tx_bytes_per_sec": 9412.0,
      "rx_errors": null,
      "tx_errors": null,
      "rx_dropped": null,
      "tx_dropped": null
    }
  ],
  "collector_status": [
    {"component": "cpu", "status": "ok", "message": null, "last_success_ms": 1786468200000}
  ]
}
```

Returns `503` if no data has been collected yet. Unavailable values are
`null` — never silently converted to zero. Check `collector_status` for
per-component health.

### `GET /api/history`

Historical metric data with automatic resolution selection.

```bash
# Last hour of CPU at granular resolution
curl -s "http://127.0.0.1:8080/api/history?metric=cpu&range=1h" | jq

# Last 30 days of memory
curl -s "http://127.0.0.1:8080/api/history?metric=memory&range=30d" | jq

# Network for a specific interface, last 7 days
curl -s "http://127.0.0.1:8080/api/history?metric=network&interface=eth0&range=7d" | jq

# Disk for root filesystem, explicit resolution
curl -s "http://127.0.0.1:8080/api/history?metric=disk&mount=%2F&range=30d&resolution=daily" | jq

# Custom time range with epoch milliseconds
curl -s "http://127.0.0.1:8080/api/history?metric=cpu&from=1700000000000&until=1700086400000" | jq
```

**Query parameters:**

| Parameter | Description |
|---|---|
| `metric` | `cpu`, `memory`, `disk`, `network`, `sockets` |
| `range` | `1h`, `24h`, `7d`, `30d`, `1y` (mutually exclusive with `from`/`until`) |
| `from` | Start epoch milliseconds |
| `until` | End epoch milliseconds |
| `resolution` | `granular`, `hourly`, `daily`, `monthly`, `yearly`, or `auto` (default) |
| `interface` | Network interface filter (for `network` metric) |
| `mount` | Disk mount filter (for `disk` metric) |

**Response:**

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

- Results are limited to ~10,000 points maximum.
- Resolution is auto-selected based on the time range: ≤1h→granular,
  ≤1d→hourly, ≤30d→daily, ≤365d→monthly, \>365d→yearly.
- Missing data is represented as gaps — never zero-filled.
- For rollup resolutions, each point includes `min`/`mean`/`max`. For granular,
  each point has `value`.

### `GET /` — Dashboard

The browser dashboard. No CDN, no external assets — everything is embedded in
the binary.

### `GET /static/*` — Static assets

Embedded CSS and JavaScript. Served with `Cache-Control: public, max-age=3600`.

## Architecture

```
                  ┌──────────────────────┐
                  │   Platform collectors  │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ In-memory current     │
                  │ snapshot              │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ Granular MooFile      │
                  │ writer                │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ Background rollups    │
                  │ and retention worker  │
                  └──────────┬───────────┘
                             │
           ┌─────────────────┴─────────────────┐
           ▼                                   ▼
 ┌──────────────────────┐             ┌──────────────────────┐
 │ HTTP JSON API         │             │ Embedded web UI       │
 │ current + history    │             │ gauges + line plots   │
 └──────────────────────┘             └──────────────────────┘
```

### Storage: MooFile

Metrics are stored in five separate [MooFile](https://github.com/patw/moofile)
collections under the data directory:

```
<data-dir>/
├── granular.bson    ← raw 30-second samples
├── hourly.bson      ← 1-hour rollups
├── daily.bson       ← 1-day rollups
├── monthly.bson     ← 30-day rollups
└── yearly.bson      ← 365-day rollups
```

MooFile is an append-only embedded document store (BSON) with in-memory
indexes rebuilt from a disposable cache on cold start. Every document has a
deterministic `_id`, making rollup writes idempotent and restart-safe.

### Collection & rollup

1. **Collect** — Every 30 seconds (configurable), platform collectors gather
   CPU, memory, disk, and network counters. The snapshot updates the in-memory
   state and writes granular documents to `granular.bson`.

2. **Rollup** — Every 5 minutes, a background worker scans for completed
   buckets (e.g. an hour whose end time has passed) and computes min/mean/max
   summaries into the next resolution level. Only completed buckets are
   processed — a partial bucket is never finalized early.

3. **Retention** — Every hour, documents older than their resolution's
   configured retention boundary are deleted. Collections with >30% dead space
   (from deletions) are compacted automatically.

### Resolution auto-selection

| Time range | Resolution | Bucket size |
|---|---|---|
| ≤ 1 hour | granular | — (raw samples) |
| ≤ 1 day | hourly | 1 hour |
| ≤ 30 days | daily | 1 day |
| ≤ 365 days | monthly | 30 days |
| \> 365 days | yearly | 365 days |

## Building

### Prerequisites

- Rust 1.80+
- The [moofile-core](https://github.com/patw/moofile) crate accessible
  locally (path dependency: `../moofile/core`)

```bash
# Clone and build moofile first (if not already available)
git clone git@github.com:patw/moofile.git ../moofile

# Build TheWatcher
cd thewatcher
cargo build --release

# Binary: target/release/thewatcher (~4.5 MB)
```

To use moofile-core from a published crate instead of a local path, change the
dependency in `Cargo.toml` from:

```toml
moofile = { path = "../moofile/core", default-features = false, package = "moofile-core" }
```

to the appropriate registry or git reference.

### Running tests

```bash
cargo test
```

64 tests covering: unit tests, API integration, CLI parsing, storage CRUD,
rollup idempotency, retention, JSON serialization, and configuration.

### Cross-compiling

```bash
# Linux ARM64
cargo build --release --target aarch64-unknown-linux-gnu

# Windows x86_64
cargo build --release --target x86_64-pc-windows-gnu
```

## Project structure

```
src/
├── main.rs              # Entry point, collection/maintenance loops
├── lib.rs               # Module re-exports
├── cli.rs               # CLI argument parsing (clap)
├── config.rs            # Configuration struct + defaults
├── model.rs             # All data types, resolution logic
├── server.rs            # Axum HTTP server + embedded asset serving
├── api.rs               # Route handlers (/api/current, /api/history, …)
├── collectors/
│   ├── mod.rs           # CollectorContext, collect_all dispatch
│   ├── cpu.rs           # CPU utilization + load average
│   ├── memory.rs        # Memory + swap usage
│   ├── disk.rs          # Filesystem discovery + usage
│   ├── network.rs       # Interface counters + rate calculation
│   ├── sockets.rs       # Process count + /proc/net/sockstat (Linux)
│   └── uptime.rs        # Hostname + system uptime
├── storage/
│   ├── mod.rs           # MooFile Storage manager, CRUD, queries
│   ├── rollup.rs        # RRD-style bucket rollup worker
│   └── retention.rs     # Retention policy + compaction
└── web/
    ├── index.html       # Dashboard HTML
    ├── app.js           # Dashboard logic (vanilla JS, no deps)
    └── styles.css       # Light/dark/system theme styles

tests/
├── api_tests.rs         # HTTP API integration tests
├── cli_binding_tests.rs # CLI parsing, binding, serialization tests
└── storage_tests.rs     # MooFile CRUD, rollup, retention tests
```

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 Pat Wendorf.

## Status

**v0.1.0** — Initial release. Feature-complete for local machine monitoring.
See [CHANGELOG.md](CHANGELOG.md) for details.

Future releases may add TLS, authentication, alerting, or multi-machine
aggregation — but the core will always stay a boring, local-first tool that
works without the cloud.
