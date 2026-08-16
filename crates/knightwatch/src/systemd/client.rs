use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::systemd::{ServiceAction, SystemdSnapshot, UnitActiveState, UnitSnapshot};

use super::{
    commands::{SystemdCommand, SystemdQuery},
    event::SystemdEvent,
};
use crate::prelude::*;

#[cfg(target_os = "linux")]
pub fn subscribe_events() -> Option<broadcast::Receiver<SystemdEvent>> {
    super::monitor::SYSTEMD_EVENT_SENDER
        .get()
        .map(tokio::sync::broadcast::Sender::subscribe)
}

#[cfg(not(target_os = "linux"))]
pub const fn subscribe_events() -> Option<broadcast::Receiver<SystemdEvent>> {
    None
}

#[cfg(target_os = "linux")]
fn get_systemd_query_sender() -> Option<&'static mpsc::Sender<SystemdQuery>> {
    super::monitor::SYSTEMD_QUERY_SENDER.get()
}

#[cfg(not(target_os = "linux"))]
const fn get_systemd_query_sender() -> Option<&'static mpsc::Sender<SystemdQuery>> {
    None
}

#[cfg(target_os = "linux")]
fn get_systemd_command_sender() -> Option<&'static mpsc::Sender<SystemdCommand>> {
    super::monitor::SYSTEMD_COMMAND_SENDER.get()
}

#[cfg(not(target_os = "linux"))]
const fn get_systemd_command_sender() -> Option<&'static mpsc::Sender<SystemdCommand>> {
    None
}

pub async fn get_snapshot() -> Option<SystemdSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_systemd_query_sender()?
        .send(SystemdQuery::Snapshot { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_unit(unit_name: String) -> Option<UnitSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_systemd_query_sender()?
        .send(SystemdQuery::Unit {
            unit_name,
            response: tx,
        })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_units_by_active_state(state: UnitActiveState) -> Vec<UnitSnapshot> {
    let Some(tx_ref) = get_systemd_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdQuery::ByActiveState {
            state,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_failed_units() -> Vec<UnitSnapshot> {
    let Some(tx_ref) = get_systemd_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdQuery::ByActiveState {
            state: UnitActiveState::Failed,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// Mutating commands
// ─────────────────────────────────────────────────────────────────────────────

pub async fn control_unit(unit_name: String, action: ServiceAction) -> Result<()> {
    let tx_ref = get_systemd_command_sender().ok_or_else(Error::systemd_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdCommand::Control {
            unit_name,
            action,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(interval: std::time::Duration) -> Result<()> {
    let tx_ref = get_systemd_command_sender().ok_or_else(Error::systemd_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdCommand::SetPollInterval {
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Pause polling. The systemd continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll() -> Result<()> {
    let tx_ref = get_systemd_command_sender().ok_or_else(Error::systemd_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdCommand::PausePoll { response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll() -> Result<()> {
    let tx_ref = get_systemd_command_sender().ok_or_else(Error::systemd_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemdCommand::ResumePoll { response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}
