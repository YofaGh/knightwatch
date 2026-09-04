use crate::{events::EventPayload, prelude::*, utils::recv_or_pending};

pub async fn run_dispatcher(cancel_token: tokio_util::sync::CancellationToken) {
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
    loop {
        let payload: EventPayload = tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                info!("sse: dispatcher shutting down");
                return;
            }
            e = recv_or_pending(&mut screen_capture_rx, "sse: screen capture") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut process_tracker_rx, "sse: process tracker") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut system_resources_rx, "sse: system resources") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut systemd_rx, "sse: systemd") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut docker_tracker_rx, "sse: docker tracker") => {
                EventPayload::from(&e)
            }
        };
        super::publish(payload);
    }
}
