use axum::{Extension, extract::Path, http::StatusCode, response::Json};
use std::time::Duration;

use kw_types::{api::SetPollIntervalRequest, polling::PollStatus};

use super::super::utils::{internal_server_error, not_found};
use crate::{
    config::DisplayUser,
    systemd::{self, UnitSnapshot},
};

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
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<kw_types::api::ControlUnitParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::control_unit(user, body.unit_name, body.action)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/pause`
pub async fn systemd_pause_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::pause_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/resume`
pub async fn systemd_resume_poll(
    Extension(user): Extension<DisplayUser>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::resume_poll(user)
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}

/// `POST /systemd/poll/interval`
pub async fn systemd_set_poll_interval(
    Extension(user): Extension<DisplayUser>,
    Json(body): Json<SetPollIntervalRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    systemd::set_poll_interval(user, Duration::from_millis(body.interval_ms))
        .await
        .map_err(|error| internal_server_error(&error))?;
    Ok(StatusCode::OK)
}
