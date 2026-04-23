#[derive(clap::Parser, Debug)]
#[command(
    name = "knightwatch",
    about = "Screen monitoring and notification tool"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Host address for the API server
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port for the API server
    #[arg(long, short, default_value_t = 8083)]
    pub port: u16,

    /// Disable the API server entirely
    #[arg(long, default_value_t = false)]
    pub no_server: bool,

    /// Process ID to track
    #[arg(long)]
    pub pid: Option<u32>,

    /// Enable Telegram bot
    #[arg(long, default_value_t = false)]
    pub telegram: bool,

    /// Webhook URLs to notify on process events (repeatable)
    #[arg(long = "webhook")]
    pub webhook_urls: Vec<String>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Manage persistent configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigAction {
    /// Set a config value
    Set {
        #[command(subcommand)]
        field: ConfigField,
    },
    /// Get a config value
    Get {
        #[command(subcommand)]
        field: ConfigField,
    },
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ConfigField {
    TelegramToken { value: Option<String> },
}
