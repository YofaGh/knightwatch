use axum::http::StatusCode;

use kw_types::api::ScreenshotImage;

pub const fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

pub const fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}

pub fn internal_server_error(error: &crate::errors::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

pub fn screenshot_to_image(screenshot: kw_types::screen::Screenshot) -> ScreenshotImage {
    ScreenshotImage {
        data: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &screenshot.image,
        ),
        mime: "image/png".to_string(),
        monitor_name: screenshot.monitor_name,
        monitor_id: screenshot.monitor_id,
        width: screenshot.width,
        height: screenshot.height,
        timestamp: crate::utils::now_rfc3339(),
    }
}
