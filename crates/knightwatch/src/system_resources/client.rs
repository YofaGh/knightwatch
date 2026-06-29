use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::resources::*;

use super::commands::{SystemResourcesCommand, SystemResourcesQuery};
use crate::prelude::*;

pub fn subscribe_events() -> Option<broadcast::Receiver<super::event::SystemResourcesEvent>> {
    super::resources::SYSTEM_RESOURCES_EVENT_SENDER
        .get()
        .map(|tx| tx.subscribe())
}

fn get_system_resources_query_sender() -> Option<&'static mpsc::Sender<SystemResourcesQuery>> {
    super::resources::SYSTEM_RESOURCES_QUERY_SENDER.get()
}

fn get_system_resources_command_sender() -> Option<&'static mpsc::Sender<SystemResourcesCommand>> {
    super::resources::SYSTEM_RESOURCES_COMMAND_SENDER.get()
}

/// Get the current system snapshot.
pub async fn get_snapshot() -> Option<SystemSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_resources_query_sender()?
        .send(SystemResourcesQuery::Snapshot { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_cpu() -> Option<CpuSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_resources_query_sender()?
        .send(SystemResourcesQuery::Cpu { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_memory() -> Option<MemorySnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_resources_query_sender()?
        .send(SystemResourcesQuery::Memory { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_disks() -> Vec<DiskSnapshot> {
    let Some(tx_ref) = get_system_resources_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesQuery::Disks { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_networks() -> Vec<NetworkSnapshot> {
    let Some(tx_ref) = get_system_resources_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesQuery::Networks { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_gpus() -> Vec<GpuSnapshot> {
    let Some(tx_ref) = get_system_resources_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesQuery::Gpus { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_battery() -> Option<BatterySnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_resources_query_sender()?
        .send(SystemResourcesQuery::Battery { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_host_info() -> Option<HostInfo> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_resources_query_sender()?
        .send(SystemResourcesQuery::HostInfo { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_temperatures() -> Vec<ThermalSnapshot> {
    let Some(tx_ref) = get_system_resources_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesQuery::Temperatures { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// Mutating commands
// ─────────────────────────────────────────────────────────────────────────────

/// Change the alert thresholds.
pub async fn set_thresholds(thresholds: Thresholds) -> Result<()> {
    let tx_ref = get_system_resources_command_sender()
        .ok_or_else(Error::system_resources_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesCommand::SetThresholds {
            thresholds,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Change the refresh mask.
pub async fn set_refresh_mask(mask: RefreshMask) -> Result<()> {
    let tx_ref = get_system_resources_command_sender()
        .ok_or_else(Error::system_resources_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesCommand::SetRefreshMask { mask, response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(interval: std::time::Duration) -> Result<()> {
    let tx_ref = get_system_resources_command_sender()
        .ok_or_else(Error::system_resources_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesCommand::SetPollInterval {
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Pause polling. The system resources continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll() -> Result<()> {
    let tx_ref = get_system_resources_command_sender()
        .ok_or_else(Error::system_resources_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesCommand::PausePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll() -> Result<()> {
    let tx_ref = get_system_resources_command_sender()
        .ok_or_else(Error::system_resources_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemResourcesCommand::ResumePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}
