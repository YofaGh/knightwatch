# 🖥️ Screen View

A lightweight, real-time browser-based dashboard for monitoring live screenshots and process activity on a remote or local machine.

---

## Overview

Screen View provides a sleek dark-mode web interface that streams live screen captures and process telemetry directly in your browser — no heavy agents or desktop apps required. It's designed for quick visual oversight of a running system, whether you're monitoring a headless server, a build machine, or a long-running automation task.

---

## Features

- **Live Screenshots** — Displays one or more connected screens, refreshed every 2 seconds via `/screenshot`
- **Process Monitor** — Tracks a root process and its children with real-time CPU, memory, and state indicators
- **Work-Done Detection** — Automatically shows a completion banner when all child processes have exited
- **Responsive Layout** — Side-by-side panels on desktop, stacked on mobile
- **Zero Dependencies** — Pure HTML/CSS/JS frontend; no frameworks required

---

## How It Works

The frontend polls two local endpoints every 2 seconds:

| Endpoint | Description |
|---|---|
| `GET /screenshot` | Returns a JSON array of base64-encoded screen captures with MIME type |
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

Process `state` can be `running`, `sleeping`, `gone`, or any other string (rendered as a warning-colored pill).

---

## Getting Started

1. Serve `view.html` from any static file server or embed it in your backend
2. Implement the `/screenshot` and `/process` endpoints in your backend of choice
3. Open the page in a browser — it will begin polling automatically

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
|---|---|
| `GET /top` | Returns a ranked list of processes sorted by CPU or RAM usage |

Planned query parameters:

- `?sort=cpu` — sort by CPU usage (default)
- `?sort=mem` — sort by memory usage
- `?limit=10` — control how many processes are returned

This will also be surfaced in the dashboard as a dedicated "Top Processes" panel alongside the existing process monitor.

---

## Project Structure

```
.
├── view.html        # Frontend dashboard (self-contained)
└── README.md
```

---

## License

MIT
