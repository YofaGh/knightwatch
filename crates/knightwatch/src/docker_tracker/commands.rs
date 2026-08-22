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
        id_or_name: String,
        /// Seconds to wait before killing. `None` uses Docker's default (10 s).
        timeout_secs: Option<i32>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Immediately kill a container with SIGKILL (or a custom signal).
    KillContainer {
        id_or_name: String,
        /// e.g. `"SIGKILL"`, `"SIGTERM"`. `None` defaults to `"SIGKILL"`.
        signal: Option<String>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Start a stopped container.
    StartContainer {
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Restart a container (stop + start).
    RestartContainer {
        id_or_name: String,
        timeout_secs: Option<i32>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Pause all processes in a container (SIGSTOP).
    PauseContainer {
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Unpause a paused container.
    UnpauseContainer {
        id_or_name: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },

    /// Suspend polling (event stream keeps running).
    PausePoll {
        response: oneshot::Sender<Result<()>>,
    },

    /// Resume polling at the current interval.
    ResumePoll {
        response: oneshot::Sender<Result<()>>,
    },
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
