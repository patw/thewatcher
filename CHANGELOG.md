# Changelog

All notable changes to TheWatcher will be documented in this file.

## [0.1.0] — 2026-08-11

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
