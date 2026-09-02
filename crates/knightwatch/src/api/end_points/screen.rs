use axum::{http::StatusCode, response::Json};
use std::time::Duration;

use kw_types::{
    api::{ScreenshotImage, ScreenshotResponse, SetPollIntervalRequest},
    polling::PollStatus,
};

use super::super::utils::{internal_server_error, not_found};
use crate::screen_capture;

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
