# 🖥️ Screen View

A lightweight, real-time browser-based dashboard for monitoring live screenshots and process activity on a remote or local machine.

---

## Overview

Screen View provides a sleek dark-mode web interface that streams live screen captures and process telemetry directly in your browser. The backend is a Rust server built on [Tokio](https://tokio.rs/) and [Axum](https://github.com/tokio-rs/axum), keeping the footprint small and performance high — no heavy agents or desktop apps required. It's designed for quick visual oversight of a running system, whether you're monitoring a headless server, a build machine, or a long-running automation task.

---

## Features

- **Live Screenshots** — Displays one or more connected screens, refreshed every 2 seconds via `/screenshot`
- **Process Monitor** — Tracks a root process and its children with real-time CPU, memory, and state indicators
- **Work-Done Detection** — Automatically shows a completion banner when all child processes have exited
- **Responsive Layout** — Side-by-side panels on desktop, stacked on mobile
- **Linux Extended Telemetry** — On Linux, child process snapshots include working directory, command line, open file descriptors, and I/O stats
- **Structured Logging** — Tracing via `tracing-subscriber` with configurable log levels via `RUST_LOG`

---

## How It Works

The Rust backend exposes a small HTTP API (served by Axum) that the frontend polls every 2 seconds:

| Endpoint | Description |
| --- | --- |
| `GET /` | Serves the self-contained `view.html` dashboard |
| `GET /health` | Returns server status, version, and uptime |
| `GET /screenshot` | Returns a JSON array of base64-encoded PNG screen captures |
| `GET /process` | Returns root process info, child processes, CPU/memory stats, and `work_done` flag |

### Expected Response Shapes

**`/screenshot`**

```json
{
  "screens": [
    { "mime": "image/png", "data": "<base64>" }
  ]
}
```

**`/process`**

```json
{
  "work_done": false,
  "root": {
    "name": "my-app",
    "pid": 1234,
    "state": "running",
    "cpu_usage": 12.5,
    "memory_human": "128 MB"
  },
  "child_count": 2,
  "children": [...]
}
```

**`/health`**

```json
{
  "status": "healthy",
  "timestamp": "2025-01-01T00:00:00Z",
  "version": "0.1.0",
  "uptime": "3600s"
}
```

Process `state` can be `running`, `sleeping`, `gone`, or any other string (rendered as a warning-colored pill).

---

## Getting Started

### Prerequisites

- Rust toolchain (stable)
- A display server (X11/Wayland on Linux, or macOS/Windows)

### Running

```bash
cargo run -- --pid <PID>
```

Pass the PID of the root process you want to monitor. The server will start and begin tracking that process and all its children.

By default the API server starts automatically. To run without it:

```bash
cargo run -- --pid <PID> --no-server
```

Then open the dashboard in your browser at the configured address.

### Log Level

Set the `RUST_LOG` environment variable to control verbosity:

```bash
RUST_LOG=debug cargo run -- --pid <PID>
```

---

---

## Roadmap

### 🤖 Telegram Bot Integration *(planned)*

A Telegram bot to allow remote monitoring and alerting without opening the web dashboard. Planned capabilities:

- On-demand screenshot delivery via chat command
- Process status summaries on request
- Alerts when the root process exits or crashes
- Work-done notifications pushed to a configured chat or channel

### 📊 Top Processes Endpoint *(planned)*

A new `/top` endpoint to surface the most resource-intensive processes on the host machine.

| Endpoint | Description |
| --- | --- |
| `GET /top` | Returns a ranked list of processes sorted by CPU or RAM usage |

Planned query parameters:

- `?sort=cpu` — sort by CPU usage (default)
- `?sort=mem` — sort by memory usage
- `?limit=10` — control how many processes are returned

This will also be surfaced in the dashboard as a dedicated "Top Processes" panel alongside the existing process monitor.

---

## License

MIT
