#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::time::Duration;

use tokio::{
    io::{AsyncWriteExt, DuplexStream, duplex},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};

use kw_types::socket::{Frame, SocketAction, SocketCommand, SocketMessage, SocketQuery};

use super::{
    client::ClientId,
    client_manager::spawn_client_reader,
    event::ClientManagerEvent,
    framing::{MAX_FRAME_LEN, send_message},
    message::{RequestKind, SocketRequest},
};

struct Rig {
    peer: DuplexStream,
    requests: mpsc::Receiver<SocketRequest>,
    events: mpsc::Receiver<ClientManagerEvent>,
    written: mpsc::Receiver<Frame>,
    handle: JoinHandle<tokio::io::ReadHalf<DuplexStream>>,
    client_id: ClientId,
    _shutdown: oneshot::Sender<()>,
}

fn rig() -> Rig {
    let (peer, near) = duplex(1 << 16);
    // `w` keeps the duplex write half open
    let (reader, w) = tokio::io::split(near);
    let (req_tx, requests) = mpsc::channel(16);
    let (ev_tx, events) = mpsc::channel(16);
    let (writer_tx, mut writer_rx) = mpsc::channel::<super::message::CorrelatedSocketMessage>(16);
    let (out_tx, written) = mpsc::channel(16);
    tokio::spawn(async move {
        let _keep = w;
        while let Some(m) = writer_rx.recv().await {
            let _ = out_tx.send(m.frame).await;
            if let Some(tx) = m.response_tx {
                let _ = tx.send(Ok(()));
            }
        }
    });
    let (shutdown, shutdown_rx) = oneshot::channel();
    let client_id = ClientId::new_v4();
    let handle = spawn_client_reader(client_id, reader, req_tx, ev_tx, writer_tx, shutdown_rx);
    Rig {
        peer,
        requests,
        events,
        written,
        handle,
        client_id,
        _shutdown: shutdown,
    }
}

async fn next<T>(rx: &mut mpsc::Receiver<T>) -> T {
    timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn requests_keep_send_order_across_kinds() {
    let mut r = rig();
    let login = SocketAction::Login {
        username: "u".into(),
        password: "p".into(),
    };
    let frames = [
        Frame::reply(Some(1), SocketMessage::Action { action: login }),
        Frame::reply(
            Some(2),
            SocketMessage::Command {
                command: SocketCommand::TrackPid { pid: 1 },
            },
        ),
        Frame::reply(
            Some(3),
            SocketMessage::Query {
                query: SocketQuery::Info,
            },
        ),
    ];
    for f in &frames {
        send_message(&mut r.peer, f).await.unwrap();
    }
    let a = next(&mut r.requests).await;
    let b = next(&mut r.requests).await;
    let c = next(&mut r.requests).await;
    assert_eq!(a.client_id, r.client_id);
    assert_eq!((a.id, b.id, c.id), (Some(1), Some(2), Some(3)));
    assert!(matches!(a.kind, RequestKind::Action(_)));
    assert!(matches!(b.kind, RequestKind::Command(_)));
    assert!(matches!(c.kind, RequestKind::Query(_)));
}

#[tokio::test]
async fn bad_json_gets_invalid_message_and_reader_continues() {
    let mut r = rig();
    let bad = b"{not json";
    r.peer
        .write_all(&(bad.len() as u32).to_be_bytes())
        .await
        .unwrap();
    r.peer.write_all(bad).await.unwrap();
    let q = Frame::reply(
        Some(4),
        SocketMessage::Query {
            query: SocketQuery::Info,
        },
    );
    send_message(&mut r.peer, &q).await.unwrap();
    let reply = next(&mut r.written).await;
    assert_eq!(reply.id, None);
    assert!(matches!(reply.message, SocketMessage::InvalidMessage));
    assert_eq!(next(&mut r.requests).await.id, Some(4));
}

#[tokio::test]
async fn non_request_message_gets_invalid_message_with_its_id() {
    let mut r = rig();
    send_message(
        &mut r.peer,
        &Frame::reply(Some(5), SocketMessage::Handshake),
    )
    .await
    .unwrap();
    let reply = next(&mut r.written).await;
    assert_eq!(reply.id, Some(5));
    assert!(matches!(reply.message, SocketMessage::InvalidMessage));
}

#[tokio::test]
async fn oversized_frame_disconnects() {
    let mut r = rig();
    r.peer
        .write_all(&(u32::try_from(MAX_FRAME_LEN + 1).unwrap()).to_be_bytes())
        .await
        .unwrap();
    let ev = next(&mut r.events).await;
    assert!(
        matches!(ev, ClientManagerEvent::ClientDisconnected { client_id } if client_id == r.client_id)
    );
    timeout(Duration::from_secs(2), r.handle)
        .await
        .unwrap()
        .unwrap();
}
