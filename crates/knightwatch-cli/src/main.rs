#![allow(clippy::print_stdout)]

use clap::Parser;
use std::error::Error;

use kw_types::{
    docker::DockerSortKey,
    process::{ProcessSignal, ProcessesSortKey},
    systemd::ServiceAction,
};

mod colors;
mod interactive;

/// CLI client for Knightwatch API
#[derive(Parser)]
#[command(name = "kwctl", about = "CLI client for Knightwatch API", version)]
struct Cli {
    /// Base URL of the API server
    #[arg(long, short, env = "KW_URL", default_value = "http://localhost:8083")]
    url: String,

    /// Bearer token for authenticated requests
    #[arg(long, short, env = "KW_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    // ── Interactive ────────────────────────────────────────────────────────
    Interactive,
    // ── Common ────────────────────────────────────────────────────────────
    /// Check server health
    Health,
    /// Get server info / feature flags
    Info,
    /// Shut down the server
    Shutdown,

    // ── Auth ──────────────────────────────────────────────────────────────
    /// Log in and receive a bearer token
    Login {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
    },
    /// Log out (invalidate the current token)
    Logout,

    // ── Screenshot ────────────────────────────────────────────────────────
    /// Fetch the latest screenshots (base64 JSON)
    Screenshot,

    /// Fetch screen capture poll status
    ScreenCapturePollStatus,

