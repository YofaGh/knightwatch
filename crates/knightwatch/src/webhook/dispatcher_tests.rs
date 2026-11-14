#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use reqwest::Client;
use serde_json::{Value, json};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use kw_types::event::{EventPayload, EventSource};

use super::{fire_with_retry, run_dispatcher};

// ───────────────────────── Helpers ─────────────────────────

struct Received {
    at: Instant,
    head: String,
    body: String,
}

impl Received {
    fn request_line(&self) -> &str {
        self.head.lines().next().unwrap_or_default()
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

/// Answers each request with the next status (the last one repeats) and
/// reports what it received. Every reply closes the connection.
async fn server(statuses: &[u16]) -> (SocketAddr, mpsc::UnboundedReceiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let statuses = statuses.to_vec();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        for n in 0.. {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let split = loop {
                let read = stream.read(&mut chunk).await.unwrap();
                assert!(read > 0, "the client closed before sending headers");
                buf.extend_from_slice(&chunk[..read]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i;
                }
            };
            let head = String::from_utf8(buf[..split].to_vec()).unwrap();
            let len: usize = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap_or(0);
            let mut body = buf[split + 4..].to_vec();
            while body.len() < len {
                let read = stream.read(&mut chunk).await.unwrap();
                body.extend_from_slice(&chunk[..read]);
            }
            let status = statuses[n.min(statuses.len() - 1)];
            let reply =
                format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            stream.write_all(reply.as_bytes()).await.unwrap();
            let _ = tx.send(Received {
                at: Instant::now(),
                head,
                body: String::from_utf8(body).unwrap(),
            });
        }
    });
    (addr, rx)
}

fn payload() -> EventPayload {
    EventPayload {
        version: "1".into(),
        source: EventSource::SystemResources,
        event: "resources.alarm".into(),
        timestamp: "2026-11-08T00:00:00Z".into(),
        data: json!({ "cpu": 99 }),
    }
}

fn drain(rx: &mut mpsc::UnboundedReceiver<Received>) -> Vec<Received> {
    let mut all = Vec::new();
    while let Ok(r) = rx.try_recv() {
        all.push(r);
    }
    all
}

async fn fire(url: &str, cancel: &CancellationToken) {
    fire_with_retry(&Client::new(), url, &payload(), cancel).await;
}

// ───────────────────────── fire_with_retry ─────────────────────────

#[tokio::test]
async fn a_2xx_is_sent_once_as_json() {
    let (addr, mut rx) = server(&[204]).await;
    fire(
        &format!("http://{addr}/hook?a=1"),
        &CancellationToken::new(),
    )
    .await;

    let got = drain(&mut rx);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].request_line(), "POST /hook?a=1 HTTP/1.1");
    assert_eq!(got[0].header("content-type"), Some("application/json"));
    let body: Value = serde_json::from_str(&got[0].body).unwrap();
    assert_eq!(
        body,
        json!({
            "version": "1",
            "event": "resources.alarm",
            "timestamp": "2026-11-08T00:00:00Z",
            "data": { "cpu": 99 },
        })
    );
}

#[tokio::test(start_paused = true)]
async fn a_failing_target_gets_three_attempts_with_2s_then_4s_backoff() {
    let (addr, mut rx) = server(&[500]).await;
    fire(&format!("http://{addr}/"), &CancellationToken::new()).await;

    let got = drain(&mut rx);
    assert_eq!(got.len(), 3);
    assert!(got[1].at - got[0].at >= Duration::from_secs(2));
    assert!(got[2].at - got[1].at >= Duration::from_secs(4));
}

#[tokio::test(start_paused = true)]
async fn a_success_after_a_failure_stops_retrying() {
    let (addr, mut rx) = server(&[503, 200]).await;
    fire(&format!("http://{addr}/"), &CancellationToken::new()).await;
    assert_eq!(drain(&mut rx).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_redirect_is_followed_and_not_a_failure() {
    // reqwest follows redirects by default, so 3xx with a location is fine.
    let (target, mut target_rx) = server(&[200]).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf).await.unwrap();
        let reply = format!(
            "HTTP/1.1 307 X\r\nlocation: http://{target}/moved\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        s.write_all(reply.as_bytes()).await.unwrap();
    });
    fire(&format!("http://{addr}/"), &CancellationToken::new()).await;

    let got = drain(&mut target_rx);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].request_line(), "POST /moved HTTP/1.1");
}

#[tokio::test(start_paused = true)]
async fn an_unreachable_target_gives_up_after_three_attempts() {
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let start = Instant::now();
    fire(&format!("http://{addr}/"), &CancellationToken::new()).await;
    // Two backoff sleeps happened: 2s + 4s.
    assert!(Instant::now() - start >= Duration::from_secs(6));
}

#[tokio::test]
async fn an_already_cancelled_token_sends_nothing() {
    let (addr, mut rx) = server(&[200]).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    fire(&format!("http://{addr}/"), &cancel).await;
    tokio::task::yield_now().await;
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn cancelling_during_backoff_returns_at_once() {
    let (addr, mut rx) = server(&[500]).await;
    let cancel = CancellationToken::new();
    let url = format!("http://{addr}/");
    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { fire(&url, &cancel).await }
    });

    let first = rx.recv().await.unwrap();
    cancel.cancel();
    task.await.unwrap();

    assert!(Instant::now() - first.at < Duration::from_secs(2));
    assert!(drain(&mut rx).is_empty());
}

// ───────────────────────── run_dispatcher ─────────────────────────

#[tokio::test]
async fn the_dispatcher_returns_at_once_with_no_tracker_running() {
    // No `init_*` runs in unit tests, so every event sender is unset.
    tokio::time::timeout(
        Duration::from_secs(5),
        run_dispatcher(Vec::new(), CancellationToken::new()),
    )
    .await
    .expect("run_dispatcher should return when nothing can send events");
}
