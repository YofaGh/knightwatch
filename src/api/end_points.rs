use axum::{
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{Html, Json, Response},
};
use base64::{Engine as _, engine::general_purpose};
use std::time::SystemTime;

use super::{constants::*, models::*};
use crate::{
    core::utils::now_rfc3339,
    process_tracker::{
        self,
        structs::{ProcessInfo, ProcessStatus, ProcessTree},
        utils::snapshot_to_response,
    },
};

pub async fn shutdown(
    State(cancel_token): State<tokio_util::sync::CancellationToken>,
) -> &'static str {
    cancel_token.cancel();
    "Shutting down…"
}

pub async fn health() -> Json<HealthResponse> {
    let start_time = SystemTime::UNIX_EPOCH;
    let uptime = SystemTime::now()
        .duration_since(start_time)
        .unwrap_or_default()
        .as_secs();
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: now_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: format!("{uptime}s"),
    })
}

// ---------------------------------------------------------------------------
// Screenshot endpoints
// ---------------------------------------------------------------------------

pub async fn screenshot() -> Result<Json<ScreenshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    let images = crate::screen_capture::screenshot_all_screens().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                message: format!("Failed to take screenshot: {err}"),
            }),
        )
    })?;
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

pub async fn view() -> Html<&'static str> {
    Html(VIEW_HTML)
}

pub async fn view_css() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css")
        .body(Body::from(VIEW_CSS))
        .unwrap()
}

pub async fn view_js() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript")
        .body(Body::from(VIEW_JS))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Process tracking endpoints
// ---------------------------------------------------------------------------

/// `GET /process`
///
/// Returns the full process tree: root + all live descendants, plus a
/// `work_done` flag. Useful for dashboards or external orchestration.
pub async fn process_tree() -> Json<ProcessTree> {
    let (root_snap, children_snaps, work_done) = tokio::join!(
        process_tracker::get_root(),
        process_tracker::get_children(),
        process_tracker::is_work_done(),
    );

    let child_count = children_snaps.len();
    Json(ProcessTree {
        root: root_snap.map(|s| snapshot_to_response(&s)),
        children: children_snaps
            .into_iter()
            .map(|s| snapshot_to_response(&s))
            .collect(),
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}

/// `GET /process/root`
///
/// Returns only the root process snapshot, or 404 if it has exited.
pub async fn process_root() -> Result<Json<ProcessInfo>, (StatusCode, Json<ErrorResponse>)> {
    match process_tracker::get_root().await {
        Some(snap) => Ok(Json(snapshot_to_response(&snap))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                message: "Root process is not running".to_string(),
            }),
        )),
    }
}

/// `GET /process/children`
///
/// Returns snapshots of all currently live child processes.
pub async fn process_children() -> Json<Vec<ProcessInfo>> {
    let children = process_tracker::get_children().await;
    Json(
        children
            .into_iter()
            .map(|s| snapshot_to_response(&s))
            .collect(),
    )
}

/// `GET /process/status`
///
/// Lightweight summary — cheap to poll frequently.
/// Returns root alive/dead, child count, and the `work_done` flag.
pub async fn process_status() -> Json<ProcessStatus> {
    let (root_snap, child_count, work_done) = tokio::join!(
        process_tracker::get_root(),
        async { process_tracker::get_children().await.len() },
        process_tracker::is_work_done(),
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
