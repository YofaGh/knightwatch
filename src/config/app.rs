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
                    None => error!("telegram_token is not set"),
                },
            },
            ConfigAction::Set { field } => {
                let mut persistent = PersistentConfig::load()?;
                match field {
                    ConfigField::TelegramToken { value } => {
                        persistent.telegram_token = value.clone();
                        persistent.save()?;
                        info!("telegram_token updated.");
                    }
                }
            }
        },
    }
    Ok(())
}
