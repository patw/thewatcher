# Changelog

All notable changes to TheWatcher will be documented in this file.

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

[0.1.2]: https://github.com/beholder/thewatcher/releases/tag/v0.1.2

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

[0.1.0]: https://github.com/beholder/thewatcher/releases/tag/v0.1.0
