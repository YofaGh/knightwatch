use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::resources::{
    AlarmSnapshot, BatterySnapshot, CpuSnapshot, DiskSnapshot, GpuSnapshot, HostInfo,
    MemorySnapshot, NetworkSnapshot, RefreshMask, SystemSnapshot, ThermalSnapshot, Thresholds,
};

use crate::prelude::*;

#[derive(Debug)]
pub enum SystemResourcesQuery {
    /// Returns the most recent full snapshot.
    Snapshot {
        response: oneshot::Sender<Option<SystemSnapshot>>,
    },

    /// Returns the most recent CPU reading only (cheaper to clone).
    Cpu {
        response: oneshot::Sender<Option<CpuSnapshot>>,
    },

    /// Returns the most recent memory reading.
    Memory {
        response: oneshot::Sender<Option<MemorySnapshot>>,
    },

    /// Returns the most recent per-disk readings.
    Disks {
        response: oneshot::Sender<Vec<DiskSnapshot>>,
    },

    /// Returns the most recent per-network-interface readings.
    Networks {
        response: oneshot::Sender<Vec<NetworkSnapshot>>,
    },

    /// Returns the most recent GPU readings (may be empty if unsupported).
    Gpus {
        response: oneshot::Sender<Vec<GpuSnapshot>>,
    },

    /// Returns the most recent battery snapshot (None if no battery present).
    Battery {
        response: oneshot::Sender<Option<BatterySnapshot>>,
    },

    /// Returns the host info (static — only changes on hostname/OS update).
    HostInfo {
        response: oneshot::Sender<Option<HostInfo>>,
    },

    /// Returns thermal readings (may be empty if unsupported).
    Temperatures {
        response: oneshot::Sender<Vec<ThermalSnapshot>>,
    },
    /// Returns the current state of all threshold alarms, independent of
    /// event emission/cooldown timing.
    Alarms {
        response: oneshot::Sender<AlarmSnapshot>,
    },
    PollStatus {
        response: oneshot::Sender<Option<kw_types::polling::PollStatus>>,
    },
    GetThresholds {
        response: oneshot::Sender<Option<Thresholds>>,
    },
    GetRefreshMask {
        response: oneshot::Sender<Option<RefreshMask>>,
    },
}

#[derive(Debug)]
pub enum SystemResourcesCommand {
    /// Replace all alert thresholds at once.
    SetThresholds {
        user: DisplayUser,
        thresholds: Thresholds,
        response: oneshot::Sender<Result<()>>,
    },

    /// Control which subsystems are refreshed each tick.
    SetRefreshMask {
        user: DisplayUser,
        mask: RefreshMask,
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SystemResourcesCommandAction {
    SetThresholds { thresholds: Thresholds },
    SetRefreshMask { refresh_mask: RefreshMask },
    SetPollInterval { interval: std::time::Duration },
    PausePoll,
    ResumePoll,
}

impl SystemResourcesCommandAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SetThresholds { .. } => "set_thresholds",
            Self::SetRefreshMask { .. } => "set_refresh_mask",
            Self::SetPollInterval { .. } => "set_poll_interval",
            Self::PausePoll => "pause_poll",
            Self::ResumePoll => "resume_poll",
        }
    }
}

impl std::fmt::Display for SystemResourcesCommandAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::SetThresholds { thresholds } => write!(f, "{} {}", self.name(), thresholds),
            Self::SetRefreshMask { refresh_mask } => write!(f, "{} {}", self.name(), refresh_mask),
            Self::SetPollInterval { interval } => {
                write!(f, "set poll interval to {}ms", interval.as_millis())
            }
            Self::PausePoll => write!(f, "pause polling"),
            Self::ResumePoll => write!(f, "resume polling"),
        }
    }
}

pub struct SystemResourcesChannels {
    pub query_tx: mpsc::Sender<SystemResourcesQuery>,
    pub query_rx: Option<mpsc::Receiver<SystemResourcesQuery>>,
    pub command_tx: mpsc::Sender<SystemResourcesCommand>,
    pub command_rx: Option<mpsc::Receiver<SystemResourcesCommand>>,
    pub event_tx: broadcast::Sender<super::event::SystemResourcesEvent>,
}

impl SystemResourcesChannels {
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

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<SystemResourcesQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::SystemResources("Query receiver already taken".into()))
    }

    pub fn take_command_rx(&mut self) -> Result<mpsc::Receiver<SystemResourcesCommand>> {
        self.command_rx
            .take()
            .ok_or_else(|| Error::SystemResources("Command receiver already taken".into()))
    }
}
