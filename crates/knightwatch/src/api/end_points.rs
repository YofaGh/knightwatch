use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::{TypedHeader, headers};
use std::time::Duration;

use kw_types::{
    api::{
        ContainerRequest, ContainerTimeoutRequest, HealthResponse, InfoResponse,
        KillContainerRequest, KillProcessRequest, LoginRequest, LoginResponse, ScreenshotImage,
        ScreenshotResponse, SetPollIntervalRequest, SetRefreshMaskRequest, SetThresholdsRequest,
        TopContainersParams, TopProcessesParams,
    },
    polling::PollStatus,
};

use super::utils::{bad_request, internal_server_error, not_found};
use crate::{
    docker_tracker::{self, ContainerSnapshot},
    process_tracker::{self, ProcessSignal, ProcessSnapshot, ProcessTree},
    screen_capture,
    system_resources::{self, RefreshMask, Thresholds},
    systemd::{self, UnitSnapshot},
};

pub async fn shutdown(
    State(cancel_token): State<tokio_util::sync::CancellationToken>,
) -> &'static str {
    cancel_token.cancel();
    "Shutting down…"
}

pub async fn health() -> Json<HealthResponse> {
    let uptime = super::handlers::START_TIME
        .get()
        .map_or(0, |t| t.elapsed().as_secs());
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: crate::utils::now_rfc3339(),
        version: crate::utils::get_version().to_string(),
        uptime: kw_utils::format_time(uptime),
    })
}

pub async fn info() -> Json<InfoResponse> {
    let args = &crate::prelude::get_config().args;
    Json(InfoResponse {
        auth_enabled: args.enable_auth,
        shutdown_enabled: args.enable_shutdown,
        blind: args.is_blind(),
        pid: process_tracker::get_root_pids().await,
        top_processes: args.top_processes,
        limit_processes: args.limit_processes,
        telegram_bot: args.telegram,
        system_resources: args.system_resources,
        systemd: args.systemd,
        docker: args.docker,
        allow_process_commands: args.allow_process_commands,
        allow_screen_commands: args.allow_screen_commands,
        allow_system_resources_commands: args.allow_system_resources_commands,
        allow_systemd_commands: args.allow_systemd_commands,
        allow_docker_commands: args.allow_docker_commands,
    })
}

pub async fn login(Json(body): Json<LoginRequest>) -> Result<Json<LoginResponse>, StatusCode> {
    let Some(users) = crate::config::get_users().filter(|u| !u.users.is_empty()) else {
        return Err(StatusCode::NOT_FOUND);
    };
    match users.verify_password(&body.username, &body.password) {
        Ok(true) => {}
        _ => return Err(StatusCode::UNAUTHORIZED),
    }
    let token = uuid::Uuid::new_v4().to_string();
    let session = super::session::Session {
        username: body.username,
        token: token.clone(),
    };
    super::session::get_sessions()
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .insert(session);
    Ok(Json(LoginResponse { token }))
}

pub async fn logout(
    TypedHeader(auth): TypedHeader<headers::Authorization<headers::authorization::Bearer>>,
) -> StatusCode {
    super::session::get_sessions().write().map_or(
        StatusCode::INTERNAL_SERVER_ERROR,
        |mut sessions| {
            sessions.remove_by_token(auth.token());
            StatusCode::OK
        },
    )
}

// ---------------------------------------------------------------------------
// Screenshot endpoints
// ---------------------------------------------------------------------------

pub async fn screenshot() -> Result<Json<ScreenshotResponse>, (StatusCode, String)> {
    let images = screen_capture::get_screenshots().await;
    if images.is_empty() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "No screens found".to_string(),
        ));
    }
    let screens: Vec<ScreenshotImage> = images.into_iter().map(Into::into).collect();
    let count = screens.len();
    Ok(Json(ScreenshotResponse { screens, count }))
}

/// `GET /screen/poll/status`
pub async fn screen_capture_poll_status() -> Result<Json<PollStatus>, (StatusCode, String)> {
    screen_capture::get_poll_status()
        .await
        .map(Json)
        .ok_or_else(|| not_found("Screen capture is not running".to_string()))
}

