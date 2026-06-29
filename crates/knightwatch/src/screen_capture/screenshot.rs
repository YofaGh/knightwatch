#[derive(Debug, Clone, serde::Serialize)]
pub struct Screenshot {
    pub image: Vec<u8>,
    pub monitor_name: String,
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    pub timestamp: String,
}

impl From<Screenshot> for kw_types::api::ScreenshotImage {
    fn from(screenshot: Screenshot) -> Self {
        Self {
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
}