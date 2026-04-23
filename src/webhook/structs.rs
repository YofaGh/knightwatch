#[derive(Debug, serde::Serialize)]
pub struct WebhookPayload {
    pub version: &'static str,
    pub event: &'static str,
    pub timestamp: String,
    pub data: serde_json::Value,
}