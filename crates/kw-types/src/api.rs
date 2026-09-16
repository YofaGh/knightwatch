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
            self.monitor_name, self.monitor_id, self.width, self.height, self.mime, self.timestamp
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
pub struct ControlUnitParams {
    pub unit_name: String,
    pub action: crate::systemd::ServiceAction,
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
