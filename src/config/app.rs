use clap::Parser;
use std::sync::OnceLock;

use super::{args::CliArgs, persistent::PersistentConfig};
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

pub fn init_config() -> Result<()> {
    let config = AppConfig {
        args: CliArgs::parse(),
        persistent: PersistentConfig::load()?,
    };
    CONFIG
        .set(config)
        .map_err(|_| Error::Config("Config already initialized".to_string()))
}
