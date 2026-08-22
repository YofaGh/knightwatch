use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::docker::ContainerSnapshot;

use super::commands::{DockerTrackerCommand, DockerTrackerQuery};
use crate::prelude::*;

/// Subscribe to tracker events (e.g. from a Telegram bot or WebSocket handler).
/// Returns `None` if the tracker was not started.
pub fn subscribe_events() -> Option<broadcast::Receiver<super::event::DockerTrackerEvent>> {
    super::tracker::DOCKER_TRACKER_EVENT_SENDER
        .get()
        .map(tokio::sync::broadcast::Sender::subscribe)
}

fn get_docker_tracker_query_sender() -> Option<&'static mpsc::Sender<DockerTrackerQuery>> {
    super::tracker::DOCKER_TRACKER_QUERY_SENDER.get()
}

fn get_docker_tracker_command_sender() -> Option<&'static mpsc::Sender<DockerTrackerCommand>> {
    super::tracker::DOCKER_TRACKER_COMMAND_SENDER.get()
}

/// Query the tracker for a list of all containers. Returns an empty list if the tracker is not running.
pub async fn list_containers() -> Vec<ContainerSnapshot> {
    let Some(tx_ref) = get_docker_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerQuery::ListContainers { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Query the tracker for a single container by ID or name. Returns `None` if the tracker is not running or if no container matches.
pub async fn get_container(id_or_name: String) -> Option<ContainerSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_docker_tracker_query_sender()?
        .send(DockerTrackerQuery::GetContainer {
            id_or_name,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Query the tracker for the top N containers sorted by a specific key. Returns an empty list if the tracker is not running.
pub async fn get_top_containers(
    by: kw_types::docker::DockerSortKey,
    limit: usize,
) -> Vec<ContainerSnapshot> {
    let Some(tx_ref) = get_docker_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerQuery::GetTopContainers {
            by,
            limit,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_poll_status() -> Option<kw_types::polling::PollStatus> {
    let (tx, rx) = oneshot::channel();
    let _ = get_docker_tracker_query_sender()?
        .send(DockerTrackerQuery::PollStatus { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Mutating commands
// ─────────────────────────────────────────────────────────────────────────────

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn stop_container(id_or_name: String, timeout_secs: Option<i32>) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::StopContainer {
            id_or_name,
            timeout_secs,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn kill_container(id_or_name: String, signal: Option<String>) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::KillContainer {
            id_or_name,
            signal,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn start_container(id_or_name: String) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::StartContainer {
            id_or_name,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn restart_container(id_or_name: String, timeout_secs: Option<i32>) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::RestartContainer {
            id_or_name,
            timeout_secs,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn pause_container(id_or_name: String) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::PauseContainer {
            id_or_name,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Send a signal to a container. Returns `None` if the tracker is not running or if no container matches.
pub async fn unpause_container(id_or_name: String) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::UnpauseContainer {
            id_or_name,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(interval: std::time::Duration) -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::SetPollInterval {
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Pause polling. The capture continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll() -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::PausePoll { response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll() -> Result<()> {
    let tx_ref = get_docker_tracker_command_sender().ok_or_else(Error::docker_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(DockerTrackerCommand::ResumePoll { response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}
