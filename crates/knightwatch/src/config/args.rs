use clap::Subcommand;

#[derive(clap::Parser, Debug)]
#[command(
    name = "knightwatch",
    about = "Screen monitoring and notification tool",
    version
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

    /// Enable authentication
    #[arg(long, default_value_t = false)]
    pub enable_auth: bool,

    /// Disable the API server entirely
    #[arg(long, default_value_t = false)]
    pub no_api: bool,

    /// Disable the web dashboard
    #[arg(long, default_value_t = false)]
    pub no_dashboard: bool,

    /// Disable the Screen Capture API, which may require elevated permissions on some platforms
    #[cfg(feature = "screenshot")]
    #[arg(long, default_value_t = false)]
    pub blind: bool,

    /// Process ID to track
    #[arg(long)]
    pub pid: Vec<u32>,

    /// Enable Telegram bot
    #[arg(long, default_value_t = false)]
    pub telegram: bool,

    /// Webhook URLs to notify on process events (repeatable)
    #[arg(long = "webhook")]
    pub webhook_urls: Vec<String>,

    /// Enable Webhooks
    #[arg(long, default_value_t = false)]
    pub with_webhook: bool,

    /// Enable top processes tracker
    #[arg(long, default_value_t = false)]
    pub top_processes: bool,

    /// Limit number of top processes to track
    #[arg(long, default_value_t = 5)]
    pub limit_processes: usize,

    /// Enable system resources
    #[arg(long, default_value_t = false)]
    pub system_resources: bool,

    /// Enable systemd
    #[arg(long, default_value_t = false)]
    pub systemd: bool,

    /// Enable docker tracker
    #[arg(long, default_value_t = false)]
    pub docker: bool,

    /// Allow process commands (kill, track, etc.)
    #[arg(long, default_value_t = false)]
    pub allow_process_commands: bool,

    /// Allow screen commands
    #[arg(long, default_value_t = false)]
    pub allow_screen_commands: bool,

    /// Allow system_resources commands
    #[arg(long, default_value_t = false)]
    pub allow_system_resources_commands: bool,

    /// Allow systemd commands
    #[arg(long, default_value_t = false)]
    pub allow_systemd_commands: bool,

    /// Allow docker commands
    #[arg(long, default_value_t = false)]
    pub allow_docker_commands: bool,
}

impl CliArgs {
    pub fn is_blind(&self) -> bool {
        #[cfg(feature = "screenshot")]
        return self.blind;
        #[cfg(not(feature = "screenshot"))]
        return true;
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage persistent configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage users
    Users {
        #[command(subcommand)]
        action: UsersAction,
    },
}

#[derive(Subcommand, Debug)]
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

#[derive(Subcommand, Debug)]
pub enum UsersAction {
    /// Add a new user
    Add {
        username: String,
    },
    /// Remove a user
    Remove {
        username: String,
    },
    /// List all users
    List,
    /// Remove all users
    Clear,
    /// Show the Telegram authentication token for a user
    Token {
        username: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigField {
    TelegramToken {
        value: Option<String>,
        #[arg(long, default_value_t = false, conflicts_with = "value")]
        clear: bool,
    },
    WebhookUrls {
        #[arg(long)]
        add: Vec<String>,
        #[arg(long)]
        remove: Vec<String>,
        #[arg(
            long,
            default_value_t = false,
            conflicts_with = "add",
            conflicts_with = "remove"
        )]
        clear: bool,
    },
}
