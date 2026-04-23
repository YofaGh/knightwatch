use reqwest::Client;
use tokio_util::sync::CancellationToken;

use super::structs::WebhookPayload;
use crate::{prelude::*, process_tracker::enums::ProcessTrackerEvent};

pub async fn run_dispatcher(urls: Vec<String>, cancel_token: CancellationToken) {
    let Some(mut rx) = crate::process_tracker::subscribe_events() else {
        return;
    };
    let client = Client::new();
    loop {
        let event = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("webhook: dispatcher shutting down");
                return;
            }
            result = rx.recv() => match result {
                Ok(e) => e,
                Err(err) => {
                    error!("webhook: event channel error: {err}");
                    continue;
                }
            }
        };
        let payload = event_to_payload(&event);
        for url in &urls {
            fire_with_retry(&client, url, &payload, &cancel_token).await;
        }
    }
}

async fn fire_with_retry(
    client: &Client,
    url: &str,
    payload: &WebhookPayload,
    cancel_token: &CancellationToken,
) {
    let mut attempts = 0u32;
    loop {
        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("webhook: retry loop cancelled for {url}");
                return;
            }
            result = client.post(url).json(payload).send() => {
                match result {
                    Ok(r) if r.status().is_success() => return,
                    Ok(r) => warn!("webhook {url}: non-2xx {}", r.status()),
                    Err(e) => warn!("webhook {url}: send error {e}"),
                }
            }
        }
        attempts += 1;
        if attempts >= 3 {
            return;
        }
        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("webhook: backoff sleep cancelled for {url}");
                return;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2u64.pow(attempts))) => {}
        }
    }
}

fn event_to_payload(event: &ProcessTrackerEvent) -> WebhookPayload {
    let (name, data) = match event {
        ProcessTrackerEvent::RootExited { pid } => {
            ("process.root_exited", serde_json::json!({ "pid": pid }))
        }
        ProcessTrackerEvent::ChildrenExited(pids) => (
            "process.children_exited",
            serde_json::json!({ "pids": pids }),
        ),
        ProcessTrackerEvent::ChildrenAppeared(snaps) => (
            "process.children_appeared",
            serde_json::json!({ "count": snaps.len() }),
        ),
        ProcessTrackerEvent::AllChildrenGone => {
            ("process.all_children_gone", serde_json::json!({}))
        }
        ProcessTrackerEvent::InitialSnapshot { root, children } => (
            "process.initial_snapshot",
            serde_json::json!({
                "root_pid": root.pid,
                "child_count": children.len()
            }),
        ),
    };
    WebhookPayload {
        version: env!("CARGO_PKG_VERSION"),
        event: name,
        timestamp: crate::utils::now_rfc3339(),
        data,
    }
}