    // ── Process tracking ──────────────────────────────────────────────────
    /// List tracked root PIDs
    RootPids,
    /// Full process tree for a root PID
    ProcessTree {
        root_pid: u32,
    },
    /// Root process snapshot only
    ProcessRoot {
        root_pid: u32,
    },
    /// Child process snapshots
    ProcessChildren {
        root_pid: u32,
    },
    /// Lightweight process status summary
    ProcessStatus {
        root_pid: u32,
    },
    /// Check whether all children of a root PID have exited
    ProcessIsDone {
        root_pid: u32,
    },
    /// All tracked process trees
    ProcessTrees,
    /// Top processes by CPU/memory/disk
    TopProcesses {
        /// Processes Sort key: cpu | memory | disk
        #[arg(long, default_value = "cpu", value_parser = |s: &str| ProcessesSortKey::try_from(s))]
        sort: ProcessesSortKey,
        /// Max results (0 = all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List signals supported on this platform
    SupportedSignals,

    /// Fetch process tracker poll status
    ProcessTrackerPollStatus,

    // ── Process commands ──────────────────────────────────────────────────
    /// Send a signal to a process
    KillProcess {
        pid: u32,
        #[arg(long, default_value = "term", value_parser = |s: &str| ProcessSignal::try_from(s))]
        signal: ProcessSignal,
    },
    /// Kill an entire process tree
    KillTree {
        root_pid: u32,
    },
    /// Start tracking a PID
    TrackPid {
        pid: u32,
    },
    /// Stop tracking a PID
    UntrackPid {
        pid: u32,
    },
    /// Pause the process tracker poll loop
    ProcessPollPause,
    /// Resume the process tracker poll loop
    ProcessPollResume,
    /// Set the process tracker poll interval
    ProcessPollInterval {
        /// Interval in milliseconds
        interval_ms: u64,
    },

    // ── Screen capture commands ───────────────────────────────────────────
    /// Pause the screen capture poll loop
    ScreenPollPause,
    /// Resume the screen capture poll loop
    ScreenPollResume,
    /// Set the screen capture poll interval
    ScreenPollInterval {
        interval_ms: u64,
    },

    // ── System resources ──────────────────────────────────────────────────
    /// Full system snapshot
    System,
    /// CPU snapshot
    Cpu,
    /// Memory snapshot
    Memory,
    /// Disk snapshots
    Disks,
    /// Network snapshots
    Networks,
    /// GPU snapshots
    Gpus,
    /// Battery snapshot
    Battery,
    /// Host info
    HostInfo,
    /// Temperature snapshots
    Temperatures,
    /// System Alarms
    Alarms,

    /// Fetch system resources poll status
    SystemResourcesPollStatus,

    // ── System resource commands ──────────────────────────────────────────
    /// Set alert thresholds
    SetThresholds {
        #[arg(long)]
        cpu_warn: f32,
        #[arg(long)]
        memory_warn: f32,
        #[arg(long)]
        disk_warn: f32,
        #[arg(long)]
        battery_low: f32,
    },
    /// Set resource refresh mask (choose which subsystems to collect)
    SetRefreshMask {
        #[arg(long)]
        cpu: bool,
        #[arg(long)]
        memory: bool,
        #[arg(long)]
        disks: bool,
        #[arg(long)]
        networks: bool,
        #[arg(long)]
        temperatures: bool,
        #[arg(long)]
        gpus: bool,
    },
    /// Pause the system resources poll loop
    ResourcesPollPause,
    /// Resume the system resources poll loop
    ResourcesPollResume,
    /// Set the system resources poll interval
    ResourcesPollInterval {
        interval_ms: u64,
    },

    // ── Systemd ───────────────────────────────────────────────────────────
    /// Systemd snapshot (all units)
    Systemd,
    /// Snapshot of a single unit
    Unit {
        unit_name: String,
    },
    /// Units filtered by active state (e.g. active, failed, inactive)
    UnitsByState {
        unit_state: String,
    },
    /// List failed units
    FailedUnits,

    /// Fetch systemd poll status
    SystemdPollStatus,

    // ── Systemd commands ──────────────────────────────────────────────────
    /// control a unit
    ControlUnit {
        unit_name: String,
        #[arg(long, default_value = "start", value_parser = |s: &str| ServiceAction::try_from(s))]
        action: ServiceAction,
    },
    /// Pause the systemd poll loop
    SystemdPollPause,
    /// Resume the systemd poll loop
    SystemdPollResume,
    /// Set the systemd poll interval
    SystemdPollInterval {
        interval_ms: u64,
    },

    // ── Docker ────────────────────────────────────────────────────────────
    /// List all docker containers
    DockerContainers,
    /// Get a container by ID or name
    Container {
        id_or_name: String,
    },
    /// Top containers by CPU/memory
    TopContainers {
        /// Sort key: cpu | memory
        #[arg(long, default_value = "cpu", value_parser = |s: &str| DockerSortKey::try_from(s))]
        sort: DockerSortKey,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Fetch docker tracker poll status
    DockerTrackerPollStatus,

    // ── Docker commands ───────────────────────────────────────────────────
    /// Stop a container
    StopContainer {
        id_or_name: String,
        #[arg(long)]
        timeout_secs: Option<i32>,
    },
    /// Kill a container with a signal
    KillContainer {
        id_or_name: String,
        /// Signal string, e.g. SIGKILL
        #[arg(long)]
        signal: Option<String>,
    },
    /// Start a container
    StartContainer {
        id_or_name: String,
    },
    /// Restart a container
    RestartContainer {
        id_or_name: String,
        #[arg(long)]
        timeout_secs: Option<i32>,
    },
    /// Pause a container
    PauseContainer {
        id_or_name: String,
    },
    /// Unpause a container
    UnpauseContainer {
        id_or_name: String,
    },
    /// Pause the docker tracker poll loop
    DockerPollPause,
    /// Resume the docker tracker poll loop
    DockerPollResume,
    /// Set the docker tracker poll interval
    DockerPollInterval {
        interval_ms: u64,
    },
}

fn print<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn ok() {
    println!("OK");
}

async fn dispatch(command: Commands, api: &kw_clients::ApiClient) -> Result<(), Box<dyn Error>> {
    match command {
        // ── Common ────────────────────────────────────────────────────────
        Commands::Health => {
            println!("{}", api.health().await?);
        }
        Commands::Info => {
            println!("{}", api.info().await?);
        }
        Commands::Shutdown => {
            if let Some(v) = api.shutdown().await? {
                print(&v);
            } else {
                ok();
            }
        }

        // ── Auth ──────────────────────────────────────────────────────────
        Commands::Login { username, password } => {
            println!("{}", api.login(username, password).await?);
        }
        Commands::Logout => {
            api.logout().await?;
            ok();
        }

        // ── Screenshot ────────────────────────────────────────────────────
        Commands::Screenshot => {
            let screenshot_response = api.screenshot().await?;
            for screen in screenshot_response.screens {
                let ext = screen.mime.split('/').nth(1).unwrap_or("png");
                let filename = format!(
                    "screenshot_{}_{}.{}",
                    screen.monitor_id,
                    screen.timestamp.replace(':', "-"),
                    ext
                );
                let bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &screen.data,
                )?;
                std::fs::write(&filename, bytes)?;
                println!("Saved {filename}");
                println!("{screen}");
            }
        }
        Commands::ScreenCapturePollStatus => {
            println!("screen capture poll status: {}", api.screen_capture_poll_status().await?);
        }

        // ── Screen capture commands ───────────────────────────────────────
        Commands::ScreenPollPause => {
            api.screen_capture_poll_pause().await?;
            ok();
        }
        Commands::ScreenPollResume => {
            api.screen_capture_poll_resume().await?;
            ok();
        }
        Commands::ScreenPollInterval { interval_ms } => {
            api.screen_capture_interval(interval_ms).await?;
            ok();
        }

        // ── Process tracking ──────────────────────────────────────────────
        Commands::RootPids => print(&api.root_pids().await?),
        Commands::ProcessTree { root_pid } => {
            println!("{}", api.process_tree(root_pid).await?);
        }
        Commands::ProcessRoot { root_pid } => {
            println!("{}", api.process_root(root_pid).await?);
        }
        Commands::ProcessChildren { root_pid } => {
            let v = api.process_children(root_pid).await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::ProcessStatus { root_pid } => {
            println!("{}", api.process_status(root_pid).await?);
        }
        Commands::ProcessIsDone { root_pid } => print(&api.process_is_done(root_pid).await?),
        Commands::ProcessTrees => {
            let v = api.process_trees().await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::TopProcesses { sort, limit } => {
            let v = api.top_processes(sort, limit).await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::SupportedSignals => {
            print(&api.supported_signals().await?);
        }
        Commands::ProcessTrackerPollStatus => {
            println!("process tracker poll status: {}", api.process_tracker_poll_status().await?);
        }

        // ── Process commands ──────────────────────────────────────────────
        Commands::KillProcess { pid, signal } => {
            api.kill_process(pid, signal).await?;
            ok();
        }
        Commands::KillTree { root_pid } => {
            print(&api.kill_process_tree(root_pid).await?);
        }
        Commands::TrackPid { pid } => {
            api.track_pid(pid).await?;
            ok();
        }
        Commands::UntrackPid { pid } => {
            api.untrack_pid(pid).await?;
            ok();
        }
        Commands::ProcessPollPause => {
            api.process_poll_pause().await?;
            ok();
        }
        Commands::ProcessPollResume => {
            api.process_poll_resume().await?;
            ok();
        }
        Commands::ProcessPollInterval { interval_ms } => {
            api.process_poll_interval(interval_ms).await?;
            ok();
        }

        // ── System resources ──────────────────────────────────────────────
        Commands::System => {
            println!("{}", api.system_snapshot().await?);
        }
        Commands::Cpu => {
            println!("{}", api.cpu_snapshot().await?);
        }
        Commands::Memory => {
            println!("{}", api.memory_snapshot().await?);
        }
        Commands::Disks => {
            let v = api.disk_snapshots().await?;
            for d in &v {
                println!("{d}");
            }
        }
        Commands::Networks => {
            let v = api.network_snapshots().await?;
            for n in &v {
                println!("{n}");
            }
        }
        Commands::Gpus => {
            let v = api.gpu_snapshots().await?;
            for g in &v {
                println!("{g}");
            }
        }
        Commands::Battery => {
            println!("{}", api.battery_snapshot().await?);
        }
        Commands::HostInfo => {
            println!("{}", api.host_info().await?);
        }
        Commands::Temperatures => {
            let v = api.temperatures().await?;
            for t in &v {
                println!("{t}");
            }
        }
        Commands::Alarms => {
            println!("{}", api.alarms().await?);
        }
        Commands::SystemResourcesPollStatus => {
            println!("screen capture poll status: {}", api.system_resources_poll_status().await?);
        }

        // ── System resource commands ──────────────────────────────────────
        Commands::SetThresholds {
            cpu_warn,
            memory_warn,
            disk_warn,
            battery_low,
        } => {
            api.set_thresholds(cpu_warn, memory_warn, disk_warn, battery_low)
                .await?;
            ok();
        }
        Commands::SetRefreshMask {
            cpu,
            memory,
            disks,
            networks,
            temperatures,
            gpus,
        } => {
            api.set_refresh_mask(cpu, memory, disks, networks, temperatures, gpus)
                .await?;
            ok();
        }
        Commands::ResourcesPollPause => {
            api.resources_poll_pause().await?;
            ok();
        }
        Commands::ResourcesPollResume => {
            api.resources_poll_resume().await?;
            ok();
        }
        Commands::ResourcesPollInterval { interval_ms } => {
            api.resources_poll_interval(interval_ms).await?;
            ok();
        }

        // ── Systemd ───────────────────────────────────────────────────────
        Commands::Systemd => {
            println!("{}", api.systemd_snapshot().await?);
        }
        Commands::Unit { unit_name } => {
            println!("{}", api.unit_snapshot(&unit_name).await?);
        }
        Commands::UnitsByState { unit_state } => {
            let v = api.units_by_state(&unit_state).await?;
            for u in &v {
                println!("{u}");
            }
        }
        Commands::FailedUnits => {
            let v = api.failed_units().await?;
            for u in &v {
                println!("{u}");
            }
        }
        Commands::SystemdPollStatus => {
            println!("systemd poll status: {}", api.systemd_poll_status().await?);
        }

        // ── Systemd commands ──────────────────────────────────────────────
        Commands::ControlUnit { unit_name, action } => {
            api.control_unit(&unit_name, action).await?;
            ok();
        }
        Commands::SystemdPollPause => {
            api.systemd_poll_pause().await?;
            ok();
        }
        Commands::SystemdPollResume => {
            api.systemd_poll_resume().await?;
            ok();
        }
        Commands::SystemdPollInterval { interval_ms } => {
            api.systemd_poll_interval(interval_ms).await?;
            ok();
        }

        // ── Docker ────────────────────────────────────────────────────────
        Commands::DockerContainers => {
            let v = api.docker_containers().await?;
            for c in &v {
                println!("{c}");
            }
        }
        Commands::Container { id_or_name } => {
            let v = api.docker_container(&id_or_name).await?;
            println!("{v}");
        }
        Commands::TopContainers { sort, limit } => {
            let v = api.top_containers(sort, limit).await?;
            for c in &v {
                println!("{c}");
            }
        }
        Commands::DockerTrackerPollStatus => {
            println!("docker tracker poll status: {}", api.docker_tracker_poll_status().await?);
        }

        // ── Docker commands ───────────────────────────────────────────────
        Commands::StopContainer {
            id_or_name,
            timeout_secs,
        } => {
            api.stop_container(&id_or_name, timeout_secs).await?;
            ok();
        }
        Commands::KillContainer { id_or_name, signal } => {
            api.kill_container(&id_or_name, signal).await?;
            ok();
        }
        Commands::StartContainer { id_or_name } => {
            api.start_container(&id_or_name).await?;
            ok();
        }
        Commands::RestartContainer {
            id_or_name,
            timeout_secs,
        } => {
            api.restart_container(&id_or_name, timeout_secs).await?;
            ok();
        }
        Commands::PauseContainer { id_or_name } => {
            api.pause_container(&id_or_name).await?;
            ok();
        }
        Commands::UnpauseContainer { id_or_name } => {
            api.unpause_container(&id_or_name).await?;
            ok();
        }
        Commands::DockerPollPause => {
            api.docker_poll_pause().await?;
            ok();
        }
        Commands::DockerPollResume => {
            api.docker_poll_resume().await?;
            ok();
        }
        Commands::DockerPollInterval { interval_ms } => {
            api.docker_poll_interval(interval_ms).await?;
            ok();
        }
        Commands::Interactive => {
            // Handled before dispatch is called — should never reach here.
            // unreachable!()
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let api = kw_clients::ApiClient::new(&cli.url, cli.token);

    if matches!(cli.command, Commands::Interactive) {
        interactive::run_interactive(api).await;
        return Ok(());
    }

    dispatch(cli.command, &api).await
}
