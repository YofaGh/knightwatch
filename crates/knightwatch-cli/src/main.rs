use clap::Parser;
use reqwest::{Client, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::error::Error;

use kw_types::{
    api::{ContainerRequest, ContainerTimeoutRequest, SetPollIntervalRequest},
    docker::{ContainerSnapshot, DockerSortKey},
    process::{ProcessSignal, ProcessSnapshot, ProcessTree, SortKey},
    resources,
    systemd::UnitSnapshot,
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
        /// Sort key: cpu | memory | disk
        #[arg(long, default_value = "cpu", value_parser = |s: &str| SortKey::try_from(s))]
        sort: SortKey,
        /// Max results (0 = all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List signals supported on this platform
    SupportedSignals,

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

struct ApiClient {
    client: Client,
    base: String,
    token: Option<String>,
}

impl ApiClient {
    fn new(base: String, token: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base,
            token,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base, path)
    }

    fn bearer(&self, req: RequestBuilder) -> RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    async fn get(&self, path: &str) -> Result<Value, Box<dyn Error>> {
        let resp = self.bearer(self.client.get(self.url(path))).send().await?;
        handle(resp).await
    }

    async fn get_typed<T: DeserializeOwned>(&self, path: &str) -> Result<T, Box<dyn Error>> {
        let resp = self.bearer(self.client.get(self.url(path))).send().await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn get_typed_query<P: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        params: P,
    ) -> Result<T, Box<dyn Error>> {
        let resp = self
            .bearer(self.client.get(self.url(path)).query(&params))
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn post<B: Serialize>(
        &self,
        path: &str,
        body: B,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        let resp = self
            .bearer(self.client.post(self.url(path)).json(&body))
            .send()
            .await?;
        handle_post(resp).await
    }

    async fn post_typed<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: B,
    ) -> Result<T, Box<dyn Error>> {
        let resp = self
            .bearer(self.client.post(self.url(path)).json(&body))
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }
}

async fn handle(resp: reqwest::Response) -> Result<Value, Box<dyn Error>> {
    let status = resp.status();
    let text = resp.text().await?;
    if status.is_success() {
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    } else {
        Err(format!("HTTP {status}: {text}").into())
    }
}

async fn handle_post(resp: reqwest::Response) -> Result<Option<Value>, Box<dyn Error>> {
    let status = resp.status();
    let text = resp.text().await?;
    if status.is_success() {
        if text.is_empty() || status == reqwest::StatusCode::NO_CONTENT {
            Ok(None)
        } else {
            Ok(Some(
                serde_json::from_str(&text).unwrap_or(Value::String(text)),
            ))
        }
    } else {
        Err(format!("HTTP {status}: {text}").into())
    }
}

fn print(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn ok() {
    println!("OK");
}

async fn dispatch(command: Commands, api: &ApiClient) -> Result<(), Box<dyn Error>> {
    match command {
        // ── Common ────────────────────────────────────────────────────────
        Commands::Health => {
            let v: kw_types::api::HealthResponse = api.get_typed("/health").await?;
            println!("{v}");
        }
        Commands::Info => {
            let v: kw_types::api::InfoResponse = api.get_typed("/info").await?;
            println!("{v}");
        }
        Commands::Shutdown => {
            if let Some(v) = api.post("/shutdown", json!({})).await? {
                print(&v);
            } else {
                ok();
            }
        }

        // ── Auth ──────────────────────────────────────────────────────────
        Commands::Login { username, password } => {
            let v: kw_types::api::LoginResponse = api
                .post_typed(
                    "/auth/login",
                    kw_types::api::LoginRequest { username, password },
                )
                .await?;
            println!("{v}");
        }
        Commands::Logout => {
            api.post("/auth/logout", json!({})).await?;
            ok();
        }

        // ── Screenshot ────────────────────────────────────────────────────
        Commands::Screenshot => {
            let screenshot_response: kw_types::api::ScreenshotResponse =
                api.get_typed("/screenshot").await?;
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
                println!("Saved {}", filename);
                println!("{}", screen);
            }
        }

        // ── Process tracking ──────────────────────────────────────────────
        Commands::RootPids => print(&api.get("/root_pids").await?),
        Commands::ProcessTree { root_pid } => {
            let v: ProcessTree = api.get_typed(&format!("/process/{root_pid}")).await?;
            println!("{v}");
        }
        Commands::ProcessRoot { root_pid } => {
            let v: ProcessSnapshot = api.get_typed(&format!("/process/root/{root_pid}")).await?;
            println!("{v}");
        }
        Commands::ProcessChildren { root_pid } => {
            let v: Vec<ProcessSnapshot> = api
                .get_typed(&format!("/process/children/{root_pid}"))
                .await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::ProcessStatus { root_pid } => {
            let v: kw_types::process::ProcessStatus = api
                .get_typed(&format!("/process/status/{root_pid}"))
                .await?;
            println!("{v}");
        }
        Commands::ProcessIsDone { root_pid } => {
            print(&api.get(&format!("/process/is-done/{root_pid}")).await?)
        }
        Commands::ProcessTrees => {
            let v: Vec<ProcessTree> = api.get_typed("/process/trees").await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::TopProcesses { sort, limit } => {
            let v: Vec<ProcessSnapshot> = api
                .get_typed_query(
                    "/top-processes",
                    kw_types::api::TopProcessesParams { sort, limit },
                )
                .await?;
            for p in &v {
                println!("{p}");
            }
        }
        Commands::SupportedSignals => print(&api.get("/supported-signals").await?),

        // ── Process commands ──────────────────────────────────────────────
        Commands::KillProcess { pid, signal } => {
            api.post(
                &format!("/process/kill/{pid}"),
                kw_types::api::KillProcessRequest { signal },
            )
            .await?;
            ok();
        }
        Commands::KillTree { root_pid } => {
            if let Some(v) = api
                .post(&format!("/process/kill-tree/{root_pid}"), json!({}))
                .await?
            {
                print(&v);
            }
        }
        Commands::TrackPid { pid } => {
            api.post(&format!("/process/track/{pid}"), json!({}))
                .await?;
            ok();
        }
        Commands::UntrackPid { pid } => {
            api.post(&format!("/process/untrack/{pid}"), json!({}))
                .await?;
            ok();
        }
        Commands::ProcessPollPause => {
            api.post("/process/poll/pause", json!({})).await?;
            ok();
        }
        Commands::ProcessPollResume => {
            api.post("/process/poll/resume", json!({})).await?;
            ok();
        }
        Commands::ProcessPollInterval { interval_ms } => {
            api.post(
                "/process/poll/interval",
                SetPollIntervalRequest { interval_ms },
            )
            .await?;
            ok();
        }

        // ── Screen capture commands ───────────────────────────────────────
        Commands::ScreenPollPause => {
            api.post("/screen/poll/pause", json!({})).await?;
            ok();
        }
        Commands::ScreenPollResume => {
            api.post("/screen/poll/resume", json!({})).await?;
            ok();
        }
        Commands::ScreenPollInterval { interval_ms } => {
            api.post(
                "/screen/poll/interval",
                SetPollIntervalRequest { interval_ms },
            )
            .await?;
            ok();
        }

        // ── System resources ──────────────────────────────────────────────
        Commands::System => {
            let v: resources::SystemSnapshot = api.get_typed("/system").await?;
            println!("{v}");
        }
        Commands::Cpu => {
            let v: resources::CpuSnapshot = api.get_typed("/cpu").await?;
            println!("{v}");
        }
        Commands::Memory => {
            let v: resources::MemorySnapshot = api.get_typed("/memory").await?;
            println!("{v}");
        }
        Commands::Disks => {
            let v: Vec<resources::DiskSnapshot> = api.get_typed("/disks").await?;
            for d in &v {
                println!("{d}");
            }
        }
        Commands::Networks => {
            let v: Vec<resources::NetworkSnapshot> = api.get_typed("/networks").await?;
            for n in &v {
                println!("{n}");
            }
        }
        Commands::Gpus => {
            let v: Vec<resources::GpuSnapshot> = api.get_typed("/gpus").await?;
            for g in &v {
                println!("{g}");
            }
        }
        Commands::Battery => {
            let v: resources::BatterySnapshot = api.get_typed("/battery").await?;
            println!("{v}");
        }
        Commands::HostInfo => {
            let v: resources::HostInfo = api.get_typed("/host-info").await?;
            println!("{v}");
        }
        Commands::Temperatures => {
            let v: Vec<resources::ThermalSnapshot> = api.get_typed("/temperatures").await?;
            for t in &v {
                println!("{t}");
            }
        }

        // ── System resource commands ──────────────────────────────────────
        Commands::SetThresholds {
            cpu_warn,
            memory_warn,
            disk_warn,
            battery_low,
        } => {
            api.post(
                "/resources/thresholds",
                kw_types::api::SetThresholdsRequest {
                    cpu_warn,
                    memory_warn,
                    disk_warn,
                    battery_low,
                },
            )
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
            api.post(
                "/resources/refresh-mask",
                kw_types::api::SetRefreshMaskRequest {
                    cpu,
                    memory,
                    disks,
                    networks,
                    temperatures,
                    gpus,
                },
            )
            .await?;
            ok();
        }
        Commands::ResourcesPollPause => {
            api.post("/resources/poll/pause", json!({})).await?;
            ok();
        }
        Commands::ResourcesPollResume => {
            api.post("/resources/poll/resume", json!({})).await?;
            ok();
        }
        Commands::ResourcesPollInterval { interval_ms } => {
            api.post(
                "/resources/poll/interval",
                SetPollIntervalRequest { interval_ms },
            )
            .await?;
            ok();
        }

        // ── Systemd ───────────────────────────────────────────────────────
        Commands::Systemd => {
            let v: Vec<kw_types::systemd::SystemdSnapshot> = api.get_typed("/systemd").await?;
            for s in &v {
                println!("{s}");
            }
        }
        Commands::Unit { unit_name } => {
            let v: UnitSnapshot = api.get_typed(&format!("/unit/{unit_name}")).await?;
            println!("{v}");
        }
        Commands::UnitsByState { unit_state } => {
            let v: Vec<UnitSnapshot> = api.get_typed(&format!("/units/{unit_state}")).await?;
            for u in &v {
                println!("{u}");
            }
        }
        Commands::FailedUnits => {
            let v: Vec<UnitSnapshot> = api.get_typed("/failed_units").await?;
            for u in &v {
                println!("{u}");
            }
        }
        Commands::SystemdPollPause => {
            api.post("/systemd/poll/pause", json!({})).await?;
            ok();
        }
        Commands::SystemdPollResume => {
            api.post("/systemd/poll/resume", json!({})).await?;
            ok();
        }
        Commands::SystemdPollInterval { interval_ms } => {
            api.post(
                "/systemd/poll/interval",
                SetPollIntervalRequest { interval_ms },
            )
            .await?;
            ok();
        }

        // ── Docker ────────────────────────────────────────────────────────
        Commands::DockerContainers => {
            let v: Vec<ContainerSnapshot> = api.get_typed("/docker-containers").await?;
            for c in &v {
                println!("{c}");
            }
        }
        Commands::Container { id_or_name } => {
            let v: ContainerSnapshot = api.get_typed(&format!("/container/{id_or_name}")).await?;
            println!("{v}");
        }
        Commands::TopContainers { sort, limit } => {
            let v: Vec<ContainerSnapshot> = api
                .get_typed_query(
                    "/top-containers",
                    kw_types::api::TopContainersParams { sort, limit },
                )
                .await?;
            for c in &v {
                println!("{c}");
            }
        }

        // ── Docker commands ───────────────────────────────────────────────
        Commands::StopContainer {
            id_or_name,
            timeout_secs,
        } => {
            api.post(
                "/docker/stop-container",
                ContainerTimeoutRequest {
                    id_or_name,
                    timeout_secs,
                },
            )
            .await?;
            ok();
        }
        Commands::KillContainer { id_or_name, signal } => {
            api.post(
                "/docker/kill-container",
                kw_types::api::KillContainerRequest { id_or_name, signal },
            )
            .await?;
            ok();
        }
        Commands::StartContainer { id_or_name } => {
            api.post("/docker/start-container", ContainerRequest { id_or_name })
                .await?;
            ok();
        }
        Commands::RestartContainer {
            id_or_name,
            timeout_secs,
        } => {
            api.post(
                "/docker/restart-container",
                ContainerTimeoutRequest {
                    id_or_name,
                    timeout_secs,
                },
            )
            .await?;
            ok();
        }
        Commands::PauseContainer { id_or_name } => {
            api.post("/docker/pause-container", ContainerRequest { id_or_name })
                .await?;
            ok();
        }
        Commands::UnpauseContainer { id_or_name } => {
            api.post("/docker/unpause-container", ContainerRequest { id_or_name })
                .await?;
            ok();
        }
        Commands::DockerPollPause => {
            api.post("/docker/poll/pause", json!({})).await?;
            ok();
        }
        Commands::DockerPollResume => {
            api.post("/docker/poll/resume", json!({})).await?;
            ok();
        }
        Commands::DockerPollInterval { interval_ms } => {
            api.post(
                "/docker/poll/interval",
                SetPollIntervalRequest { interval_ms },
            )
            .await?;
            ok();
        }
        Commands::Interactive => {
            // Handled before dispatch is called — should never reach here.
            unreachable!()
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let api = ApiClient::new(cli.url, cli.token);

    if matches!(cli.command, Commands::Interactive) {
        interactive::run_interactive(api).await;
        return Ok(());
    }

    dispatch(cli.command, &api).await
}
