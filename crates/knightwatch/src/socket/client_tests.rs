#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::default_trait_access
)]

use std::time::Duration;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    time::timeout,
};

use kw_types::socket::{SocketCommandResponse, SocketMessage, SocketQueryResponse};

use super::{
    client::{Client, ClientConnection, ClientId, send_message_to_client},
    message::CorrelatedSocketMessage,
    transport::Transport,
};
use crate::errors::Error;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn within<F: Future>(fut: F) -> F::Output {
    timeout(Duration::from_secs(5), fut)
        .await
        .expect("timed out after 5s")
}

fn json(msg: &SocketMessage) -> serde_json::Value {
    serde_json::to_value(msg).unwrap()
}

/// A connection whose writer side is the returned receiver.
async fn fake_connection() -> (ClientConnection, mpsc::Receiver<CorrelatedSocketMessage>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (server, peer) = tokio::join!(listener.accept(), TcpStream::connect(addr));
    let _peer = peer.unwrap();
    let (reader, writer) = tokio::io::split(Transport::new_tcp(server.unwrap().0));
    let (reader_shutdown_tx, _reader_shutdown_rx) = oneshot::channel();
    let (writer_shutdown_tx, _writer_shutdown_rx) = oneshot::channel();
    let (message_writer_tx, message_writer_rx) = mpsc::channel(8);
    let connection = ClientConnection {
        reader_shutdown_tx,
        writer_shutdown_tx,
        reader_handle: tokio::spawn(async move { reader }),
        writer_handle: tokio::spawn(async move { writer }),
        message_writer_tx,
    };
    (connection, message_writer_rx)
}

async fn new_client() -> (Client, mpsc::Receiver<CorrelatedSocketMessage>) {
    let (connection, rx) = fake_connection().await;
    (Client::new(ClientId::new_v4(), connection), rx)
}

/// Plays the writer for one message: receives it, answers with `reply`,
/// returns the message.
async fn expect_message(
    rx: &mut mpsc::Receiver<CorrelatedSocketMessage>,
    reply: Result<(), Error>,
) -> SocketMessage {
    let CorrelatedSocketMessage {
        message,
        response_tx,
    } = rx.recv().await.expect("writer channel closed");
    let _ = response_tx.send(reply);
    message
}

