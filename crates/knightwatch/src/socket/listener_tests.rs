#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::time::Duration;

use kw_types::socket::{Frame, SocketAction, SocketMessage};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};

use super::handle_client;
use crate::{
    prelude::*,
    socket::{
        framing::{receive_message, send_message},
        transport::Transport,
    },
};

// ───────────────────────── Helpers ─────────────────────────

/// Runs `handle_client` on the server end of a real TCP pair.
async fn handshake() -> (JoinHandle<Result<()>>, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (server, _) = listener.accept().await.unwrap();
    let task = tokio::spawn(async move {
        let mut transport = Transport::new_tcp(server);
        handle_client(&mut transport).await
    });
    (task, client)
}

async fn within<F: Future>(fut: F) -> F::Output {
    timeout(Duration::from_secs(5), fut)
        .await
        .expect("timed out")
}

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

// ───────────────────────── handle_client ─────────────────────────

#[tokio::test]
async fn server_speaks_first_with_a_handshake_push() {
    let (_task, mut client) = handshake().await;
    let first: Frame = within(receive_message(&mut client)).await.unwrap();
    assert_eq!(json(&first), json(&Frame::push(SocketMessage::Handshake)));
}

#[tokio::test]
async fn a_handshake_response_completes_it() {
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    send_message(&mut client, &Frame::push(SocketMessage::HandshakeResponse))
        .await
        .unwrap();
    within(task).await.unwrap().unwrap();
}

#[tokio::test]
async fn the_response_id_is_ignored() {
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    let reply = Frame::reply(Some(42), SocketMessage::HandshakeResponse);
    send_message(&mut client, &reply).await.unwrap();
    within(task).await.unwrap().unwrap();
}

#[tokio::test]
async fn any_other_first_message_is_rejected() {
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    let login = SocketMessage::Action {
        action: SocketAction::Logout,
    };
    send_message(&mut client, &Frame::push(login))
        .await
        .unwrap();
    let err = within(task).await.unwrap().unwrap_err();
    assert!(matches!(err, Error::Other(_)), "{err:?}");
    assert!(err.to_string().contains("HandshakeResponse"), "{err}");
}

#[tokio::test]
async fn echoing_the_handshake_is_rejected() {
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    send_message(&mut client, &Frame::push(SocketMessage::Handshake))
        .await
        .unwrap();
    assert!(within(task).await.unwrap().is_err());
}

#[tokio::test]
async fn a_bare_message_without_a_frame_is_rejected() {
    // Pre-request-id clients sent `SocketMessage` directly.
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    send_message(&mut client, &SocketMessage::HandshakeResponse)
        .await
        .unwrap();
    assert!(within(task).await.unwrap().is_err());
}

#[tokio::test]
async fn a_client_closing_before_replying_is_an_error() {
    let (task, mut client) = handshake().await;
    let _: Frame = within(receive_message(&mut client)).await.unwrap();
    drop(client);
    assert!(within(task).await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
#[ignore = "known issue (finding 16): handle_client waits forever for the handshake response"]
async fn a_silent_client_should_time_out() {
    let (task, _client) = handshake().await;
    // Paused time jumps ahead while the server waits.
    let result = timeout(Duration::from_mins(5), task).await;
    assert!(
        result.is_ok(),
        "handle_client should give up on a silent client"
    );
}
