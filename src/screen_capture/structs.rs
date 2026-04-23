#[derive(Debug, Clone, serde::Serialize)]
pub struct Screenshot {
    pub image: Vec<u8>,
    pub monitor_name: String,
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    pub timestamp: String,
}