#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct PersistentConfig {
    pub telegram_token: Option<String>,
    #[serde(default)]
    pub webhook_urls: Vec<String>,
}

impl super::store::JsonStore for PersistentConfig {
    const NAME: &'static str = "config";
    fn path() -> std::path::PathBuf {
        super::paths::conig_file_path("config.json")
    }
}
