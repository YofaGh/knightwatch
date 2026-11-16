#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct PersistentConfig {
    pub telegram_token: Option<String>,
    #[serde(default)]
    pub webhook_urls: Vec<String>,
}

// hand-written so `{:?}` never prints the bot token or webhook urls (Slack and
// Discord urls carry their secret in the url itself)
impl std::fmt::Debug for PersistentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("PersistentConfig")
            .field(
                "telegram_token",
                &self.telegram_token.as_ref().map(|_| "[redacted]"),
            )
            .field("webhook_urls", &self.webhook_urls.len())
            .finish()
    }
}

impl super::store::JsonStore for PersistentConfig {
    const NAME: &'static str = "config";
    fn path() -> Option<std::path::PathBuf> {
        super::paths::config_file_path("config.json")
    }
}
