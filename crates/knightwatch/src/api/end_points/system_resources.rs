use axum::{http::StatusCode, response::Json};
use std::time::Duration;

use kw_types::{
    api::{SetPollIntervalRequest, SetRefreshMaskRequest, SetThresholdsRequest},
    polling::PollStatus,
};

use super::super::utils::{internal_server_error, not_found};
use crate::system_resources::{self, RefreshMask, Thresholds};

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
