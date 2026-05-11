use tokio::sync::{broadcast, mpsc, oneshot};

use super::{enums::ProcessTrackerQuery, structs::ProcessSnapshot};

/// Subscribe to tracker events (e.g. from a Telegram bot or WebSocket handler).
/// Returns `None` if the tracker was not started (no `--pid` given).
pub fn subscribe_events() -> Option<broadcast::Receiver<super::enums::ProcessTrackerEvent>> {
    super::tracker::PROCESS_TRACKER_EVENT_SENDER
        .get()
        .map(|tx| tx.subscribe())
}

fn get_process_tracker_query_sender() -> Option<&'static mpsc::Sender<ProcessTrackerQuery>> {
    super::tracker::PROCESS_TRACKER_QUERY_SENDER.get()
}

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
    rx.await.unwrap_or(None)
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
pub async fn is_work_done(root_pid: u32) -> bool {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return true; // no tracker = no work to wait for
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::IsWorkDone {
            root_pid,
            response: tx,
        })
        .await;
    rx.await.unwrap_or(true)
}

/// Get the top N processes sorted by the given key.
/// Returns an empty vec if the tracker was not started.
pub async fn get_top_processes(by: super::enums::SortKey, limit: usize) -> Vec<ProcessSnapshot> {
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
