use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub uptime: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub blind: bool,
    pub pid: Vec<u32>,
    pub top_processes: bool,
    pub limit_processes: usize,
    pub telegram_bot: bool,
    pub system_monitor: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScreenshotImage {
    pub data: String,
    pub mime: String,
    pub monitor_name: String,
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScreenshotResponse {
    pub screens: Vec<ScreenshotImage>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct TopProcessesParams {
    pub limit: Option<usize>,
    pub sort: String,
}

#[cfg(feature = "ssr")]
#[derive(Clone, axum::extract::FromRef)]
pub struct AppState {
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub leptos_options: Option<leptos_config::LeptosOptions>,
}

#[cfg(feature = "ssr")]
impl axum::extract::FromRef<AppState> for leptos_config::LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
            .expect("leptos_options must be Some when dashboard is enabled")
    }
}

