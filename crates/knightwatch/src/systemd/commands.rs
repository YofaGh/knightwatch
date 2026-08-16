#![allow(dead_code)]

use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::systemd::{ServiceAction, SystemdSnapshot, UnitSnapshot};

use crate::prelude::*;

pub enum SystemdQuery {
    Snapshot {
        response: oneshot::Sender<Option<SystemdSnapshot>>,
    },
    Unit {
        unit_name: String,
        response: oneshot::Sender<Option<UnitSnapshot>>,
    },
    ByActiveState {
        state: kw_types::systemd::UnitActiveState,
        response: oneshot::Sender<Vec<UnitSnapshot>>,
    },
}

/// Mutating commands that alter systemd state.
/// These require `&mut self` and travel on a separate channel from read-only queries.
#[derive(Debug)]
pub enum SystemdCommand {
    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },
    /// Stop emitting ticks; the systemd keeps running and still handles queries/commands.
    PausePoll {
        response: oneshot::Sender<Result<()>>,
    },
    /// Resume ticking at the current poll interval.
    ResumePoll {
        response: oneshot::Sender<Result<()>>,
    },
    /// Restart/start/stop/reload a unit. Requires the privileged helper —
    /// fails cleanly if it isn't ready or wasn't authorized.
    Control {
        unit_name: String,
        action: ServiceAction,
        response: oneshot::Sender<Result<()>>,
    },
}

pub struct SystemdMonitorChannels {
    pub query_tx: mpsc::Sender<SystemdQuery>,
    pub query_rx: Option<mpsc::Receiver<SystemdQuery>>,
    pub command_tx: mpsc::Sender<SystemdCommand>,
    pub command_rx: Option<mpsc::Receiver<SystemdCommand>>,
    pub event_tx: broadcast::Sender<super::event::SystemdEvent>,
}

impl SystemdMonitorChannels {
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

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<SystemdQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::Systemd("Query receiver already taken".into()))
    }

    pub fn take_command_rx(&mut self) -> Result<mpsc::Receiver<SystemdCommand>> {
        self.command_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Command receiver already taken".into()))
    }
}
