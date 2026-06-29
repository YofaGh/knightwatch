#[derive(Debug)]
pub enum Error {
    Network(String),
    ChannelClosed(String),
    Screen(String),
    Config(String),
    ProcessTracker(String),
    SystemResources(String),
    DockerTracker(String),
    Systemd(String),
    Other(String),
    TelegramBot(String),
}

impl Error {
    pub fn bind_address(address: &str, err: std::io::Error) -> Self {
        Self::Network(format!("Failed to bind address: {address}, {err}"))
    }
    pub fn channel_closed(err: tokio::sync::oneshot::error::RecvError) -> Self {
        Self::ChannelClosed(format!("Channel was closed: {err}"))
    }
    pub fn unsupported_signal(signal: crate::process_tracker::ProcessSignal) -> Self {
        Self::ProcessTracker(format!(
            "Signal '{signal:?}' is not supported on this platform"
        ))
    }
    pub fn process_commands_disabled() -> Self {
        Self::ProcessTracker(
            "Process commands are disabled — rerun with --allow-process-commands".into(),
        )
    }
    pub fn screen_commands_disabled() -> Self {
        Self::Screen("Screen commands are disabled — rerun with --allow-screen-commands".into())
    }
    pub fn system_resources_commands_disabled() -> Self {
        Self::SystemResources(
            "System resources commands are disabled — rerun with --allow-system-resources-commands"
                .into(),
        )
    }
    pub fn systemd_commands_disabled() -> Self {
        Self::Systemd("Systemd commands are disabled — rerun with --allow-systemd-commands".into())
    }
    pub fn bollard_error(err: bollard::errors::Error) -> Self {
        Self::DockerTracker(format!("Docker API error: {err}"))
    }
    pub fn docker_commands_disabled() -> Self {
        Self::DockerTracker(
            "Docker commands are disabled — rerun with --allow-docker-commands".into(),
        )
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Other(msg)
            | Error::Config(msg)
            | Error::Network(msg)
            | Error::ChannelClosed(msg)
            | Error::TelegramBot(msg)
            | Error::Screen(msg)
            | Error::SystemResources(msg)
            | Error::DockerTracker(msg)
            | Error::Systemd(msg)
            | Error::ProcessTracker(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

impl From<teloxide::errors::RequestError> for Error {
    fn from(err: teloxide::errors::RequestError) -> Self {
        Error::TelegramBot(err.to_string())
    }
}
