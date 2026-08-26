# Changelog

All notable changes to TheWatcher will be documented in this file.

## [0.1.4] — 2026-08-25

### Fixed

- **7d / 30d (and 1y) history empty** — `compute_rollup` read *granular* field
  names (`cpu_percent`, `used_percent`, `rx_bytes_per_sec`, …) from its source
  documents even when the source was itself a rollup. The hourly → daily,
  daily → monthly, and monthly → yearly steps therefore aggregated the wrong
  fields and wrote documents with `sample_count > 0` but every min/mean/max
  value null. Since the 7d/30d/1y ranges resolve to the `daily`/`monthly`
  collections, those ranges always returned empty while 1h (granular) and 24h
  (hourly) worked. Rollup-to-rollup aggregation now merges min-of-mins,
  max-of-maxes, and a `sample_count`-weighted mean-of-means. A one-time cleanup
  removes the stale empty daily/monthly/yearly documents so they are rebuilt
  from hourly on the next maintenance cycle.

## [0.1.3] — 2026-08-17

### Fixed

- **Dashboard chart Y-axis** — percentage charts (CPU, memory, disk) are now
  pinned to a fixed 0–100 range instead of being auto-scaled onto just the
  spread of the data, so small fluctuations no longer get zoomed into a
  misleading baseline. All other metrics (byte rates, process/socket counts,
  load) can no longer produce a negative Y-axis — the lower bound is clamped at
  0 since these values are inherently non-negative.

## [0.1.3] — 2026-08-14

### Fixed

- **CPU hot loop (v0.1.2 regression)** — the sockets rollup read granular
  `tcp_inuse`/`udp_inuse`/`total_sockets` fields (stored as `u32` → BSON int64)
  with `get_i32()`, which always failed. Every sockets rollup was written
  without those values, so the new stale-doc cleanup deleted each rollup as
  soon as it was written, `last_timestamp()` never found a resume point, and
  the rollup loop restarted from **Unix epoch** on every maintenance cycle —
  roughly 500k per-bucket scans of the granular collection, pegging a CPU core
  at 100% indefinitely. Rollups now read numeric fields regardless of BSON
  int32/int64/double representation.
- **Defensive bound on rollup resume** — if a metric ever lacks a resume point,
  rollups now scan back at most the granular retention window (30 days)
  instead of epoch, so this class of failure can no longer pin a core.
- **Disk rollups accidentally dropped** — 0.1.2's rewrite removed the disk
  match arm, so disk hourly+ summaries silently stopped being written. The
  disk arm is restored (used_percent rolled into the `mem_used_*` fields the
  history API already reads).
- Stale-doc cleanup now only ever matches documents written by the buggy
  versions; fresh rollups always carry the fields the cleanup checks for.

### Added

- Regression tests for sockets/network/disk rollup aggregation, the stale-doc
  cleanup, and the resume-point invariant that protects against the hot loop.

[0.1.3]: https://github.com/patw/thewatcher/releases/tag/v0.1.3

## [0.1.2] — 2026-08-13

### Fixed

- **Network & sockets 24-hour history** — The rollup system only aggregated CPU
  and memory metrics; network and sockets fell through an empty match arm, so
  hourly+ rollup summaries were written with no meaningful data. Now properly
  aggregates per-interface network rates (rx/tx bytes/sec) and sockets metrics
  (process count, TCP/UDP inuse, total sockets) across all rollup resolutions.
- Includes a one-time cleanup of stale empty rollup documents left by the bug.

### Changed

- `RollupDoc` extended with 30 new fields for network and sockets rollup data.
- Rollup query API now supports optional `interface` and `mount` filters.

[0.1.2]: https://github.com/patw/thewatcher/releases/tag/v0.1.2

### Initial Release

TheWatcher is a self-hosted, single-binary system metrics viewer for sysadmins
who want local, durable machine visibility without cloud-based metrics services.

**Features:**

- Collects CPU, memory, disk, network, uptime, and host information.
- Persists historical observations locally using MooFile.
- Performs RRD-style rollups from granular observations into hourly, daily,
  monthly, and yearly summaries.
- Serves a browser-based dashboard with gauges, cards, and interactive line
  charts.
- Exposes a read-only JSON API for programmatic access.
- Runs as one executable with embedded web assets — no Node.js, Python, or
  database server required.
- Loopback-only binding by default with clear security warnings for broad
  binding.
- Light, dark, and system-following themes with cookie persistence.
- Cross-platform: Linux x86_64/aarch64, Windows x86_64/aarch64.
- Licensed under MIT.

[0.1.0]: https://github.com/patw/thewatcher/releases/tag/v0.1.0
