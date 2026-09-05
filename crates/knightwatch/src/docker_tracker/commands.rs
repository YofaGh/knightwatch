use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::docker::ContainerSnapshot;

use crate::prelude::*;

#[derive(Debug)]
pub enum DockerTrackerQuery {
    /// Returns snapshots of all currently tracked containers.
    ListContainers {
        response: oneshot::Sender<Vec<ContainerSnapshot>>,
    },

    /// Returns the snapshot for a single container by ID or name.
    /// `None` if not currently tracked.
    GetContainer {
        id_or_name: String,
        response: oneshot::Sender<Option<ContainerSnapshot>>,
    },

    /// Returns the top N containers sorted by the given key.
    GetTopContainers {
        by: kw_types::docker::DockerSortKey,
        limit: usize,
        response: oneshot::Sender<Vec<ContainerSnapshot>>,
    },
    PollStatus {
        response: oneshot::Sender<Option<kw_types::polling::PollStatus>>,
    },
}

// ============================================================================
// Commands
// ============================================================================

/// Mutating commands — require `&mut self` and travel on the command channel.
#[derive(Debug)]
pub enum DockerTrackerCommand {
    /// Stop a running container (graceful SIGTERM + timeout, then SIGKILL).
    StopContainer {
        user: DisplayUser,
        id_or_name: String,
        /// Seconds to wait before killing. `None` uses Docker's default (10 s).
        timeout_secs: Option<i32>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Immediately kill a container with SIGKILL (or a custom signal).
    KillContainer {
        user: DisplayUser,
        id_or_name: String,
        /// e.g. `"SIGKILL"`, `"SIGTERM"`. `None` defaults to `"SIGKILL"`.
        signal: Option<String>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Start a stopped container.
    StartContainer {
        user: DisplayUser,
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Restart a container (stop + start).
    RestartContainer {
        user: DisplayUser,
        id_or_name: String,
        timeout_secs: Option<i32>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Pause all processes in a container (SIGSTOP).
    PauseContainer {
        user: DisplayUser,
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Unpause a paused container.
    UnpauseContainer {
        user: DisplayUser,
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        user: DisplayUser,
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },

    /// Suspend polling (event stream keeps running).
    PausePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },

    /// Resume polling at the current interval.
    ResumePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },
}

/// Describes which mutating command was executed, with its parameters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DockerCommandAction {
    Stop {
        id_or_name: String,
        container_name: Option<String>,
        timeout_secs: Option<i32>,
    },
    Kill {
        id_or_name: String,
        container_name: Option<String>,
        signal: Option<String>,
    },
    Start {
        id_or_name: String,
        container_name: Option<String>,
    },
    Restart {
        id_or_name: String,
        container_name: Option<String>,
        timeout_secs: Option<i32>,
    },
    Pause {
        id_or_name: String,
        container_name: Option<String>,
    },
    Unpause {
        id_or_name: String,
        container_name: Option<String>,
    },
    SetPollInterval {
        interval: std::time::Duration,
    },
    PausePoll,
    ResumePoll,
}

impl DockerCommandAction {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Stop { .. } => "stop",
            Self::Kill { .. } => "kill",
            Self::Start { .. } => "start",
            Self::Restart { .. } => "restart",
            Self::Pause { .. } => "pause",
            Self::Unpause { .. } => "unpause",
            Self::SetPollInterval { .. } => "set_poll_interval",
            Self::PausePoll => "pause_poll",
            Self::ResumePoll => "resume_poll",
        }
    }

    /// The id/name the user targeted, if this action is container-scoped.
    pub fn id_or_name(&self) -> Option<&str> {
        match self {
            Self::Stop { id_or_name, .. }
            | Self::Kill { id_or_name, .. }
            | Self::Start { id_or_name, .. }
            | Self::Restart { id_or_name, .. }
            | Self::Pause { id_or_name, .. }
            | Self::Unpause { id_or_name, .. } => Some(id_or_name),
            Self::SetPollInterval { .. } | Self::PausePoll | Self::ResumePoll => None,
        }
    }

    /// The resolved container name, if known and if this action is container-scoped.
    pub fn container_name(&self) -> Option<&str> {
        match self {
            Self::Stop { container_name, .. }
            | Self::Kill { container_name, .. }
            | Self::Start { container_name, .. }
            | Self::Restart { container_name, .. }
            | Self::Pause { container_name, .. }
            | Self::Unpause { container_name, .. } => container_name.as_deref(),
            Self::SetPollInterval { .. } | Self::PausePoll | Self::ResumePoll => None,
        }
    }
}

impl std::fmt::Display for DockerCommandAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn target(id_or_name: &str, container_name: Option<&String>) -> String {
            match container_name {
                Some(name) if name != id_or_name => format!("{name} ({id_or_name})"),
                _ => id_or_name.to_string(),
            }
        }

        match self {
            Self::Stop {
                id_or_name,
                container_name,
                timeout_secs,
            } => match timeout_secs {
                Some(t) => write!(
                    f,
                    "stop {} (timeout {t}s)",
                    target(id_or_name, container_name.as_ref())
                ),
                None => write!(f, "stop {}", target(id_or_name, container_name.as_ref())),
            },
            Self::Kill {
                id_or_name,
                container_name,
                signal,
            } => match signal {
                Some(s) => write!(
                    f,
                    "kill {} (signal {s})",
                    target(id_or_name, container_name.as_ref())
                ),
                None => write!(f, "kill {}", target(id_or_name, container_name.as_ref())),
            },
            Self::Start {
                id_or_name,
                container_name,
            } => write!(f, "start {}", target(id_or_name, container_name.as_ref())),
            Self::Restart {
                id_or_name,
                container_name,
                timeout_secs,
            } => match timeout_secs {
                Some(t) => write!(
                    f,
                    "restart {} (timeout {t}s)",
                    target(id_or_name, container_name.as_ref())
                ),
                None => write!(f, "restart {}", target(id_or_name, container_name.as_ref())),
            },
            Self::Pause {
                id_or_name,
                container_name,
            } => write!(f, "pause {}", target(id_or_name, container_name.as_ref())),
            Self::Unpause {
                id_or_name,
                container_name,
            } => write!(f, "unpause {}", target(id_or_name, container_name.as_ref())),
            Self::SetPollInterval { interval } => {
                write!(f, "set poll interval to {}ms", interval.as_millis())
            }
            Self::PausePoll => write!(f, "pause polling"),
            Self::ResumePoll => write!(f, "resume polling"),
        }
    }
}

pub struct DockerTrackerChannels {
    pub query_tx: mpsc::Sender<DockerTrackerQuery>,
    pub query_rx: Option<mpsc::Receiver<DockerTrackerQuery>>,
    pub command_tx: mpsc::Sender<DockerTrackerCommand>,
    pub command_rx: Option<mpsc::Receiver<DockerTrackerCommand>>,
    pub event_tx: broadcast::Sender<super::event::DockerTrackerEvent>,
}

impl DockerTrackerChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            command_tx,
            command_rx: Some(command_rx),
            event_tx,
        }
    }

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<DockerTrackerQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::DockerTracker("Query receiver already taken".into()))
    }

    pub fn take_command_rx(&mut self) -> Result<mpsc::Receiver<DockerTrackerCommand>> {
        self.command_rx
            .take()
            .ok_or_else(|| Error::DockerTracker("Command receiver already taken".into()))
    }
}
