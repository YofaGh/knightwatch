mod app;
mod args;
mod login;
mod paths;
mod persistent;
mod store;
mod users;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod login_tests;

#[cfg(test)]
pub use {args::CliArgs, persistent::PersistentConfig};

pub use app::{AppConfig, get_config, handle_command, init_config};
pub use login::{LoginOutcome, login};
pub use paths::log_dir;
pub use users::{DisplayUser, User, get_users, load_users, set_user_chat_id};
