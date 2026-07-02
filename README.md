# 🖥️ Knightwatch

A lightweight, real-time browser-based dashboard for monitoring system performance, live screenshots, and process activity on a remote or local machine.

---

## Overview

Knightwatch provides a sleek dark-mode web interface that streams system performance, live screen captures, and process telemetry directly in your browser. The backend is a Rust server built on [Tokio](https://tokio.rs/) and [Axum](https://github.com/tokio-rs/axum), keeping the footprint small and performance high — no heavy agents or desktop apps required.

A companion `knightwatch-cli` client is also available for connecting to a remote or local knightwatch server through its API from the terminal.

---

## Features

- **REST API** — Axum-powered HTTP API for querying state and sending commands programmatically
- **Server-Sent Events (SSE)** — Real-time event stream available at `/sse` via the Axum API server
- **Live Screenshots** — Displays one or more connected screens, refreshed every 2 seconds
- **Screen Commands** — control polling via API or Telegram (requires `--allow-screen-commands`)
- **Process Monitor** — Tracks a root process and its children with real-time CPU, memory, and state indicators
- **Process Commands** — Kill, track, untrack processes and control polling via API or Telegram (requires `--allow-process-commands`)
- **Linux Extended Telemetry** — Child process snapshots include working directory, command line, open file descriptors, and I/O stats
- **System Resources Monitor** — Real-time hardware telemetry: CPU, memory, disks, network, battery, thermals, and aggregate health scoring
- **System Resources Commands** — Set tresholds, refresh masks and control polling via API or Telegram (requires `--allow-system-resources-commands`)
- **Systemd Monitor** — Live systemd unit tracking with active/failed/inactive counts, per-unit state, resource usage, and change events (Linux only)
- **Systemd Commands** — control polling via API or Telegram (requires `--allow-systemd-commands`)
- **Docker Containers Monitor** — Monitor docker containers and track their resource usages
- **Docker Commands** — Manager Containers and control polling via API or Telegram (requires `--allow-docker-commands`)
- **Telegram Bot** — Optional bot for remote monitoring, push notifications, and process commands
- **Webhook Dispatcher** — POST process and system events to one or more URLs with automatic retry

---

## Installation

Knightwatch ships two binaries: the `knightwatch` server and the `knightwatch-cli` client, which connects to a remote or local knightwatch server through its API. Both are installed with the same scripts — just pick a package.

### Shell (macOS & Linux)

```bash
# Install the server (default package)
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.ps1 | sh

# Install the CLI client
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.sh | sh -s -- --package knightwatch-cli
```

You can also pin a specific version with `--version <VERSION>`.

### PowerShell (Windows)

```powershell
# Install the server (default package)
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.ps1 | iex"
```

Piping into `iex` can't forward `-Package`/`-Version` arguments, so to install the CLI (or pin a version), download the script and run it directly:

```powershell
iwr -useb https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.ps1 -OutFile install.ps1
.\install.ps1 -Package knightwatch-cli -Version 1.0.0
```

### Homebrew (macOS)

```bash
# Install the server
brew install YofaGh/tap/knightwatch

# Install the CLI client
brew install YofaGh/tap/knightwatch-cli
```

### Linux Servers (Headless)

For Linux servers without a display server, a headless build (no screenshot support) is available as a standalone tarball on the [Releases page](https://github.com/YofaGh/knightwatch/releases/latest). Download `knightwatch-headless-x86_64-unknown-linux.tar.gz` and extract it:

```bash
tar -xzf knightwatch-headless-x86_64-unknown-linux.tar.gz
chmod +x knightwatch-headless
./knightwatch-headless
```

### Pre-built Binaries

Standalone binaries for all supported platforms are available on the [Releases page](https://github.com/YofaGh/knightwatch/releases/latest).

---

## Getting Started

The server starts on `0.0.0.0:8083` by default and serves the dashboard at `http://localhost:8083`.
All tracking flags are optional — enable only what you need:

```bash
# Track one or more processes
knightwatch --pid <PID>

# Monitor system resources (CPU, memory, disks, network, battery, thermals)
knightwatch --system-resources

# Track systemd units (Linux only)
knightwatch --systemd

# Track docker containers
knightwatch --docker

# Track top processes by CPU/memory
knightwatch --top-processes

# Combine any of the above
knightwatch --pid <PID> --system-resources --systemd

# Enable Telegram bot
knightwatch --pid <PID> --telegram

# Enable webhook dispatching
knightwatch --pid <PID> --with-webhook --webhook https://example.com/hook

# Run headless (no screen capture)
knightwatch --pid <PID> --blind

# Enable process commands (kill, track, untrack, poll control)
knightwatch --pid <PID> --allow-process-commands
```

### CLI Flags

| Flag | Default | Description |
| --- | --- | --- |
| `--pid <PID>` | — | PID of a process to track (repeatable, optional) |
| `--host <HOST>` | `0.0.0.0` | Host address for the API server |
| `--port <PORT>` / `-p` | `8083` | Port for the API server |
| `--enable-auth` | `false` | Enable authentication |
| `--enable-shutdown` | `false` | Enable app shutdown remotely |
| `--no-api` | `false` | Disable the API server entirely |
| `--no-dashboard` | `false` | Disable the web dashboard |
| `--blind` | `false` | Disable screen capture |
| `--system-resources` | `false` | Enable CPU, memory, disk, network, battery, and thermal monitoring |
| `--systemd` | `false` | Enable systemd monitor (Linux only) |
| `--docker` | `false` | Enable docker tracker |
| `--telegram` | `false` | Enable the Telegram bot |
| `--with-webhook` | `false` | Enable webhook dispatching |
| `--webhook <URL>` | — | Webhook target URL (repeatable) |
| `--top-processes` | `false` | Enable top processes tracker |
| `--limit-processes <N>` | `5` | Number of top processes to track |
| `--allow-process-commands` | `false` | Enable process command endpoints (kill, track, untrack, poll control) — **always requires authentication** |
| `--allow-screen-commands` | `false` | Enable screen command endpoints (poll control) — **always requires authentication** |
| `--allow-system-resources-commands` | `false` | Enable system resources command endpoints (set tresholds, refresh masks, poll control) — **always requires authentication** |
| `--allow-systemd-commands` | `false` | Enable systemd command endpoints (poll control) — **always requires authentication** |
| `--allow-docker-commands` | `false` | Enable docker command endpoints (manager containers, poll control) — **always requires authentication** |

> **Note:** `allowing commands` always requires authentication regardless of the `--enable-auth` flag. The auth session endpoints are automatically enabled when this flag is set.

### Log Level

Set the `RUST_LOG` environment variable to control verbosity:

```bash
RUST_LOG=debug knightwatch --pid <PID>
```

---

## Documentation

Full reference documentation is available in the [Wiki](https://github.com/YofaGh/knightwatch/wiki):

- [API Reference](https://github.com/YofaGh/knightwatch/wiki/API-Reference) — All endpoints and response shapes
- [Process Tracker](https://github.com/YofaGh/knightwatch/wiki/Process-Tracker) — Process tracking, commands, and events
- [System Resources Monitor](https://github.com/YofaGh/knightwatch/wiki/System-Resources-Monitor) — Hardware telemetry and thresholds
- [Systemd Monitor](https://github.com/YofaGh/knightwatch/wiki/Systemd-Monitor) — Unit tracking and events (Linux only)
- [Docker Tracker](https://github.com/YofaGh/knightwatch/wiki/Docker-Tracker) — Docker containers tracking, commands and events
- [Telegram Bot](https://github.com/YofaGh/knightwatch/wiki/Telegram-Bot) — Setup, commands, and notifications
- [Webhooks](https://github.com/YofaGh/knightwatch/wiki/Webhooks) — Payload format and event catalogue
- [Authentication](https://github.com/YofaGh/knightwatch/wiki/Authentication) — User management and API auth
- [Persistent Configuration](https://github.com/YofaGh/knightwatch/wiki/Persistent-Configuration) — Stored settings via `config` subcommand
- [Knightwatch CLI](https://github.com/YofaGh/knightwatch/wiki/Knightwatch-cli) — Using `knightwatch-cli` to connect to a remote or local server

---

## License

MIT
