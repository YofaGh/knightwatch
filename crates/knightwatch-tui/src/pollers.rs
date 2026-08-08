use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;

use crate::events::AppEvent;

/// Local pacing state for one poller, shared between the poller task and
/// that tab's polling panel. Flipping either field here takes effect on
/// the poller's next loop iteration — no restart, no request needed.
#[derive(Clone, Copy)]
pub struct PollControl {
    pub paused: bool,
    pub interval_ms: u64,
}

impl PollControl {
    pub const fn new(interval_ms: u64) -> Self {
        Self {
            paused: false,
            interval_ms,
        }
    }
    pub fn new_arc(interval_ms: u64) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(interval_ms)))
    }
}

/// Spawns a background task that calls `fetch` on a fixed interval and
/// forwards whatever `AppEvent` it produces into `tx`. This is the whole
/// "shared API, separate endpoint per tab" pattern: every tab gets one
/// call to `spawn_poller` with its own interval and its own fetch closure
/// (which itself just wraps a `GET` through the shared `ApiClient`).
/// `fetch` returning `None` means "nothing to report this tick" — a
/// network error, a 404, a bad decode — the poller just quietly waits for
/// the next tick rather than retrying in a hot loop.
fn spawn_poller<F, Fut>(tx: Sender<AppEvent>, control: Arc<Mutex<PollControl>>, mut fetch: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Option<AppEvent>> + Send,
{
    tokio::spawn(async move {
        loop {
            let PollControl {
                paused,
                interval_ms,
            } = *control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if paused {
                // Short sleep, not a full interval, so resuming is picked
                // up promptly instead of after a stale wait.
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            tokio::time::sleep(Duration::from_millis(interval_ms)).await;

            // Pause could have happened while we were sleeping.
            if control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .paused
            {
                continue;
            }

            if let Some(event) = fetch().await
                && tx.send(event).await.is_err()
            {
                break;
            }
        }
    });
}

pub fn spawn_input(tx: Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut stream = crossterm::event::EventStream::new();
        while let Some(Ok(event)) = futures::StreamExt::next(&mut stream).await {
            if tx.send(AppEvent::Input(event)).await.is_err() {
                break;
            }
        }
    });
}

pub fn spawn_screen_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        async move {
            api.screenshot()
                .await
                .map(|r| r.screens)
                .map_or(None, |screenshots| {
                    Some(AppEvent::ScreenImages(screenshots))
                })
        }
    });
}

pub fn spawn_system_resources_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        async move {
            api.system_snapshot().await.map_or(None, |snapshot| {
                Some(AppEvent::SystemSnapshot(Box::new(snapshot)))
            })
        }
    });
}

pub fn spawn_docker_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        async move {
            api.docker_containers()
                .await
                .map_or(None, |snapshot| Some(AppEvent::DockerContainers(snapshot)))
        }
    });
}

pub fn spawn_systemd_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        async move {
            api.systemd_snapshot()
                .await
                .map_or(None, |snapshot| Some(AppEvent::SystemdSnapshot(snapshot)))
        }
    });
}

pub fn spawn_process_trees_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        async move {
            api.process_trees().await.map_or(None, |process_trees| {
                Some(AppEvent::ProcessTrees(process_trees))
            })
        }
    });
}

pub fn spawn_top_processes_poller(
    tx: Sender<AppEvent>,
    api: Arc<ApiClient>,
    poll_config: Arc<std::sync::Mutex<crate::tabs::TopProcessesPollConfig>>,
    control: Arc<Mutex<PollControl>>,
) {
    spawn_poller(tx, control, move || {
        let api = api.clone();
        let config = poll_config.clone();
        async move {
            let cfg = *config
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            api.top_processes(cfg.sort, cfg.limit)
                .await
                .map_or(None, |top_processes| {
                    Some(AppEvent::TopProcesses(top_processes))
                })
        }
    });
}
