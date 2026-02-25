use super::paths::{config_dir_path, config_file_path};
use crate::prelude::*;

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct PersistentConfig {
    pub telegram_token: Option<String>,
}

impl PersistentConfig {
    pub fn load() -> Result<Self> {
        let path = config_file_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("Failed to read config file: {e}")))?;
        serde_json::from_str(&contents)
            .map_err(|e| Error::Config(format!("Failed to parse config file: {e}")))
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir_path();
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::Config(format!("Failed to create config directory: {e}")))?;
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {e}")))?;
        std::fs::write(config_file_path(), contents)
            .map_err(|e| Error::Config(format!("Failed to write config file: {e}")))
    }
}
