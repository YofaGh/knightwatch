use axum::{
    http::StatusCode,
    response::{Html, Json},
};
use base64::{Engine as _, engine::general_purpose};
// use screenshots::image::ImageFormat;
use std::time::SystemTime;

use super::{constants::*, models::*, utils::*};
use crate::core::process_tracker;

// ---------------------------------------------------------------------------
// Screenshot endpoints
// ---------------------------------------------------------------------------

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

pub async fn screenshot() -> Result<Json<ScreenshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    let images = crate::core::screenshot_all_screens().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { success: false, message: format!("Failed to take screenshot: {err}") }),
        )
    })?;
    if images.is_empty() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            success: false,
            message: "No screens found".to_string(),
        })));
    }
    let screens: Vec<ScreenshotImage> = images
        .into_iter()
        .map(|png_bytes| ScreenshotImage {
            data: general_purpose::STANDARD.encode(&png_bytes),
            mime: "image/png".to_string(),
        })
        .collect();
    let count = screens.len();
    Ok(Json(ScreenshotResponse { screens, count }))
}

pub async fn screenshot_view() -> Html<&'static str> {
    Html(VIEW_HTML)
}

// ---------------------------------------------------------------------------
// Process tracking endpoints
// ---------------------------------------------------------------------------

/// `GET /process`
///
/// Returns the full process tree: root + all live descendants, plus a
/// `work_done` flag. Useful for dashboards or external orchestration.
pub async fn process_tree() -> Json<ProcessTreeResponse> {
    let (root_snap, children_snaps, work_done) = tokio::join!(
        process_tracker::get_root(),
        process_tracker::get_children(),
        process_tracker::is_work_done(),
    );

    let child_count = children_snaps.len();
    Json(ProcessTreeResponse {
        root: root_snap.map(snapshot_to_response),
        children: children_snaps
            .into_iter()
            .map(snapshot_to_response)
            .collect(),
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}

/// `GET /process/root`
///
/// Returns only the root process snapshot, or 404 if it has exited.
pub async fn process_root()
-> Result<Json<ProcessSnapshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    match process_tracker::get_root().await {
        Some(snap) => Ok(Json(snapshot_to_response(snap))),
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
pub async fn process_children() -> Json<Vec<ProcessSnapshotResponse>> {
    let children = process_tracker::get_children().await;
    Json(children.into_iter().map(snapshot_to_response).collect())
}

/// `GET /process/status`
///
/// Lightweight summary — cheap to poll frequently.
/// Returns root alive/dead, child count, and the `work_done` flag.
pub async fn process_status() -> Json<ProcessStatusResponse> {
    let (root_snap, child_count, work_done) = tokio::join!(
        process_tracker::get_root(),
        async { process_tracker::get_children().await.len() },
        process_tracker::is_work_done(),
    );

    Json(ProcessStatusResponse {
        root_alive: root_snap.is_some(),
        root_pid: root_snap.as_ref().map(|s| s.pid),
        root_name: root_snap.map(|s| s.name),
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}
