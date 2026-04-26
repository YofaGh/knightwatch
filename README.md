# 🖥️ Knightwatch

A lightweight, real-time browser-based dashboard for monitoring live screenshots and process activity on a remote or local machine.

---

## Overview

Knightwatch provides a sleek dark-mode web interface that streams live screen captures and process telemetry directly in your browser. The backend is a Rust server built on [Tokio](https://tokio.rs/) and [Axum](https://github.com/tokio-rs/axum), keeping the footprint small and performance high — no heavy agents or desktop apps required. It's designed for quick visual oversight of a running system, whether you're monitoring a headless server, a build machine, or a long-running automation task.

---

## Features

- **Live Screenshots** — Displays one or more connected screens, refreshed every 2 seconds via `/screenshot`
- **Process Monitor** — Tracks a root process and its children with real-time CPU, memory, and state indicators
- **Work-Done Detection** — Automatically shows a completion banner when all child processes have exited
- **Responsive Layout** — Side-by-side panels on desktop, stacked on mobile
- **Linux Extended Telemetry** — On Linux, child process snapshots include working directory, command line, open file descriptors, and I/O stats
- **Telegram Bot** — Optional bot for remote monitoring and push notifications on process events
- **Webhook Dispatcher** — POST process events to one or more URLs with automatic retry
- **Structured Logging** — Tracing via `tracing-subscriber` with configurable log levels via `RUST_LOG`

---

## How It Works

The Rust backend exposes a small HTTP API (served by Axum) that the frontend polls every 2 seconds:

| Endpoint | Method | Description |
| --- | --- | --- |
| `GET /` | GET | Serves the self-contained `view.html` dashboard |
| `GET /health` | GET | Returns server status, version, and uptime |
| `GET /screenshot` | GET | Returns a JSON array of base64-encoded PNG screen captures |
| `GET /process` | GET | Returns root process info, child processes, CPU/memory stats, and `work_done` flag |
| `GET /process/root` | GET | Returns only the root process snapshot, or 404 if it has exited |
| `GET /process/children` | GET | Returns snapshots of all currently live child processes |
| `GET /process/status` | GET | Lightweight summary — root alive/dead, child count, and `work_done` flag |
| `GET /top-processes` | GET | Returns the top N processes sorted by the given key. |
| `POST /shutdown` | POST | Gracefully shuts down the server |

### Expected Response Shapes

**`/screenshot`**

```json
{
  "screens": [
    {
      "mime": "image/png",
      "data": "<base64>",
      "monitor_name": "Built-in Display",
      "monitor_id": 0,
      "width": 1920,
      "height": 1080,
      "timestamp": "2025-01-01T00:00:00Z"
    }
  ],
  "count": 1
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
  "children": [...],
  "timestamp": "2025-01-01T00:00:00Z"
}
```

**`/process/root`**

Returns a single `ProcessInfo` object, or `404` if the root process has exited.

**`/process/status`**

```json
{
  "root_alive": true,
  "root_pid": 1234,
  "root_name": "my-app",
  "child_count": 2,
  "work_done": false,
  "timestamp": "2025-01-01T00:00:00Z"
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

**`/top-processes?sort=cpu&limit=1`**

```json
[
  {
    "name": "my-app",
    "pid": 1234,
    "state": "running",
    "cpu_usage": 12.5,
    "memory_human": "128 MB"
  }
]
```

Process `state` can be `running`, `sleeping`, `gone`, or any other string (rendered as a warning-colored pill).

---

## Getting Started

### Running

```bash
knightwatch --pid <PID>
```

Pass the PID of the root process you want to monitor. The server will start on `0.0.0.0:8083` by default.

### CLI Arguments

| Flag | Default | Description |
| --- | --- | --- |
| `--pid <PID>` | — | PID of the root process to track |
| `--host <HOST>` | `0.0.0.0` | Host address for the API server |
| `--port <PORT>` / `-p` | `8083` | Port for the API server |
| `--no-server` | `false` | Disable the API server entirely |
| `--blind` | `false` | Disable the Screen Capture API (useful on platforms where it requires elevated permissions) |
| `--telegram` | `false` | Enable the Telegram bot |
| `--with-webhook` | `false` | Enable webhook dispatching |
| `--webhook <URL>` | — | Webhook URL to POST process events to (repeatable) |
| `--top-processes` | `false` | Enable top processes tracker |
| `--limit-processes <NUMBER>` | `5` | Limit number of top processes to track (default is 5) |

To run without the API server:

```bash
knightwatch --pid <PID> --no-server
```

To run without screen capture (e.g. on a headless server or where permissions are restricted):

```bash
knightwatch --pid <PID> --blind
```

To enable webhook dispatching with one or more targets:

```bash
knightwatch --pid <PID> --with-webhook --webhook https://example.com/hook
```

To enable tracking top processes with a limit:

```bash
knightwatch --top-processes --limit-processes 10
```

### Log Level

Set the `RUST_LOG` environment variable to control verbosity:

```bash
RUST_LOG=debug knightwatch --pid <PID>
```

---

## Telegram Bot

Knightwatch includes a Telegram bot for remote monitoring and alerting without opening the web dashboard.

### Setup

Store your bot token in persistent config:

```bash
knightwatch config set telegram-token <YOUR_BOT_TOKEN>
```

Clear it if needed:

```bash
knightwatch config set telegram-token --clear
```

Verify it was saved:

```bash
knightwatch config get telegram-token
```

### Enabling

Pass the `--telegram` flag at runtime:

```bash
knightwatch --pid <PID> --telegram
```

### Capabilities

The bot sends push notifications for all process events:

- 🟢 **Initial snapshot** — root and children when tracking begins
- 🆕 **Children appeared** — new child processes detected
- 🔴 **Children exited** — specific child PIDs exited
- ✅ **All children gone** — all child processes have exited
- 💀 **Root process exited** — the root process itself has stopped

---

## Webhooks

Knightwatch can POST process events to one or more HTTP endpoints. Useful for integrating with external orchestration, alerting, or logging pipelines.

### Usage

Enable webhooks with `--with-webhook` and provide one or more targets via `--webhook`:

```bash
knightwatch --pid <PID> --with-webhook --webhook https://example.com/hook --webhook https://other.com/hook
```

Webhook URLs can also be stored in persistent config (merged with any provided via `--webhook` at runtime, deduplicated). See [Persistent Configuration](#persistent-configuration) below.

### Payload Format

```json
{
  "version": "1.0.0",
  "event": "process.children_exited",
  "timestamp": "2025-01-01T00:00:00Z",
  "data": {
    "pids": [5678, 5679]
  }
}
```

**Event names:**

| Event | Description |
| --- | --- |
| `process.initial_snapshot` | First capture after startup |
| `process.children_appeared` | New child processes detected |
| `process.children_exited` | One or more children exited |
| `process.all_children_gone` | All children have exited |
| `process.root_exited` | Root process exited |

Failed deliveries are retried up to 3 times with exponential backoff.

---

## Persistent Configuration

Knightwatch stores settings such as the Telegram token and webhook URLs in a persistent config file, managed via the `config` subcommand. Persisted webhook URLs are merged with any `--webhook` flags provided at runtime and deduplicated.

The `config` subcommand uses `get` and `set` actions with the following fields:

### `telegram-token`

```bash
# Set the token
knightwatch config set telegram-token <TOKEN>

# Clear the token
knightwatch config set telegram-token --clear

# Read the current value
knightwatch config get telegram-token
```

### `webhook-urls`

```bash
# Add one or more URLs
knightwatch config set webhook-urls --add https://example.com/hook --add https://other.com/hook

# Remove a specific URL
knightwatch config set webhook-urls --remove https://example.com/hook

# Clear all stored URLs
knightwatch config set webhook-urls --clear

# List all stored URLs
knightwatch config get webhook-urls
```

## License

MIT
