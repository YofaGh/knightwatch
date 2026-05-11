use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use base64::{Engine as _, engine::general_purpose};

use super::{constants::*, models::*};
use crate::{
    process_tracker::{self, ProcessSnapshot, ProcessStatus, ProcessTree},
    system_monitor,
    utils::now_rfc3339,
};

pub async fn shutdown(
    State(cancel_token): State<tokio_util::sync::CancellationToken>,
) -> &'static str {
    cancel_token.cancel();
    "Shutting down…"
}

pub async fn health() -> Json<HealthResponse> {
    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: now_rfc3339(),
        version: crate::utils::get_version().to_string(),
        uptime: crate::utils::format_uptime(uptime),
    })
}

pub async fn config() -> Json<ConfigResponse> {
    let args = &crate::prelude::get_config().args;
    Json(ConfigResponse {
        blind: args.blind,
        pid: args.pid.clone(),
        top_processes: args.top_processes,
        limit_processes: args.limit_processes,
        telegram_bot: args.telegram,
        system_monitor: args.system_monitor,
    })
}

// ---------------------------------------------------------------------------
// Screenshot endpoints
// ---------------------------------------------------------------------------

pub async fn screenshot() -> Result<Json<ScreenshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    let images = crate::screen_capture::get_screenshots().await;
    if images.is_empty() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                message: "No screens found".to_string(),
            }),
        ));
    }
    let screens: Vec<ScreenshotImage> = images
        .into_iter()
        .map(|s| ScreenshotImage {
            data: general_purpose::STANDARD.encode(&s.image),
            mime: "image/png".to_string(),
            monitor_name: s.monitor_name,
            monitor_id: s.monitor_id,
            width: s.width,
            height: s.height,
            timestamp: s.timestamp,
        })
        .collect();
    let count = screens.len();
    Ok(Json(ScreenshotResponse { screens, count }))
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
pub async fn process_tree(Path(root_pid): Path<u32>) -> Json<ProcessTree> {
    let (root, children, work_done) = tokio::join!(
        process_tracker::get_root(root_pid),
        process_tracker::get_children(root_pid),
        process_tracker::is_work_done(root_pid),
    );

    let child_count = children.len();
    Json(ProcessTree {
        root,
        children,
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}

/// `GET /process/root/{pid}`
///
/// Returns only the root process snapshot of a given root pid, or 404 if it has exited.
pub async fn process_root(
    Path(root_pid): Path<u32>,
) -> Result<Json<ProcessSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    match process_tracker::get_root(root_pid).await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "Root process is not running".to_string(),
            }),
        )),
    }
}

/// `GET /process/children/{pid}`
///
/// Returns snapshots of all currently live child processes of a given root pid.
pub async fn process_children(Path(root_pid): Path<u32>) -> Json<Vec<ProcessSnapshot>> {
    let children = process_tracker::get_children(root_pid).await;
    Json(children)
}

/// `GET /process/status/{pid}`
///
/// Lightweight summary — cheap to poll frequently.
/// Returns root alive/dead, child count, and the `work_done` flag of a given root pid.
pub async fn process_status(Path(root_pid): Path<u32>) -> Json<ProcessStatus> {
    let (root_snap, child_count, work_done) = tokio::join!(
        process_tracker::get_root(root_pid),
        async { process_tracker::get_children(root_pid).await.len() },
        process_tracker::is_work_done(root_pid),
    );

    Json(ProcessStatus {
        root_alive: root_snap.is_some(),
        root_pid: root_snap.as_ref().map(|s| s.pid),
        root_name: root_snap.map(|s| s.name),
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}

/// `GET /top-processes?limit=10&sort=cpu`
///
/// Returns the top N processes sorted by the given key.
///
/// # Query Parameters
/// - `limit`: Number of processes to return (default: 0 = all)
/// - `sort`: Sort key, either `cpu` or `mem` (default: `cpu`)
///
/// # Errors
/// - `400 Bad Request` if `sort` is not a valid sort key
pub async fn top_processes(
    Query(params): Query<TopProcessesParams>,
) -> Result<Json<Vec<ProcessSnapshot>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(0);
    let sort_key = process_tracker::SortKey::try_from(params.sort).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                success: false,
                message: e,
            }),
        )
    })?;
    let top_processes = process_tracker::get_top_processes(sort_key, limit).await;
    Ok(Json(top_processes))
}

// ---------------------------------------------------------------------------
// System Monitor endpoints
// ---------------------------------------------------------------------------

/// `GET /system`
///
/// Returns the current System Snapshot.
pub async fn system_snapshot()
-> Result<Json<system_monitor::SystemSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    match system_monitor::get_snapshot().await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "No System Snapshot was found".to_string(),
            }),
        )),
    }
}

/// `GET /cpu`
///
/// Returns the current Cpu Snapshot.
pub async fn cpu_snapshot()
-> Result<Json<system_monitor::CpuSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    match system_monitor::get_cpu().await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "No Cpu Snapshot was found".to_string(),
            }),
        )),
    }
}

/// `GET /memory`
///
/// Returns the current Memory Snapshot.
pub async fn memory_snapshot()
-> Result<Json<system_monitor::MemorySnapshot>, (StatusCode, Json<ErrorResponse>)> {
    match system_monitor::get_memory().await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "No Memory Snapshot was found".to_string(),
            }),
        )),
    }
}

/// `GET /disks`
///
/// Returns the Disks Snapshots.
pub async fn disks_snapshots() -> Json<Vec<system_monitor::DiskSnapshot>> {
    Json(system_monitor::get_disks().await)
}

/// `GET /networks`
///
/// Returns the Networks Snapshots.
pub async fn networks_snapshot() -> Json<Vec<system_monitor::NetworkSnapshot>> {
    Json(system_monitor::get_networks().await)
}

/// `GET /gpus`
///
/// Returns the Gpus Snapshots.
pub async fn gpus_snapshots() -> Json<Vec<system_monitor::GpuSnapshot>> {
    Json(system_monitor::get_gpus().await)
}

/// `GET /battery`
///
/// Returns the current Battery Snapshot.
pub async fn battery_snapshot()
-> Result<Json<system_monitor::BatterySnapshot>, (StatusCode, Json<ErrorResponse>)> {
    match system_monitor::get_battery().await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "No battery Snapshot was found".to_string(),
            }),
        )),
    }
}

/// `GET /host-info`
///
/// Returns the current Host Info Snapshot.
pub async fn host_info_snapshot()
-> Result<Json<system_monitor::HostInfo>, (StatusCode, Json<ErrorResponse>)> {
    match system_monitor::get_host_info().await {
        Some(snap) => Ok(Json(snap)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "No host info was found".to_string(),
            }),
        )),
    }
}

/// `GET /temperatures`
///
/// Returns the Temperatures Snapshots.
pub async fn temperatures_snapshots() -> Json<Vec<system_monitor::ThermalSnapshot>> {
    Json(system_monitor::get_temperatures().await)
}
