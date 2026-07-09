use reqwest::{Client, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use kw_types::{
    api::{ContainerRequest, ContainerTimeoutRequest, SetPollIntervalRequest},
    docker::ContainerSnapshot,
    process::{ProcessSignal, ProcessSnapshot, ProcessTree},
    resources,
    systemd::UnitSnapshot,
};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

pub struct ApiClient {
    client: Client,
    base: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base: String, token: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base: base.trim_end_matches('/').to_string(),
            token,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base, path)
    }

    pub fn bearer(&self, req: RequestBuilder) -> RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    async fn get_typed<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.bearer(self.client.get(self.url(path))).send().await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn get_typed_query<P: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        params: P,
    ) -> Result<T> {
        let resp = self
            .bearer(self.client.get(self.url(path)).query(&params))
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn post<B: Serialize>(&self, path: &str, body: B) -> Result<Option<Value>> {
        let resp = self
            .bearer(self.client.post(self.url(path)).json(&body))
            .send()
            .await?;
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

    async fn post_typed<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: B,
    ) -> Result<T> {
        let resp = self
            .bearer(self.client.post(self.url(path)).json(&body))
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    // ── Common ────────────────────────────────────────────────────────

    pub async fn health(&self) -> Result<kw_types::api::HealthResponse> {
        self.get_typed("/health").await
    }

    pub async fn info(&self) -> Result<kw_types::api::InfoResponse> {
        self.get_typed("/info").await
    }

    pub async fn shutdown(&self) -> Result<Option<Value>> {
        self.post("/shutdown", json!({})).await
    }

    // ── Auth ──────────────────────────────────────────────────────────

    pub async fn login(
        &self,
        username: String,
        password: String,
    ) -> Result<kw_types::api::LoginResponse> {
        self.post_typed(
            "/auth/login",
            kw_types::api::LoginRequest { username, password },
        )
        .await
    }

    pub async fn logout(&self) -> Result<()> {
        self.post("/auth/logout", json!({})).await?;
        Ok(())
    }

    // ── Screenshot ────────────────────────────────────────────────────

    pub async fn screenshot(&self) -> Result<kw_types::api::ScreenshotResponse> {
        self.get_typed("/screenshot").await
    }

    // ── Screen capture commands ───────────────────────────────────────

    pub async fn screen_capture_poll_pause(&self) -> Result<()> {
        self.post("/screen/poll/pause", json!({})).await?;
        Ok(())
    }

    pub async fn screen_capture_poll_resume(&self) -> Result<()> {
        self.post("/screen/poll/resume", json!({})).await?;
        Ok(())
    }

    pub async fn screen_capture_interval(&self, interval_ms: u64) -> Result<()> {
        self.post(
            "/screen/poll/interval",
            json!({ "interval_ms": interval_ms }),
        )
        .await?;
        Ok(())
    }

    // ── Process tracking ──────────────────────────────────────────────

    pub async fn root_pids(&self) -> Result<Vec<u32>> {
        self.get_typed("/root_pids").await
    }

    pub async fn process_tree(&self, root_pid: u32) -> Result<ProcessTree> {
        self.get_typed(&format!("/process/{root_pid}")).await
    }

    pub async fn process_root(&self, root_pid: u32) -> Result<ProcessSnapshot> {
        self.get_typed(&format!("/process/root/{root_pid}")).await
    }

    pub async fn process_children(&self, root_pid: u32) -> Result<Vec<ProcessSnapshot>> {
        self.get_typed(&format!("/process/children/{root_pid}"))
            .await
    }

    pub async fn process_status(&self, root_pid: u32) -> Result<kw_types::process::ProcessStatus> {
        self.get_typed(&format!("/process/status/{root_pid}")).await
    }

    pub async fn process_is_done(&self, root_pid: u32) -> Result<bool> {
        self.get_typed(&format!("/process/is-done/{root_pid}"))
            .await
    }

    pub async fn process_trees(&self) -> Result<Vec<ProcessTree>> {
        self.get_typed("/process/trees").await
    }

    pub async fn top_processes(
        &self,
        sort: kw_types::process::SortKey,
        limit: Option<usize>,
    ) -> Result<Vec<ProcessSnapshot>> {
        self.get_typed_query(
            "/top-processes",
            kw_types::api::TopProcessesParams { sort, limit },
        )
        .await
    }

    pub async fn supported_signals(&self) -> Result<Vec<ProcessSignal>> {
        self.get_typed("/supported-signals").await
    }

    // ── Process commands ──────────────────────────────────────────────

    pub async fn kill_process(&self, pid: u32, signal: ProcessSignal) -> Result<()> {
        self.post(
            &format!("/process/kill/{pid}"),
            kw_types::api::KillProcessRequest { signal },
        )
        .await?;
        Ok(())
    }

    pub async fn kill_process_tree(&self, root_pid: u32) -> Result<Vec<u32>> {
        self.post_typed(&format!("/process/kill-tree/{root_pid}"), json!({}))
            .await
    }

    pub async fn track_pid(&self, pid: u32) -> Result<()> {
        self.post(&format!("/process/track/{pid}"), json!({}))
            .await?;
        Ok(())
    }

    pub async fn untrack_pid(&self, pid: u32) -> Result<()> {
        self.post(&format!("/process/untrack/{pid}"), json!({}))
            .await?;
        Ok(())
    }

    pub async fn process_poll_pause(&self) -> Result<()> {
        self.post("/process/poll/pause", json!({})).await?;
        Ok(())
    }

    pub async fn process_poll_resume(&self) -> Result<()> {
        self.post("/process/poll/resume", json!({})).await?;
        Ok(())
    }

    pub async fn process_poll_interval(&self, interval_ms: u64) -> Result<()> {
        self.post(
            "/process/poll/interval",
            SetPollIntervalRequest { interval_ms },
        )
        .await?;
        Ok(())
    }

    // ── System resources ──────────────────────────────────────────────

    pub async fn system_snapshot(&self) -> Result<resources::SystemSnapshot> {
        self.get_typed("/system").await
    }

    pub async fn cpu_snapshot(&self) -> Result<resources::CpuSnapshot> {
        self.get_typed("/cpu").await
    }

    pub async fn memory_snapshot(&self) -> Result<resources::MemorySnapshot> {
        self.get_typed("/memory").await
    }

    pub async fn disk_snapshots(&self) -> Result<Vec<resources::DiskSnapshot>> {
        self.get_typed("/disks").await
    }

    pub async fn network_snapshots(&self) -> Result<Vec<resources::NetworkSnapshot>> {
        self.get_typed("/networks").await
    }

    pub async fn gpu_snapshots(&self) -> Result<Vec<resources::GpuSnapshot>> {
        self.get_typed("/gpus").await
    }

    pub async fn battery_snapshot(&self) -> Result<resources::BatterySnapshot> {
        self.get_typed("/battery").await
    }

    pub async fn host_info(&self) -> Result<resources::HostInfo> {
        self.get_typed("/host-info").await
    }

    pub async fn temperatures(&self) -> Result<Vec<resources::ThermalSnapshot>> {
        self.get_typed("/temperatures").await
    }

    // ── System resource commands ──────────────────────────────────────

    pub async fn set_thresholds(
        &self,
        cpu_warn: f32,
        memory_warn: f32,
        disk_warn: f32,
        battery_low: f32,
    ) -> Result<()> {
        self.post(
            "/resources/thresholds",
            kw_types::api::SetThresholdsRequest {
                cpu_warn,
                memory_warn,
                disk_warn,
                battery_low,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn set_refresh_mask(
        &self,
        cpu: bool,
        memory: bool,
        disks: bool,
        networks: bool,
        temperatures: bool,
        gpus: bool,
    ) -> Result<()> {
        self.post(
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
        Ok(())
    }

    pub async fn resources_poll_pause(&self) -> Result<()> {
        self.post("/resources/poll/pause", json!({})).await?;
        Ok(())
    }

    pub async fn resources_poll_resume(&self) -> Result<()> {
        self.post("/resources/poll/resume", json!({})).await?;
        Ok(())
    }

    pub async fn resources_poll_interval(&self, interval_ms: u64) -> Result<()> {
        self.post(
            "/resources/poll/interval",
            SetPollIntervalRequest { interval_ms },
        )
        .await?;
        Ok(())
    }

    // ── Systemd ───────────────────────────────────────────────────────

    pub async fn systemd_snapshot(&self) -> Result<kw_types::systemd::SystemdSnapshot> {
        self.get_typed("/systemd").await
    }

    pub async fn unit_snapshot(&self, unit_name: &str) -> Result<UnitSnapshot> {
        self.get_typed(&format!("/unit/{unit_name}")).await
    }

    pub async fn units_by_state(&self, unit_state: &str) -> Result<Vec<UnitSnapshot>> {
        self.get_typed(&format!("/units/{unit_state}")).await
    }

    pub async fn failed_units(&self) -> Result<Vec<UnitSnapshot>> {
        self.get_typed("/failed_units").await
    }

    // ── Systemd commands ──────────────────────────────────────

    pub async fn systemd_poll_pause(&self) -> Result<()> {
        self.post("/systemd/poll/pause", json!({})).await?;
        Ok(())
    }

    pub async fn systemd_poll_resume(&self) -> Result<()> {
        self.post("/systemd/poll/resume", json!({})).await?;
        Ok(())
    }

    pub async fn systemd_poll_interval(&self, interval_ms: u64) -> Result<()> {
        self.post(
            "/systemd/poll/interval",
            SetPollIntervalRequest { interval_ms },
        )
        .await?;
        Ok(())
    }

    // ── Docker ────────────────────────────────────────────────────────

    pub async fn docker_containers(&self) -> Result<Vec<ContainerSnapshot>> {
        self.get_typed("/docker-containers").await
    }

    pub async fn docker_container(&self, id_or_name: &str) -> Result<ContainerSnapshot> {
        self.get_typed(&format!("/container/{id_or_name}")).await
    }

    pub async fn top_containers(
        &self,
        sort: kw_types::docker::DockerSortKey,
        limit: Option<usize>,
    ) -> Result<Vec<ContainerSnapshot>> {
        self.get_typed_query(
            "/top-containers",
            kw_types::api::TopContainersParams { sort, limit },
        )
        .await
    }

    // ── Docker commands ───────────────────────────────────────────────

    pub async fn stop_container(&self, id_or_name: &str, timeout_secs: Option<i32>) -> Result<()> {
        self.post(
            "/docker/stop-container",
            ContainerTimeoutRequest {
                id_or_name: id_or_name.to_string(),
                timeout_secs,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn kill_container(&self, id_or_name: &str, signal: Option<String>) -> Result<()> {
        self.post(
            "/docker/kill-container",
            kw_types::api::KillContainerRequest {
                id_or_name: id_or_name.to_string(),
                signal,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn start_container(&self, id_or_name: &str) -> Result<()> {
        self.post(
            "/docker/start-container",
            ContainerRequest {
                id_or_name: id_or_name.to_string(),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn restart_container(
        &self,
        id_or_name: &str,
        timeout_secs: Option<i32>,
    ) -> Result<()> {
        self.post(
            "/docker/restart-container",
            ContainerTimeoutRequest {
                id_or_name: id_or_name.to_string(),
                timeout_secs,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn pause_container(&self, id_or_name: &str) -> Result<()> {
        self.post(
            "/docker/pause-container",
            ContainerRequest {
                id_or_name: id_or_name.to_string(),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn unpause_container(&self, id_or_name: &str) -> Result<()> {
        self.post(
            "/docker/unpause-container",
            ContainerRequest {
                id_or_name: id_or_name.to_string(),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn docker_poll_pause(&self) -> Result<()> {
        self.post("/docker/poll/pause", json!({})).await?;
        Ok(())
    }

    pub async fn docker_poll_resume(&self) -> Result<()> {
        self.post("/docker/poll/resume", json!({})).await?;
        Ok(())
    }

    pub async fn docker_poll_interval(&self, interval_ms: u64) -> Result<()> {
        self.post(
            "/docker/poll/interval",
            SetPollIntervalRequest { interval_ms },
        )
        .await?;
        Ok(())
    }
}
