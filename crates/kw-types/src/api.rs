use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub uptime: String,
}

impl fmt::Display for HealthResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "status: {}  version: {}  uptime: {}  ({})",
            self.status, self.version, self.uptime, self.timestamp
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InfoResponse {
    pub auth_enabled: bool,
    pub shutdown_enabled: bool,
    pub blind: bool,
    pub pid: Vec<u32>,
    pub top_processes: bool,
    pub limit_processes: usize,
    pub telegram_bot: bool,
    pub system_resources: bool,
    pub systemd: bool,
    pub docker: bool,
    pub allow_process_commands: bool,
    pub allow_screen_commands: bool,
    pub allow_system_resources_commands: bool,
    pub allow_systemd_commands: bool,
    pub allow_docker_commands: bool,
}

impl fmt::Display for InfoResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let pids: Vec<String> = self.pid.iter().map(|p| p.to_string()).collect();
        writeln!(f, "auth enabled:    {}", self.auth_enabled)?;
        writeln!(f, "shutdown enabled:    {}", self.shutdown_enabled)?;
        writeln!(f, "blind:           {}", self.blind)?;
        writeln!(
            f,
            "tracked PIDs:    {}",
            if pids.is_empty() {
                "none".into()
            } else {
                pids.join(", ")
            }
        )?;
        writeln!(
            f,
            "top processes:   {}  (limit: {})",
            self.top_processes, self.limit_processes
        )?;
        writeln!(f, "telegram bot:    {}", self.telegram_bot)?;
        writeln!(
            f,
            "features:        system_resources={} systemd={} docker={}",
            self.system_resources, self.systemd, self.docker
        )?;
        writeln!(
            f,
            "commands:        process={} screen={} resources={} systemd={} docker={}",
            self.allow_process_commands,
            self.allow_screen_commands,
            self.allow_system_resources_commands,
            self.allow_systemd_commands,
            self.allow_docker_commands
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

impl fmt::Display for LoginResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "token: {}", self.token)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScreenshotImage {
    pub data: String,
    pub mime: String,
    pub monitor_name: String,
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    pub timestamp: String,
}

impl fmt::Display for ScreenshotImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Screenshot[monitor={} ({}), {}x{}, mime={}, taken_at={}]",
            self.monitor_name,
            self.monitor_id,
            self.width,
            self.height,
            self.mime,
            self.timestamp
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotResponse {
    pub screens: Vec<ScreenshotImage>,
    pub count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TopProcessesParams {
    pub limit: Option<usize>,
    pub sort: crate::process::ProcessesSortKey,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KillProcessRequest {
    pub signal: crate::process::ProcessSignal,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetPollIntervalRequest {
    pub interval_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetThresholdsRequest {
    pub cpu_warn: f32,
    pub memory_warn: f32,
    pub disk_warn: f32,
    pub battery_low: f32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetRefreshMaskRequest {
    pub cpu: bool,
    pub memory: bool,
    pub disks: bool,
    pub networks: bool,
    pub temperatures: bool,
    pub gpus: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TopContainersParams {
    pub sort: crate::docker::DockerSortKey,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ContainerRequest {
    pub id_or_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KillContainerRequest {
    pub id_or_name: String,
    pub signal: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ContainerTimeoutRequest {
    pub id_or_name: String,
    pub timeout_secs: Option<i32>,
}
