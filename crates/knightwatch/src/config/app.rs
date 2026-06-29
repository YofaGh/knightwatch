use clap::Parser;
use std::sync::OnceLock;

use super::{
    args::{CliArgs, Command, ConfigAction, ConfigField, UsersAction},
    persistent::PersistentConfig,
    store::JsonStore,
    users::Users,
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

pub fn handle_command(command: &Command) -> Result<()> {
    match command {
        Command::Config { action } => handle_config_action(action),
        Command::Users { action } => handle_users_action(action),
    }
}

fn handle_config_action(action: &ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Get { field } => {
            let config = get_config();
            match field {
                ConfigField::TelegramToken { .. } => match &config.persistent.telegram_token {
                    Some(t) => println!("telegram_token = {t}"),
                    None => println!("telegram_token is not set"),
                },
                ConfigField::WebhookUrls { .. } => {
                    if config.persistent.webhook_urls.is_empty() {
                        println!("no webhook_urls configured");
                    } else {
                        for url in &config.persistent.webhook_urls {
                            println!("webhook_url = {url}");
                        }
                    }
                }
            }
        }
        ConfigAction::Set { field } => {
            let mut persistent = PersistentConfig::load()?;
            match field {
                ConfigField::TelegramToken { value, clear } => {
                    if *clear {
                        persistent.telegram_token = None;
                        persistent.save()?;
                        println!("telegram_token cleared.");
                    } else if value.is_some() {
                        persistent.telegram_token = value.clone();
                        persistent.save()?;
                        println!("telegram_token updated.");
                    } else {
                        println!("No action: provide a value or --clear.");
                    }
                }
                ConfigField::WebhookUrls { add, remove, clear } => {
                    if *clear {
                        persistent.webhook_urls.clear();
                        println!("webhook_urls cleared.");
                    } else {
                        for url in remove {
                            if persistent.webhook_urls.contains(url) {
                                persistent.webhook_urls.retain(|u| u != url);
                                println!("webhook_url removed: {url}");
                            } else {
                                println!("webhook_url not found: {url}");
                            }
                        }
                        for url in add {
                            if !persistent.webhook_urls.contains(url) {
                                persistent.webhook_urls.push(url.clone());
                                println!("webhook_url added: {url}");
                            } else {
                                println!("webhook_url already exists: {url}");
                            }
                        }
                    }
                    persistent.save()?;
                }
            }
        }
    };
    Ok(())
}

fn handle_users_action(action: &UsersAction) -> Result<()> {
    match action {
        UsersAction::Add { username } => {
            let password = rpassword::prompt_password(format!("Password for '{username}': "))
                .map_err(|e| Error::Config(format!("Failed to read password: {e}")))?;
            let password_hash = super::users::hash_password(&password)?;
            let telegram_token = super::users::generate_telegram_token();
            let mut users = Users::load()?;
            users.add(super::users::User {
                username: username.clone(),
                password_hash,
                telegram_token: telegram_token.clone(),
                telegram_chat_id: None,
            })?;
            users.save()?;
            println!("user added: {username}");
            println!("telegram token: {telegram_token}");
        }
        UsersAction::Remove { username } => {
            let mut users = Users::load()?;
            users.remove(username)?;
            users.save()?;
            println!("user removed: {username}");
        }
        UsersAction::List => {
            let users = Users::load()?;
            if users.users.is_empty() {
                println!("no users configured");
            } else {
                for user in &users.users {
                    let telegram_status = match user.telegram_chat_id {
                        Some(_) => "linked",
                        None => "not linked",
                    };
                    println!("user = {} (telegram: {telegram_status})", user.username);
                }
            }
        }
        UsersAction::Clear => {
            let mut users = Users::load()?;
            users.users.clear();
            users.save()?;
            println!("all users cleared.");
        }
        UsersAction::Token { username } => {
            let users = Users::load()?;
            match users.find(username) {
                Some(user) => {
                    println!("telegram token for '{username}': {}", user.telegram_token);
                    if user.telegram_chat_id.is_some() {
                        println!("(telegram already linked)");
                    } else {
                        println!("(telegram not yet linked)");
                    }
                }
                None => return Err(Error::Config(format!("User '{username}' not found"))),
            }
        }
    };
    Ok(())
}
