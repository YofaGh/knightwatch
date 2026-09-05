use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::process::{ProcessSignal, ProcessSnapshot, ProcessTree};

use crate::prelude::*;

#[derive(Debug)]
pub enum ProcessTrackerQuery {
    /// Returns a snapshot of the root process (None if already gone).
    GetRoot {
        root_pid: u32,
        response: oneshot::Sender<Option<ProcessSnapshot>>,
    },
    /// Returns snapshots of all currently live descendants.
    GetChildren {
        root_pid: u32,
        response: oneshot::Sender<Vec<ProcessSnapshot>>,
    },
    /// Returns true when no live descendants remain.
    IsWorkDone {
        root_pid: u32,
        response: oneshot::Sender<Option<bool>>,
    },
    GetTopProcesses {
        by: kw_types::process::ProcessesSortKey,
        limit: usize,
        response: oneshot::Sender<Vec<ProcessSnapshot>>,
    },
    GetTrackedPids {
        response: oneshot::Sender<Vec<u32>>,
    },
    GetProcessTree {
        root_pid: u32,
        response: oneshot::Sender<Option<ProcessTree>>,
    },
    GetAllProcessTrees {
        response: oneshot::Sender<Vec<ProcessTree>>,
    },
    GetProcessStatus {
        root_pid: u32,
        response: oneshot::Sender<Option<kw_types::process::ProcessStatus>>,
    },
    PollStatus {
        response: oneshot::Sender<Option<kw_types::polling::PollStatus>>,
    },
}

#[derive(Debug)]
pub enum ProcessTrackerCommand {
    /// Send an arbitrary signal to a single process.
    /// Responds with `Ok(true)` on success, `Ok(false)` if the signal was
    /// delivered but the OS reported failure, or `Err` if the PID was not found.
    KillProcess {
        user: DisplayUser,
        pid: u32,
        signal: ProcessSignal,
        response: oneshot::Sender<Result<bool>>,
    },
    /// Kill a root process and every descendant in its subtree.
    /// Responds with the list of PIDs that were successfully signalled.
    KillTree {
        user: DisplayUser,
        root_pid: u32,
        response: oneshot::Sender<Result<Vec<u32>>>,
    },
    /// Begin tracking a new root PID. A no-op if the PID is already tracked.
    TrackPid {
        user: DisplayUser,
        pid: u32,
        response: oneshot::Sender<Result<()>>,
    },
    /// Stop tracking a root PID and discard its state.
    UntrackPid {
        user: DisplayUser,
        pid: u32,
        response: oneshot::Sender<Result<()>>,
    },
    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        user: DisplayUser,
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },
    /// Stop emitting ticks; the tracker keeps running and still handles queries/commands.
    PausePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },
    /// Resume ticking at the current poll interval.
    ResumePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },
}

/// Describes which mutating command was executed, with its parameters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProcessCommandAction {
    KillProcess { pid: u32, signal: ProcessSignal },
    KillTree { root_pid: u32 },
    TrackPid { pid: u32 },
    UntrackPid { pid: u32 },
    SetPollInterval { interval: std::time::Duration },
    PausePoll,
    ResumePoll,
}

impl ProcessCommandAction {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::KillProcess { .. } => "kill_process",
            Self::KillTree { .. } => "kill_tree",
            Self::TrackPid { .. } => "track_pid",
            Self::UntrackPid { .. } => "untrack_pid",
            Self::SetPollInterval { .. } => "set_poll_interval",
            Self::PausePoll => "pause_poll",
            Self::ResumePoll => "resume_poll",
        }
    }
}

impl std::fmt::Display for ProcessCommandAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KillProcess { pid, signal } => write!(f, "singal {signal} process {pid}"),
            Self::KillTree { root_pid } => write!(f, "kill tree with root pid {root_pid}"),
            Self::TrackPid { pid } => write!(f, "track process with pid {pid}"),
            Self::UntrackPid { pid } => write!(f, "untrack process with pid {pid}"),
            Self::SetPollInterval { interval } => {
                write!(f, "set poll interval to {}ms", interval.as_millis())
            }
            Self::PausePoll => write!(f, "pause polling"),
            Self::ResumePoll => write!(f, "resume polling"),
        }
    }
}

pub struct ProcessTrackerChannels {
    pub query_tx: mpsc::Sender<ProcessTrackerQuery>,
    pub query_rx: Option<mpsc::Receiver<ProcessTrackerQuery>>,
    pub command_tx: mpsc::Sender<ProcessTrackerCommand>,
    pub command_rx: Option<mpsc::Receiver<ProcessTrackerCommand>>,
    pub event_tx: broadcast::Sender<super::event::ProcessTrackerEvent>,
}

impl ProcessTrackerChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(256);
        // capacity 64: events are cheap and subscribers should keep up
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            command_tx,
            command_rx: Some(command_rx),
            event_tx,
        }
    }

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<ProcessTrackerQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Query receiver already taken".into()))
    }

    pub fn take_command_rx(&mut self) -> Result<mpsc::Receiver<ProcessTrackerCommand>> {
        self.command_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Command receiver already taken".into()))
    }
}
