use axum::{
    Extension,
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
};
use std::time::Duration;

use kw_types::{
    api::{
        ContainerRequest, ContainerTimeoutRequest, KillContainerRequest, SetPollIntervalRequest,
        TopContainersParams,
    },
    polling::PollStatus,
};

use super::super::utils::{internal_server_error, not_found};
use crate::{
    config::DisplayUser,
    docker_tracker::{self, ContainerSnapshot},
};

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
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<ContainerTimeoutRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::stop_container(user, body.id_or_name, body.timeout_secs)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/kill-container`
///
/// Kills a container by ID or name, with a specified signal (e.g. "SIGKILL", "SIGTERM").
pub async fn kill_container(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<KillContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::kill_container(user, body.id_or_name, body.signal)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/start-container`
///
/// Starts a container by ID or name.
pub async fn start_container(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::start_container(user, body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/restart-container`
///
/// Restarts a container by ID or name.
pub async fn restart_container(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<ContainerTimeoutRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::restart_container(user, body.id_or_name, body.timeout_secs)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/pause-container`
///
/// Pauses a container by ID or name.
pub async fn pause_container(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::pause_container(user, body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/unpause-container`
///
/// Unpauses a container by ID or name.
pub async fn unpause_container(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<ContainerRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::unpause_container(user, body.id_or_name)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/pause`
///
/// Pauses the docker tracker polling loop.
pub async fn docker_pause_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::pause_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/resume`
///
/// Resumes the docker tracker polling loop.
pub async fn docker_resume_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::resume_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /docker/poll/interval`
///
/// Sets the interval of the docker tracker polling loop in milliseconds.
pub async fn docker_set_poll_interval(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    docker_tracker::set_poll_interval(user, Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}
