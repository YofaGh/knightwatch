use reqwest::Client;
use tokio_util::sync::CancellationToken;

use kw_types::event::EventPayload;

use crate::{prelude::*, utils::recv_or_pending};

/// How long one webhook request may take before it counts as failed.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub async fn run_dispatcher(targets: Vec<super::WebhookTarget>, cancel_token: CancellationToken) {
    let mut screen_capture_rx = crate::screen_capture::subscribe_events();
    let mut process_tracker_rx = crate::process_tracker::subscribe_events();
    let mut system_resources_rx = crate::system_resources::subscribe_events();
    let mut systemd_rx = crate::systemd::subscribe_events();
    let mut docker_tracker_rx = crate::docker_tracker::subscribe_events();
    if crate::all_none!(
        screen_capture_rx,
        process_tracker_rx,
        system_resources_rx,
        systemd_rx,
        docker_tracker_rx
    ) {
        return;
    }
    let client = client();
    loop {
        let payload = tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                info!("webhook: dispatcher shutting down");
                return;
            }
            e = recv_or_pending(&mut screen_capture_rx, "webhook: screen capture") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut process_tracker_rx, "webhook: process tracker") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut system_resources_rx, "webhook: system resources") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut systemd_rx, "webhook: systemd") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut docker_tracker_rx, "webhook: docker tracker") => {
                EventPayload::from(&e)
            }
        };
        dispatch(&client, &targets, &payload, &cancel_token);
    }
}

fn client() -> Client {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_else(|e| {
            warn!("webhook: could not build a client with a timeout: {e}");
            Client::new()
        })
}

/// Sends to each target in its own task, so a slow target holds back no one else.
// shortcut: no cap on sends in flight (each ends within ~36 s), add a per-target queue if events outpace that.
fn dispatch(
    client: &Client,
    targets: &[super::WebhookTarget],
    payload: &EventPayload,
    cancel_token: &CancellationToken,
) {
    let is_tick = payload.is_tick();
    for target in targets {
        if is_tick && !target.include_ticks {
            continue;
        }
        let (client, url, payload, cancel_token) = (
            client.clone(),
            target.url.clone(),
            payload.clone(),
            cancel_token.clone(),
        );
        tokio::spawn(async move { fire_with_retry(&client, &url, &payload, &cancel_token).await });
    }
}

async fn fire_with_retry(
    client: &Client,
    url: &str,
    payload: &EventPayload,
    cancel_token: &CancellationToken,
) {
    let mut attempts = 0u32;
    loop {
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
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
        attempts = attempts.saturating_add(1);
        if attempts >= 3 {
            return;
        }
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                info!("webhook: backoff sleep cancelled for {url}");
                return;
            }
            () = tokio::time::sleep(tokio::time::Duration::from_secs(2u64.pow(attempts))) => {}
        }
    }
}

#[cfg(test)]
#[path = "dispatcher_tests.rs"]
mod tests;