// ---------------------------------------------------------------------------
// Screen capture command endpoints (requires --allow-screen-commands)
// ---------------------------------------------------------------------------

/// `POST /screen/poll/pause`
#[cfg(feature = "screenshot")]
pub async fn screen_capture_pause_poll() -> Result<StatusCode, (StatusCode, String)> {
    screen_capture::pause_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /screen/poll/resume`
#[cfg(feature = "screenshot")]
pub async fn screen_capture_resume_poll() -> Result<StatusCode, (StatusCode, String)> {
    screen_capture::resume_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /screen/poll/interval`
#[cfg(feature = "screenshot")]
pub async fn screen_capture_set_poll_interval(
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    screen_capture::set_poll_interval(Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Process tracking endpoints
// ---------------------------------------------------------------------------

/// `GET /root_pids`
///
/// Returns a list of currently tracked root PIDs.
pub async fn root_pids() -> Json<Vec<u32>> {
    Json(process_tracker::get_root_pids().await)
}

/// `GET /process/{pid}`
///
/// Returns the full process tree of a given root pid: root + all live descendants, plus a
/// `work_done` flag. Useful for dashboards or external orchestration.
/// Returns 404 if the root process has exited and is no longer tracked.
pub async fn process_tree(
    Path(root_pid): Path<u32>,
) -> Result<Json<ProcessTree>, (StatusCode, String)> {
    process_tracker::get_process_tree(root_pid)
        .await
        .map(Json)
        .ok_or_else(|| not_found("Root process is not running".to_string()))
}

/// `GET /process/trees`
///
/// Returns all process trees currently being tracked.
pub async fn process_trees() -> Json<Vec<ProcessTree>> {
    Json(process_tracker::get_all_process_trees().await)
}

/// `GET /process/root/{pid}`
///
/// Returns only the root process snapshot of a given root pid, or 404 if it has exited.
pub async fn process_root(
    Path(root_pid): Path<u32>,
) -> Result<Json<ProcessSnapshot>, (StatusCode, String)> {
    process_tracker::get_root(root_pid)
        .await
        .map(Json)
        .ok_or_else(|| not_found("Root process is not running".to_string()))
}

/// `GET /process/children/{pid}`
///
/// Returns snapshots of all currently live child processes of a given root pid.
pub async fn process_children(Path(root_pid): Path<u32>) -> Json<Vec<ProcessSnapshot>> {
    Json(process_tracker::get_children(root_pid).await)
}

/// `GET /process/status/{pid}`
///
/// Lightweight summary — cheap to poll frequently.
/// Returns root alive/dead, child count, and the `work_done` flag of a given root pid.
/// Returns 404 if the root process has exited and is no longer tracked.
pub async fn process_status(
    Path(root_pid): Path<u32>,
) -> Result<Json<process_tracker::ProcessStatus>, (StatusCode, String)> {
    process_tracker::get_process_status(root_pid)
        .await
        .map(Json)
        .ok_or_else(|| not_found("Root process is not running".to_string()))
}

/// `GET /process/is-done/{pid}`
///
/// Returns whether the work is done (all children have exited) for a given root pid.
/// Returns 404 if the root process has exited and is no longer tracked.
pub async fn is_process_done(
    Path(root_pid): Path<u32>,
) -> Result<Json<bool>, (StatusCode, String)> {
    process_tracker::is_process_done(root_pid)
        .await
        .map(Json)
        .ok_or_else(|| not_found("Root process is not running".to_string()))
}

/// `GET /top-processes?limit=10&sort=cpu`
///
/// Returns the top N processes sorted by the given key.
///
/// # Query Parameters
/// - `limit`: Number of processes to return (default: 0 = all)
/// - `sort`: Sort key, either `cpu`, `memory` or `disk`
///
/// # Errors
/// - `400 Bad Request` if `sort` is not a valid sort key
pub async fn top_processes(
    Query(params): Query<TopProcessesParams>,
) -> Result<Json<Vec<ProcessSnapshot>>, (StatusCode, String)> {
    Ok(Json(
        process_tracker::get_top_processes(params.sort, params.limit.unwrap_or(0)).await,
    ))
}

/// `GET /supported-signals`
///
/// Returns a list of supported signal based on current platform.
pub async fn supported_signals() -> Json<Vec<ProcessSignal>> {
    Json(ProcessSignal::get_supported_signals())
}

/// `GET /process/poll/status`
pub async fn process_tracker_poll_status() -> Result<Json<PollStatus>, (StatusCode, String)> {
    process_tracker::get_poll_status()
        .await
        .map(Json)
        .ok_or_else(|| not_found("Process tracker is not running".to_string()))
}

// ---------------------------------------------------------------------------
// Process command endpoints (requires --allow-process-commands)
// ---------------------------------------------------------------------------

/// `POST /process/kill/{pid}`
pub async fn kill_process(
    Path(pid): Path<u32>,
    body: Json<KillProcessRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !body.signal.is_supported() {
        return Err(bad_request(
            crate::errors::Error::unsupported_signal(body.signal).to_string(),
        ));
    }
    process_tracker::kill_process(pid, body.signal)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/kill-tree/{root_pid}`
pub async fn kill_tree(Path(root_pid): Path<u32>) -> Result<Json<Vec<u32>>, (StatusCode, String)> {
    process_tracker::kill_tree(root_pid)
        .await
        .map(Json)
        .map_err(|error| internal_server_error(&error))
}

/// `POST /process/track/{pid}`
pub async fn track_pid(Path(pid): Path<u32>) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::track_pid(pid)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/untrack/{pid}`
pub async fn untrack_pid(Path(pid): Path<u32>) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::untrack_pid(pid)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/pause`
pub async fn process_tracker_pause_poll() -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::pause_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/resume`
pub async fn process_tracker_resume_poll() -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::resume_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/interval`
pub async fn process_tracker_set_poll_interval(
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::set_poll_interval(Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// System Resources endpoints
// ---------------------------------------------------------------------------

/// `GET /system`
///
/// Returns the current System Snapshot.
pub async fn system_snapshot()
-> Result<Json<system_resources::SystemSnapshot>, (StatusCode, String)> {
    system_resources::get_snapshot()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No System Snapshot was found".to_string()))
}

/// `GET /cpu`
///
/// Returns the current Cpu Snapshot.
pub async fn cpu_snapshot() -> Result<Json<system_resources::CpuSnapshot>, (StatusCode, String)> {
    system_resources::get_cpu()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No Cpu Snapshot was found".to_string()))
}

/// `GET /memory`
///
/// Returns the current Memory Snapshot.
pub async fn memory_snapshot()
-> Result<Json<system_resources::MemorySnapshot>, (StatusCode, String)> {
    system_resources::get_memory()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No Memory Snapshot was found".to_string()))
}

/// `GET /disks`
///
/// Returns the Disks Snapshots.
pub async fn disks_snapshots() -> Json<Vec<system_resources::DiskSnapshot>> {
    Json(system_resources::get_disks().await)
}

/// `GET /networks`
///
/// Returns the Networks Snapshots.
pub async fn networks_snapshot() -> Json<Vec<system_resources::NetworkSnapshot>> {
    Json(system_resources::get_networks().await)
}

/// `GET /gpus`
///
/// Returns the Gpus Snapshots.
pub async fn gpus_snapshots() -> Json<Vec<system_resources::GpuSnapshot>> {
    Json(system_resources::get_gpus().await)
}

/// `GET /battery`
///
/// Returns the current Battery Snapshot.
pub async fn battery_snapshot()
-> Result<Json<system_resources::BatterySnapshot>, (StatusCode, String)> {
    system_resources::get_battery()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No battery Snapshot was found".to_string()))
}

/// `GET /host-info`
///
/// Returns the current Host Info Snapshot.
pub async fn host_info_snapshot() -> Result<Json<system_resources::HostInfo>, (StatusCode, String)>
{
    system_resources::get_host_info()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No host info was found".to_string()))
}

/// `GET /temperatures`
///
/// Returns the Temperatures Snapshots.
pub async fn temperatures_snapshots() -> Json<Vec<system_resources::ThermalSnapshot>> {
    Json(system_resources::get_temperatures().await)
}

/// `GET /alarms`
///
/// Returns the Alarm Snapshots.
pub async fn alarms_snapshot() -> Result<Json<system_resources::AlarmSnapshot>, (StatusCode, String)>
{
    system_resources::get_alarms()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No alarms Snapshot was found".to_string()))
}

/// `GET /resources/poll/status`
pub async fn system_resources_poll_status() -> Result<Json<PollStatus>, (StatusCode, String)> {
    system_resources::get_poll_status()
        .await
        .map(Json)
        .ok_or_else(|| not_found("System resources is not running".to_string()))
}

/// `GET /resources/thresholds`
pub async fn thresholds() -> Result<Json<Thresholds>, (StatusCode, String)> {
    system_resources::get_thresholds()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No thresholds were found".to_string()))
}

/// `GET /resources/refresh-mask`
pub async fn refresh_mask() -> Result<Json<RefreshMask>, (StatusCode, String)> {
    system_resources::get_refresh_mask()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No refresh mask were found".to_string()))
}

// ---------------------------------------------------------------------------
// System Resources command endpoints (requires --allow-system-resources-commands)
// ---------------------------------------------------------------------------

/// `POST /resources/thresholds`
///
/// Updates the alert thresholds for CPU, memory, disk, and battery.
pub async fn resources_set_thresholds(
    Json(body): Json<SetThresholdsRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    system_resources::set_thresholds(Thresholds {
        cpu_warn: body.cpu_warn,
        memory_warn: body.memory_warn,
        disk_warn: body.disk_warn,
        battery_low: body.battery_low,
    })
    .await
    .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /resources/refresh-mask`
///
/// Updates the refresh mask that controls which subsystems are collected on each tick.
pub async fn resources_set_refresh_mask(
    Json(body): Json<SetRefreshMaskRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    system_resources::set_refresh_mask(RefreshMask {
        cpu: body.cpu,
        memory: body.memory,
        disks: body.disks,
        networks: body.networks,
        temperatures: body.temperatures,
        gpus: body.gpus,
    })
    .await
    .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /resources/poll/pause`
pub async fn resources_pause_poll() -> Result<StatusCode, (StatusCode, String)> {
    system_resources::pause_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /resources/poll/resume`
pub async fn resources_resume_poll() -> Result<StatusCode, (StatusCode, String)> {
    system_resources::resume_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /resources/poll/interval`
pub async fn resources_set_poll_interval(
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    system_resources::set_poll_interval(Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Systemd endpoints
// ---------------------------------------------------------------------------

/// `GET /systemd`
///
/// Returns the current Systemd Snapshot.
pub async fn systemd_snapshot() -> Result<Json<systemd::SystemdSnapshot>, (StatusCode, String)> {
    systemd::get_snapshot()
        .await
        .map(Json)
        .ok_or_else(|| not_found("No Systemd Snapshot was found".to_string()))
}

/// `GET /unit/{unit_name}`
///
/// Returns Unit Snapshot by name.
pub async fn unit_snapshot(
    Path(unit_name): Path<String>,
) -> Result<Json<UnitSnapshot>, (StatusCode, String)> {
    systemd::get_unit(unit_name)
        .await
        .map(Json)
        .ok_or_else(|| not_found("No Unit Snapshot was found".to_string()))
}

/// `GET /units/{unit_state}`
///
/// Returns units by active state.
pub async fn units_by_active_state(Path(unit_state): Path<String>) -> Json<Vec<UnitSnapshot>> {
    Json(systemd::get_units_by_active_state(unit_state.as_str().into()).await)
}

/// `GET /failed_units`
///
/// Returns failedunits.
pub async fn failed_units() -> Json<Vec<UnitSnapshot>> {
    Json(systemd::get_failed_units().await)
}

/// `GET /systemd/poll/status`
pub async fn systemd_poll_status() -> Result<Json<PollStatus>, (StatusCode, String)> {
    systemd::get_poll_status()
        .await
        .map(Json)
        .ok_or_else(|| not_found("Systemd is not running".to_string()))
}

// ---------------------------------------------------------------------------
// Systemd command endpoints (requires --allow-systemd-commands)
// ---------------------------------------------------------------------------

/// `POST /systemd/control_unit`
///
/// Returns the result of control action.
pub async fn control_unit(
    Json(body): Json<kw_types::api::ControlUnitParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::control_unit(body.unit_name, body.action)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/pause`
pub async fn systemd_pause_poll() -> Result<StatusCode, (StatusCode, String)> {
    systemd::pause_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/resume`
pub async fn systemd_resume_poll() -> Result<StatusCode, (StatusCode, String)> {
    systemd::resume_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/interval`
pub async fn systemd_set_poll_interval(
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::set_poll_interval(Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// docker endpoints
// ---------------------------------------------------------------------------

/// `GET /docker-containers`
///
pub async fn list_docker_containers() -> Json<Vec<ContainerSnapshot>> {
    Json(docker_tracker::list_containers().await)
}

/// `GET /container/{id_or_name}`
///
/// Returns a container snapshot by ID or name, or 404 if not found.
pub async fn get_docker_container(
    Path(id_or_name): Path<String>,
) -> Result<Json<ContainerSnapshot>, (StatusCode, String)> {
    docker_tracker::get_container(id_or_name)
        .await
        .map(Json)
        .ok_or_else(|| not_found("No docker container was found".to_string()))
}

/// `GET /top-containers?sort=cpu&limit=10`
///
/// Returns the top N containers sorted by the given key.
pub async fn top_docker_containers(
    Query(params): Query<TopContainersParams>,
) -> Result<Json<Vec<ContainerSnapshot>>, (StatusCode, String)> {
    Ok(Json(
        docker_tracker::get_top_containers(params.sort, params.limit.unwrap_or(0)).await,
    ))
}

/// `GET /docker/poll/status`
pub async fn docker_tracker_poll_status() -> Result<Json<PollStatus>, (StatusCode, String)> {
    docker_tracker::get_poll_status()
        .await
        .map(Json)
        .ok_or_else(|| not_found("Docker tracker is not running".to_string()))
}

// ---------------------------------------------------------------------------
// Docker command endpoints (requires --allow-docker-commands)
// ---------------------------------------------------------------------------

/// `POST /docker/stop-container`
///
/// Stops a container by ID or name, with an optional timeout in seconds before killing it.
pub async fn stop_container(
    Json(body): Json<ContainerTimeoutRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::stop_container(body.id_or_name, body.timeout_secs)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/kill-container`
///
/// Kills a container by ID or name, with a specified signal (e.g. "SIGKILL", "SIGTERM").
pub async fn kill_container(
    Json(body): Json<KillContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::kill_container(body.id_or_name, body.signal)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/start-container`
///
/// Starts a container by ID or name.
pub async fn start_container(
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::start_container(body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/restart-container`
///
/// Restarts a container by ID or name.
pub async fn restart_container(
    Json(body): Json<ContainerTimeoutRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::restart_container(body.id_or_name, body.timeout_secs)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/pause-container`
///
/// Pauses a container by ID or name.
pub async fn pause_container(
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::pause_container(body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/unpause-container`
///
/// Unpauses a container by ID or name.
pub async fn unpause_container(
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::unpause_container(body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/pause`
///
/// Pauses the docker tracker polling loop.
pub async fn docker_pause_poll() -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::pause_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/resume`
///
/// Resumes the docker tracker polling loop.
pub async fn docker_resume_poll() -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::resume_poll()
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/interval`
///
/// Sets the interval of the docker tracker polling loop in milliseconds.
pub async fn docker_set_poll_interval(
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::set_poll_interval(Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}
