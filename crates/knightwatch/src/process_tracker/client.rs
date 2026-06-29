use tokio::sync::{broadcast, mpsc, oneshot};

use kw_types::process::{ProcessSnapshot, ProcessStatus, ProcessTree};

use super::commands::{ProcessTrackerCommand, ProcessTrackerQuery};
use crate::prelude::*;

/// Subscribe to tracker events (e.g. from a Telegram bot or WebSocket handler).
/// Returns `None` if the tracker was not started (no `--pid` given).
pub fn subscribe_events() -> Option<broadcast::Receiver<super::event::ProcessTrackerEvent>> {
    super::tracker::PROCESS_TRACKER_EVENT_SENDER
        .get()
        .map(|tx| tx.subscribe())
}

fn get_process_tracker_query_sender() -> Option<&'static mpsc::Sender<ProcessTrackerQuery>> {
    super::tracker::PROCESS_TRACKER_QUERY_SENDER.get()
}

fn get_process_tracker_command_sender() -> Option<&'static mpsc::Sender<ProcessTrackerCommand>> {
    super::tracker::PROCESS_TRACKER_COMMAND_SENDER.get()
}

// ─────────────────────────────────────────────────────────────────────────────
// Read-only queries
// ─────────────────────────────────────────────────────────────────────────────

/// Get the root process ids being tracked.
pub async fn get_root_pids() -> Vec<u32> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetTrackedPids { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Get the current root process snapshot.
pub async fn get_root(root_pid: u32) -> Option<ProcessSnapshot> {
    let tx_ref = get_process_tracker_query_sender()?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetRoot {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Get snapshots of all currently live child processes.
pub async fn get_children(root_pid: u32) -> Vec<ProcessSnapshot> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetChildren {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Returns true when all children have exited (work is considered done).
pub async fn is_process_done(root_pid: u32) -> Option<bool> {
    let tx_ref = get_process_tracker_query_sender()?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::IsWorkDone {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Get a process tree by pid.
pub async fn get_process_tree(root_pid: u32) -> Option<ProcessTree> {
    let tx_ref = get_process_tracker_query_sender()?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetProcessTree {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Get all process trees currently being tracked.
pub async fn get_all_process_trees() -> Vec<ProcessTree> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetAllProcessTrees { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Get a process status by pid.
pub async fn get_process_status(root_pid: u32) -> Option<ProcessStatus> {
    let tx_ref = get_process_tracker_query_sender()?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetProcessStatus {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

/// Get the top N processes sorted by the given key.
/// Returns an empty vec if the tracker was not started.
pub async fn get_top_processes(by: kw_types::process::SortKey, limit: usize) -> Vec<ProcessSnapshot> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetTopProcesses {
            by,
            limit,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// Mutating commands
// ─────────────────────────────────────────────────────────────────────────────

/// Send a signal to a single process.
///
/// Returns `Ok(true)` on success, `Ok(false)` if the OS rejected the signal,
/// or `Err` if the PID was not found in the process list.
pub async fn kill_process(pid: u32, signal: kw_types::process::ProcessSignal) -> Result<bool> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::KillProcess {
            pid,
            signal,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Kill a root process and every process in its descendant subtree (SIGKILL).
///
/// Returns the list of PIDs that were successfully signalled.
pub async fn kill_tree(root_pid: u32) -> Result<Vec<u32>> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::KillTree {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Begin tracking a new root PID. A no-op if already tracked.
pub async fn track_pid(pid: u32) -> Result<()> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::TrackPid { pid, response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Stop tracking a root PID and discard its accumulated state.
pub async fn untrack_pid(pid: u32) -> Result<()> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::UntrackPid { pid, response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(interval: std::time::Duration) -> Result<()> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::SetPollInterval {
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Pause polling. The tracker continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll() -> Result<()> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::PausePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll() -> Result<()> {
    let tx_ref =
        get_process_tracker_command_sender().ok_or_else(Error::process_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerCommand::ResumePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}
