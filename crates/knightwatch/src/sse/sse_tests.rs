#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use axum::{Router, routing::get};
use serde_json::{Value, json};
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use kw_types::event::{EventPayload, EventSource};

use super::{
    dispatcher::run_dispatcher,
    handlers::{
        sse_stream, sse_stream_docker, sse_stream_process, sse_stream_screen,
        sse_stream_system_resources, sse_stream_systemd,
    },
    publish,
};

// ───────────────────────── Helpers ─────────────────────────

async fn serve() -> SocketAddr {
    // Same paths as `create_sse_router`, without the auth layer.
    let app = Router::new()
        .route("/sse", get(sse_stream))
        .route("/sse/screen-capture", get(sse_stream_screen))
        .route("/sse/process-tracker", get(sse_stream_process))
        .route("/sse/system-resources", get(sse_stream_system_resources))
        .route("/sse/systemd", get(sse_stream_systemd))
        .route("/sse/docker-tracker", get(sse_stream_docker));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

fn payload(tag: &str, source: EventSource, event: &str) -> EventPayload {
    EventPayload {
        version: "1".into(),
        source,
        event: event.into(),
        timestamp: "2026-11-08T00:00:00Z".into(),
        data: json!({ "tag": tag }),
    }
}

struct Frame {
    event: String,
    data: Value,
}

struct Stream {
    response: reqwest::Response,
    buf: String,
}

impl Stream {
    /// Connects and waits for the headers, so the handler has subscribed.
    async fn open(addr: SocketAddr, path: &str) -> Self {
        let response = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
        assert_eq!(response.status(), 200);
        Self {
            response,
            buf: String::new(),
        }
    }

    /// The next frame carrying `tag`, skipping comments and other tests' frames.
    async fn next(&mut self, tag: &str) -> Frame {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                while let Some(end) = self.buf.find("\n\n") {
                    let raw: String = self.buf.drain(..end + 2).collect();
                    let mut event = String::new();
                    let mut data = String::new();
                    for line in raw.lines() {
                        if let Some(v) = line.strip_prefix("event: ") {
                            event = v.to_owned();
                        } else if let Some(v) = line.strip_prefix("data: ") {
                            data.push_str(v);
                        }
                    }
                    if data.is_empty() {
                        continue;
                    }
                    let data: Value = serde_json::from_str(&data).unwrap();
                    if data["data"]["tag"] == tag {
                        return Frame { event, data };
                    }
                }
                let chunk = self.response.chunk().await.unwrap().expect("stream ended");
                self.buf.push_str(std::str::from_utf8(&chunk).unwrap());
            }
        })
        .await
        .expect("no tagged frame within 5 s")
    }
}

const SOURCES: [(EventSource, &str, &str); 5] = [
    (
        EventSource::ScreenCapture,
        "/sse/screen-capture",
        "t.screen",
    ),
    (
        EventSource::ProcessTracker,
        "/sse/process-tracker",
        "t.process",
    ),
    (
        EventSource::SystemResources,
        "/sse/system-resources",
        "t.resources",
    ),
    (EventSource::Systemd, "/sse/systemd", "t.systemd"),
    (
        EventSource::DockerTracker,
        "/sse/docker-tracker",
        "t.docker",
    ),
];

// ───────────────────────── Tests ─────────────────────────

#[tokio::test]
async fn sse_sends_an_event_stream_named_after_the_event() {
    let addr = serve().await;
    let mut s = Stream::open(addr, "/sse").await;
    assert_eq!(
        s.response.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );

    publish(payload(
        "named",
        EventSource::Systemd,
        "systemd.unit_failed",
    ));
    let f = s.next("named").await;
    assert_eq!(f.event, "systemd.unit_failed");
    // `source` is not serialised (finding 1).
    assert_eq!(
        f.data,
        json!({
            "version": "1",
            "event": "systemd.unit_failed",
            "timestamp": "2026-11-08T00:00:00Z",
            "data": { "tag": "named" },
        })
    );
}

#[tokio::test]
async fn ticks_are_skipped_by_default() {
    let addr = serve().await;
    let mut s = Stream::open(addr, "/sse").await;
    publish(payload(
        "noticks",
        EventSource::SystemResources,
        "resources.tick",
    ));
    publish(payload("noticks", EventSource::Systemd, "systemd.tick"));
    publish(payload("noticks", EventSource::Systemd, "systemd.after"));
    assert_eq!(s.next("noticks").await.event, "systemd.after");
}

#[tokio::test]
async fn ticks_true_includes_ticks() {
    let addr = serve().await;
    let mut s = Stream::open(addr, "/sse?ticks=true").await;
    publish(payload(
        "ticks",
        EventSource::SystemResources,
        "resources.tick",
    ));
    assert_eq!(s.next("ticks").await.event, "resources.tick");
}

#[tokio::test]
async fn ticks_false_is_the_same_as_leaving_it_out() {
    let addr = serve().await;
    let mut s = Stream::open(addr, "/sse/systemd?ticks=false").await;
    publish(payload("tickfalse", EventSource::Systemd, "systemd.tick"));
    publish(payload("tickfalse", EventSource::Systemd, "systemd.after"));
    assert_eq!(s.next("tickfalse").await.event, "systemd.after");
}

#[tokio::test]
async fn ticks_only_accepts_true_or_false() {
    // Pinned: `1`, `yes` and an empty value are rejected, not read as true.
    let addr = serve().await;
    for q in ["ticks=1", "ticks=yes", "ticks="] {
        let r = reqwest::get(format!("http://{addr}/sse?{q}"))
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "{q}");
    }
}

#[tokio::test]
async fn unknown_query_keys_are_ignored() {
    let addr = serve().await;
    let r = reqwest::get(format!("http://{addr}/sse?foo=bar"))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test]
async fn each_source_route_only_sends_its_own_source() {
    let addr = serve().await;
    for (source, path, event) in SOURCES {
        let tag = format!("route{path}");
        let mut s = Stream::open(addr, path).await;
        // Every other source first, then this one: the first frame must be ours.
        for (other, _, other_event) in SOURCES {
            if other != source {
                publish(payload(&tag, other, other_event));
            }
        }
        publish(payload(&tag, source, event));
        assert_eq!(s.next(&tag).await.event, event, "{path}");
    }
}

#[tokio::test]
async fn sse_sends_every_source() {
    let addr = serve().await;
    let mut s = Stream::open(addr, "/sse").await;
    for (source, _, event) in SOURCES {
        publish(payload("all", source, event));
    }
    for (_, _, event) in SOURCES {
        assert_eq!(s.next("all").await.event, event);
    }
}

#[tokio::test]
async fn every_client_gets_every_event() {
    let addr = serve().await;
    let mut a = Stream::open(addr, "/sse").await;
    let mut b = Stream::open(addr, "/sse/docker-tracker").await;
    publish(payload("fanout", EventSource::DockerTracker, "t.docker"));
    assert_eq!(a.next("fanout").await.event, "t.docker");
    assert_eq!(b.next("fanout").await.event, "t.docker");
}

#[tokio::test]
async fn the_dispatcher_returns_at_once_with_no_tracker_running() {
    // No `init_*` runs in unit tests, so every event sender is unset.
    tokio::time::timeout(
        Duration::from_secs(5),
        run_dispatcher(CancellationToken::new()),
    )
    .await
    .expect("run_dispatcher should return when nothing can send events");
}
