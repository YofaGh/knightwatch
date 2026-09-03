use axum::{
    Extension,
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
};
use std::time::Duration;

use kw_types::{
    api::{KillProcessRequest, SetPollIntervalRequest, TopProcessesParams},
    polling::PollStatus,
};

use super::super::utils::{bad_request, internal_server_error, not_found};
use crate::{
    config::DisplayUser,
    process_tracker::{self, ProcessSignal, ProcessSnapshot, ProcessTree},
};

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
    Extension(user): Extension<DisplayUser>,
    Path(pid): Path<u32>,
    body: Json<KillProcessRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !body.signal.is_supported() {
        return Err(bad_request(
            crate::errors::Error::unsupported_signal(body.signal).to_string(),
        ));
    }
    process_tracker::kill_process(user, pid, body.signal)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/kill-tree/{root_pid}`
pub async fn kill_tree(
    Extension(user): Extension<DisplayUser>,
    Path(root_pid): Path<u32>,
) -> Result<Json<Vec<u32>>, (StatusCode, String)> {
    process_tracker::kill_tree(user, root_pid)
        .await
        .map(Json)
        .map_err(|error| internal_server_error(&error))
}

/// `POST /process/track/{pid}`
pub async fn track_pid(
    Extension(user): Extension<DisplayUser>,
    Path(pid): Path<u32>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::track_pid(user, pid)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/untrack/{pid}`
pub async fn untrack_pid(
    Extension(user): Extension<DisplayUser>,
    Path(pid): Path<u32>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::untrack_pid(user, pid)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/pause`
pub async fn process_tracker_pause_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::pause_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/resume`
pub async fn process_tracker_resume_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::resume_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /process/poll/interval`
pub async fn process_tracker_set_poll_interval(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    process_tracker::set_poll_interval(user, Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}
