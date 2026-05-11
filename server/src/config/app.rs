use clap::Parser;
use std::sync::OnceLock;

use super::{
    args::{CliArgs, Command, ConfigAction, ConfigField},
    persistent::PersistentConfig,
};
use crate::prelude::*;

static CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub struct AppConfig {
    pub args: CliArgs,
    pub persistent: PersistentConfig,
}

impl AppConfig {
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.args.host, self.args.port)
    }
}

pub fn get_config() -> &'static AppConfig {
    CONFIG.get().expect("Config not initialized")
}

pub fn init_config() -> Result<&'static AppConfig> {
    let config = AppConfig {
        args: CliArgs::parse(),
        persistent: PersistentConfig::load()?,
    };
    CONFIG
        .set(config)
        .map_err(|_| Error::Config("Config already initialized".to_string()))?;
    Ok(get_config())
}

pub fn handle_config_command(command: &Command) -> Result<()> {
    let config = get_config();
    match command {
        Command::Config { action } => match action {
            ConfigAction::Get { field } => match field {
                ConfigField::TelegramToken { .. } => match &config.persistent.telegram_token {
                    Some(t) => info!("telegram_token = {t}"),
                    None => info!("telegram_token is not set"),
                },
                ConfigField::WebhookUrls { .. } => {
                    if config.persistent.webhook_urls.is_empty() {
                        info!("no webhook_urls configured");
                    } else {
                        for url in &config.persistent.webhook_urls {
                            info!("webhook_url = {url}");
                        }
                    }
                }
            },
            ConfigAction::Set { field } => {
                let mut persistent = PersistentConfig::load()?;
                match field {
                    ConfigField::TelegramToken { value, clear } => {
                        if *clear {
                            persistent.telegram_token = None;
                            persistent.save()?;
                            info!("telegram_token cleared.");
                        } else if value.is_some() {
                            persistent.telegram_token = value.clone();
                            persistent.save()?;
                            info!("telegram_token updated.");
                        } else {
                            info!("No action: provide a value or --clear.");
                        }
                    }
                    ConfigField::WebhookUrls { add, remove, clear } => {
                        let mut persistent = PersistentConfig::load()?;
                        if *clear {
                            persistent.webhook_urls.clear();
                            info!("webhook_urls cleared.");
                        } else {
                            for url in remove {
                                if persistent.webhook_urls.contains(url) {
                                    persistent.webhook_urls.retain(|u| u != url);
                                    info!("webhook_url removed: {url}");
                                } else {
                                    info!("webhook_url not found: {url}");
                                }
                            }
                            for url in add {
                                if !persistent.webhook_urls.contains(url) {
                                    persistent.webhook_urls.push(url.clone());
                                    info!("webhook_url added: {url}");
                                } else {
                                    info!("webhook_url already exists: {url}");
                                }
                            }
                        }
                        persistent.save()?;
                    }
                }
            }
        },
    }
    Ok(())
}