fn socket_error_text(result: Result<(), Error>) -> String {
    match result {
        Err(Error::Socket(text)) => text,
        other => panic!("expected Error::Socket, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Client: state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn new_client_defaults() {
    let (client, _rx) = new_client().await;
    assert!(!client.is_authenticated());
    assert!(client.events_enabled());
    assert!(!client.ticks_enabled());
    assert!(client.get_display_user().is_none());
}

#[tokio::test]
async fn authentication_flag_toggles() {
    let (mut client, _rx) = new_client().await;
    client.set_authentication(true);
    assert!(client.is_authenticated());
    client.set_authentication(false);
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn event_and_tick_preferences_are_independent() {
    let (mut client, _rx) = new_client().await;
    client.set_events_enabled(false);
    assert!(!client.events_enabled());
    assert!(!client.ticks_enabled());
    client.set_ticks_enabled(true);
    assert!(client.ticks_enabled());
    assert!(!client.events_enabled());
    client.set_events_enabled(true);
    client.set_ticks_enabled(false);
    assert!(client.events_enabled());
    assert!(!client.ticks_enabled());
}

#[tokio::test]
async fn clearing_a_missing_display_user_is_harmless() {
    let (mut client, _rx) = new_client().await;
    client.clear_display_user();
    assert!(client.get_display_user().is_none());
}

#[tokio::test]
async fn take_connection_gives_it_once() {
    let (mut client, _rx) = new_client().await;
    assert!(client.take_connection().is_some());
    assert!(client.take_connection().is_none());
}

#[tokio::test]
async fn taken_connection_is_still_usable() {
    let (mut client, mut rx) = new_client().await;
    let taken = client.take_connection().unwrap();
    let (result, message) = within(async {
        tokio::join!(
            send_message_to_client(&taken.message_writer_tx, SocketMessage::Handshake),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    assert_eq!(json(&message), json(&SocketMessage::Handshake));
    // the join handles return the halves they were given
    within(taken.reader_handle).await.unwrap();
    within(taken.writer_handle).await.unwrap();
}

// ---------------------------------------------------------------------------
// Client: sending
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_without_connection_is_socket_error() {
    let (mut client, _rx) = new_client().await;
    drop(client.take_connection());
    let text = socket_error_text(client.send_message(SocketMessage::Handshake).await);
    assert_eq!(text, "Client message_writer_tx is None");
}

#[tokio::test]
async fn send_message_delivers_message_to_writer() {
    let (client, mut rx) = new_client().await;
    let (result, message) = within(async {
        tokio::join!(
            client.send_message(SocketMessage::Unauthorized),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    assert_eq!(json(&message), json(&SocketMessage::Unauthorized));
}

#[tokio::test]
async fn send_message_returns_the_writers_error() {
    let (client, mut rx) = new_client().await;
    let reply = Err(Error::Socket("Connection error: broken pipe".into()));
    let (result, _) = within(async {
        tokio::join!(
            client.send_message(SocketMessage::Handshake),
            expect_message(&mut rx, reply),
        )
    })
    .await;
    assert_eq!(socket_error_text(result), "Connection error: broken pipe");
}

#[tokio::test]
async fn send_message_fails_when_writer_is_gone() {
    let (client, rx) = new_client().await;
    drop(rx);
    let text = socket_error_text(within(client.send_message(SocketMessage::Handshake)).await);
    assert_eq!(text, "Failed to send message to client");
}

#[tokio::test]
async fn respond_to_query_wraps_response_in_query_response() {
    let (client, mut rx) = new_client().await;
    let response = SocketQueryResponse::Disks {
        snapshots: Default::default(),
    };
    let (result, message) = within(async {
        tokio::join!(
            client.respond_to_query(response),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    let value = json(&message);
    assert!(
        value["QueryResponse"]["response"]["Disks"].is_object(),
        "unexpected shape: {value}"
    );
}

#[tokio::test]
async fn respond_to_command_ok_reports_success() {
    let (client, mut rx) = new_client().await;
    let (result, message) = within(async {
        tokio::join!(
            client.respond_to_command(Ok(()), SocketCommandResponse::TrackPid),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    let value = json(&message);
    assert_eq!(value["CommandResponse"]["success"], true, "{value}");
    assert!(value["CommandResponse"]["err"].is_null(), "{value}");
    assert_eq!(value["CommandResponse"]["response"], "TrackPid", "{value}");
}

#[tokio::test]
async fn respond_to_command_error_carries_the_error_text() {
    let (client, mut rx) = new_client().await;
    let failure = Err(Error::Other("boom".into()));
    let (result, message) = within(async {
        tokio::join!(
            client.respond_to_command(failure, SocketCommandResponse::UntrackPid),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    let value = json(&message);
    assert_eq!(value["CommandResponse"]["success"], false, "{value}");
    assert_eq!(value["CommandResponse"]["err"], "boom", "{value}");
    assert_eq!(
        value["CommandResponse"]["response"], "UntrackPid",
        "{value}"
    );
}

#[tokio::test]
async fn respond_to_command_without_connection_is_socket_error() {
    let (mut client, _rx) = new_client().await;
    drop(client.take_connection());
    let text = socket_error_text(
        client
            .respond_to_command(Ok(()), SocketCommandResponse::TrackPid)
            .await,
    );
    assert_eq!(text, "Client message_writer_tx is None");
}

// ---------------------------------------------------------------------------
// send_message_to_client
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_to_client_ok_when_writer_confirms() {
    let (tx, mut rx) = mpsc::channel(8);
    let (result, message) = within(async {
        tokio::join!(
            send_message_to_client(&tx, SocketMessage::HandshakeResponse),
            expect_message(&mut rx, Ok(())),
        )
    })
    .await;
    result.unwrap();
    assert_eq!(json(&message), json(&SocketMessage::HandshakeResponse));
}

#[tokio::test]
async fn send_to_client_passes_writer_error_through() {
    let (tx, mut rx) = mpsc::channel(8);
    let reply = Err(Error::Other("serialize failed".into()));
    let (result, _) = within(async {
        tokio::join!(
            send_message_to_client(&tx, SocketMessage::Handshake),
            expect_message(&mut rx, reply),
        )
    })
    .await;
    match result {
        Err(Error::Other(text)) => assert_eq!(text, "serialize failed"),
        other => panic!("expected Error::Other, got {other:?}"),
    }
}

#[tokio::test]
async fn send_to_client_closed_channel_is_socket_error() {
    let (tx, rx) = mpsc::channel(8);
    drop(rx);
    let text =
        socket_error_text(within(send_message_to_client(&tx, SocketMessage::Handshake)).await);
    assert_eq!(text, "Failed to send message to client");
}

#[tokio::test]
async fn send_to_client_dropped_reply_is_socket_error() {
    let (tx, mut rx) = mpsc::channel(8);
    let writer = async {
        let correlated: CorrelatedSocketMessage = rx.recv().await.expect("no message");
        drop(correlated.response_tx);
    };
    let (result, ()) = within(async {
        tokio::join!(
            send_message_to_client(&tx, SocketMessage::Handshake),
            writer,
        )
    })
    .await;
    assert_eq!(
        socket_error_text(result),
        "Response channel closed for client"
    );
}

#[tokio::test]
async fn send_to_client_waits_for_the_writers_reply() {
    let (tx, mut rx) = mpsc::channel(8);
    let sender =
        tokio::spawn(async move { send_message_to_client(&tx, SocketMessage::Handshake).await });
    let correlated = within(rx.recv()).await.expect("no message");
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    assert!(
        !sender.is_finished(),
        "send returned before the writer replied"
    );
    let _ = correlated.response_tx.send(Ok(()));
    within(sender).await.unwrap().unwrap();
}

#[tokio::test]
async fn send_to_client_keeps_message_order() {
    let (tx, mut rx) = mpsc::channel(8);
    let writer = async {
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(json(&expect_message(&mut rx, Ok(())).await));
        }
        seen
    };
    let (a, b, c, seen) = within(async {
        tokio::join!(
            send_message_to_client(&tx, SocketMessage::Handshake),
            send_message_to_client(&tx, SocketMessage::Unauthorized),
            send_message_to_client(&tx, SocketMessage::HandshakeResponse),
            writer,
        )
    })
    .await;
    a.unwrap();
    b.unwrap();
    c.unwrap();
    assert_eq!(
        seen,
        vec![
            json(&SocketMessage::Handshake),
            json(&SocketMessage::Unauthorized),
            json(&SocketMessage::HandshakeResponse),
        ]
    );
}
