use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::{events::EventPayload, prelude::warn};

fn make_sse_stream<F>(filter: F) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
where
    F: Fn(&EventPayload) -> bool + Send + 'static,
{
    let stream = tokio_stream::wrappers::BroadcastStream::new(super::subscribe()).filter_map(
        move |result| match result {
            Ok(payload) => {
                if !filter(&payload) {
                    return None;
                }
                let event_name = payload.event;
                match serde_json::to_string(&payload) {
                    Ok(json) => Some(Ok(Event::default().event(event_name).data(json))),
                    Err(e) => {
                        warn!("sse: serialize error: {e}");
                        None
                    }
                }
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                warn!("sse: client lagged, dropped {n} events");
                None
            }
        },
    );
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

pub async fn sse_stream() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    make_sse_stream(|_| true)
}

pub async fn sse_stream_process() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    make_sse_stream(EventPayload::is_process_tracker)
}

pub async fn sse_stream_system_resources() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    make_sse_stream(EventPayload::is_system_resources)
}

pub async fn sse_stream_systemd() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    make_sse_stream(EventPayload::is_systemd)
}

pub async fn sse_stream_docker() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    make_sse_stream(EventPayload::is_docker_tracker)
}
