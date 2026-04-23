use serde::Serialize;

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub uptime: String,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotImage {
    pub data: String,
    pub mime: String,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    pub screens: Vec<ScreenshotImage>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}
