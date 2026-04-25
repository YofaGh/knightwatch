use sysinfo::ProcessStatus;
use tokio::sync::oneshot;

use super::structs::ProcessSnapshot;

/// Events emitted by the tracker on its broadcast bus.
/// Subscribers receive these without polling.
#[derive(Debug, Clone)]
pub enum ProcessTrackerEvent {
    /// Emitted on the very first tick; contains everything we found.
    InitialSnapshot {
        root: ProcessSnapshot,
        children: Vec<ProcessSnapshot>,
    },
    /// One or more new child processes appeared.
    ChildrenAppeared(Vec<ProcessSnapshot>),
    /// One or more child PIDs exited.
    ChildrenExited(Vec<u32>),
    /// All descendants have exited (root may still be alive).
    AllChildrenGone,
    /// The root process itself has exited.
    RootExited { pid: u32 },
}

/// One-shot queries callers can send to read tracker state synchronously.
#[derive(Debug)]
pub enum ProcessTrackerQuery {
    /// Returns a snapshot of the root process (None if already gone).
    GetRoot {
        response: oneshot::Sender<Option<ProcessSnapshot>>,
    },
    /// Returns snapshots of all currently live descendants.
    GetChildren {
        response: oneshot::Sender<Vec<ProcessSnapshot>>,
    },
    /// Returns true when no live descendants remain.
    IsWorkDone { response: oneshot::Sender<bool> },
    GetTopProcesses {
        by: SortKey,
        limit: usize,
        response: oneshot::Sender<Vec<ProcessSnapshot>>,
    },
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessState {
    Running,
    Sleeping,
    Other(String),
    Gone,
}

impl From<ProcessStatus> for ProcessState {
    fn from(status: ProcessStatus) -> Self {
        match status {
            ProcessStatus::Run => ProcessState::Running,
            ProcessStatus::Sleep | ProcessStatus::Idle => ProcessState::Sleeping,
            other => ProcessState::Other(format!("{other:?}")),
        }
    }
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ProcessState::Running => write!(f, "running"),
            ProcessState::Sleeping => write!(f, "sleeping"),
            ProcessState::Other(s) => write!(f, "other({s})"),
            ProcessState::Gone => write!(f, "gone"),
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Serialize, Clone)]
pub enum FDType {
    File,
    Socket,
    Pipe,
    Other,
}

#[cfg(target_os = "linux")]
impl std::fmt::Display for FDType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FDType::File => "file",
            FDType::Socket => "socket",
            FDType::Pipe => "pipe",
            FDType::Other => "other",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SortKey {
    Memory,
    Cpu,
}

impl std::fmt::Display for SortKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "Memory"),
            Self::Cpu => write!(f, "Cpu"),
        }
    }
}

impl TryFrom<String> for SortKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "cpu" => Ok(Self::Cpu),
            "mem" => Ok(Self::Memory),
            _ => Err(format!("Invalid sort key: '{value}'. Expected 'cpu' or 'mem'")),
        }
    }
}
