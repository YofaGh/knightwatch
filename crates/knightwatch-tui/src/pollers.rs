use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::Sender;

use crate::{events::AppEvent, utils};

/// Spawns a background task that calls `fetch` on a fixed interval and
/// forwards whatever `AppEvent` it produces into `tx`. This is the whole
/// "shared API, separate endpoint per tab" pattern: every tab gets one
/// call to `spawn_poller` with its own interval and its own fetch closure
/// (which itself just wraps a `GET` through the shared `ApiClient`).
/// `fetch` returning `None` means "nothing to report this tick" — a
/// network error, a 404, a bad decode — the poller just quietly waits for
/// the next tick rather than retrying in a hot loop.
fn spawn_poller<F, Fut>(tx: Sender<AppEvent>, interval: Duration, mut fetch: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Option<AppEvent>> + Send,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Some(event) = fetch().await {
                if tx.send(event).await.is_err() {
                    break;
                }
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

pub fn spawn_screen_poller(tx: Sender<AppEvent>, api: Arc<kw_clients::ApiClient>) {
    spawn_poller(tx, Duration::from_secs(10), move || {
        let api = api.clone();
        async move {
            match utils::fetch_screen_image(&api).await {
                Ok(image) => Some(AppEvent::ScreenImage(image)),
                Err(_) => None,
            }
        }
    });
}
