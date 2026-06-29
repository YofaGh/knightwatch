use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::{prelude::*, utils::recv_or_pending, events::EventPayload};

pub async fn run_dispatcher(urls: Vec<String>, cancel_token: CancellationToken) {
    let mut process_tracker_rx = crate::process_tracker::subscribe_events();
    let mut system_resources_rx = crate::system_resources::subscribe_events();
    let mut systemd_rx = crate::systemd::subscribe_events();
    let mut docker_tracker_rx = crate::docker_tracker::subscribe_events();
    if crate::all_none!(
        process_tracker_rx,
        system_resources_rx,
        systemd_rx,
        docker_tracker_rx
    ) {
        return;
    }
    let client = Client::new();
    loop {
        let payload = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("webhook: dispatcher shutting down");
                return;
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
        for url in &urls {
            fire_with_retry(&client, url, &payload, &cancel_token).await;
        }
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
