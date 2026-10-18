#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::Role},
};

use kw_types::socket::{AuthFailedReason, SocketMessage, SocketQuery};

use super::{framing, transport::Transport, ws_adapter::WebSocketAdapter};
use crate::errors::Error;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type Ws<S> = WebSocketStream<S>;
type Adapter = WebSocketAdapter<DuplexStream>;

/// Server side wrapped in the adapter, client side as a plain websocket.
async fn ws_pair() -> (Adapter, Ws<DuplexStream>) {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    let (server, client) = tokio::join!(
        WebSocketStream::from_raw_socket(server_io, Role::Server, None),
        WebSocketStream::from_raw_socket(client_io, Role::Client, None),
    );
    (WebSocketAdapter::new(server), client)
}

async fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (server, client) = tokio::join!(listener.accept(), TcpStream::connect(addr));
    (server.unwrap().0, client.unwrap())
}

/// Server side as a websocket `Transport`, client side as a plain websocket,
/// connected over real loopback TCP with a real upgrade.
async fn ws_over_tcp() -> (Transport, Ws<TcpStream>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}/");
    let server = async {
        let (stream, _) = listener.accept().await.unwrap();
        tokio_tungstenite::accept_async(stream).await.unwrap()
    };
    let client = async {
        let stream = TcpStream::connect(addr).await.unwrap();
        tokio_tungstenite::client_async(url, stream)
            .await
            .unwrap()
            .0
    };
    let (server, client) = tokio::join!(server, client);
    (Transport::new_websocket(server), client)
}

async fn within<F: Future>(fut: F) -> F::Output {
    timeout(Duration::from_secs(5), fut)
        .await
        .expect("timed out after 5s")
}

fn bin(bytes: &[u8]) -> Message {
    Message::Binary(bytes.to_vec().into())
}

fn json(msg: &SocketMessage) -> serde_json::Value {
    serde_json::to_value(msg).unwrap()
}

/// Wire bytes of one frame: 4-byte big-endian length, then the JSON body.
fn frame(msg: &SocketMessage) -> Vec<u8> {
    let body = serde_json::to_vec(msg).unwrap();
    let mut out = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

fn sample_messages() -> Vec<SocketMessage> {
    vec![
        SocketMessage::Handshake,
        SocketMessage::HandshakeResponse,
        SocketMessage::Unauthorized,
        SocketMessage::AuthenticationFailed {
            reason: AuthFailedReason::WrongCredentials,
        },
        SocketMessage::Query {
            query: SocketQuery::Info,
        },
    ]
}

async fn next_binary<S>(client: &mut Ws<S>) -> Vec<u8>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match client.next().await {
        Some(Ok(Message::Binary(data))) => data.to_vec(),
        other => panic!("expected a binary message, got {other:?}"),
    }
}

/// Polls `poll_read` exactly once with a waker that does nothing, so a missing
/// wakeup shows up as `Pending` instead of a hang.
fn poll_read_once<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut [u8]) -> Poll<io::Result<usize>> {
    let mut cx = Context::from_waker(Waker::noop());
    let mut read_buf = ReadBuf::new(buf);
    match Pin::new(reader).poll_read(&mut cx, &mut read_buf) {
        Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
        Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
        Poll::Pending => Poll::Pending,
    }
}

fn control_messages() -> Vec<(&'static str, Message)> {
    vec![
        ("text", Message::Text("hello".into())),
        ("ping", Message::Ping(vec![1, 2].into())),
        ("pong", Message::Pong(vec![3].into())),
    ]
}

// ---------------------------------------------------------------------------
// WebSocketAdapter: reading
// ---------------------------------------------------------------------------

#[tokio::test]
async fn read_single_binary_message() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[1, 2, 3])).await.unwrap();
    let mut buf = [0u8; 16];
    let n = within(server.read(&mut buf)).await.unwrap();
    assert_eq!(&buf[..n], &[1, 2, 3]);
}

#[tokio::test]
async fn read_returns_at_most_one_message_per_call() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[1, 2])).await.unwrap();
    client.send(bin(&[3, 4, 5])).await.unwrap();
    let mut buf = [0u8; 16];
    let first = within(server.read(&mut buf)).await.unwrap();
    assert_eq!(&buf[..first], &[1, 2]);
    let second = within(server.read(&mut buf)).await.unwrap();
    assert_eq!(&buf[..second], &[3, 4, 5]);
}

#[tokio::test]
async fn read_exact_spans_message_boundaries() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[1, 2])).await.unwrap();
    client.send(bin(&[3, 4, 5])).await.unwrap();
    let mut buf = [0u8; 5];
    within(server.read_exact(&mut buf)).await.unwrap();
    assert_eq!(buf, [1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn small_reads_drain_one_message_in_pieces() {
    let (mut server, mut client) = ws_pair().await;
    client
        .send(bin(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]))
        .await
        .unwrap();
    let mut buf = [0u8; 4];
    let mut got = Vec::new();
    let mut sizes = Vec::new();
    while got.len() < 10 {
        let n = within(server.read(&mut buf)).await.unwrap();
        sizes.push(n);
        got.extend_from_slice(&buf[..n]);
    }
    assert_eq!(got, (0..10u8).collect::<Vec<_>>());
    assert_eq!(sizes, vec![4, 4, 2]);
}

#[tokio::test]
async fn buffered_remainder_is_served_before_next_message() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[1, 2, 3, 4, 5, 6])).await.unwrap();
    client.send(bin(&[9])).await.unwrap();
    let mut first = [0u8; 4];
    within(server.read_exact(&mut first)).await.unwrap();
    assert_eq!(first, [1, 2, 3, 4]);
    let mut second = [0u8; 3];
    within(server.read_exact(&mut second)).await.unwrap();
    assert_eq!(second, [5, 6, 9]);
}

// Pinned: an empty binary message completes a read with 0 bytes, which every
// reader treats as end of stream. Should flip (skip the empty message and keep
// reading) if the adapter is fixed. See finding 15.
#[tokio::test]
async fn empty_binary_message_reads_as_eof() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[])).await.unwrap();
    client.send(bin(&[5])).await.unwrap();
    let mut buf = [0u8; 4];
    let n = within(server.read(&mut buf)).await.unwrap();
    assert_eq!(n, 0);
}

// Pinned, same cause: `read_exact` (what `receive_message` uses) fails.
#[tokio::test]
async fn read_exact_on_empty_binary_message_fails() {
    let (mut server, mut client) = ws_pair().await;
    client.send(bin(&[])).await.unwrap();
    let mut buf = [0u8; 1];
    let err = within(server.read_exact(&mut buf)).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
}

#[tokio::test]
async fn close_message_is_connection_aborted() {
    let (mut server, mut client) = ws_pair().await;
    client.send(Message::Close(None)).await.unwrap();
    let mut buf = [0u8; 4];
    let err = within(server.read(&mut buf)).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::ConnectionAborted);
}

// The exact error kind for a dropped peer depends on the tungstenite version
// (UnexpectedEof from the adapter, or a wrapped protocol error), so only the
// fact that it is an error is checked.
#[tokio::test]
async fn peer_drop_is_a_read_error() {
    let (mut server, client) = ws_pair().await;
    drop(client);
    let mut buf = [0u8; 4];
    assert!(within(server.read(&mut buf)).await.is_err());
}

// Pinned (finding 14): after a text, ping or pong message `poll_read` returns
// `Pending` without having registered a wakeup, although more data is already
// waiting. It only makes progress if something else polls it again. Here the
// second poll finds the data. Should flip (first poll returns the data) when
// the adapter loops past non-binary messages.
#[tokio::test]
async fn non_binary_messages_stall_read_until_repolled() {
    for (name, skipped) in control_messages() {
        let (mut server, mut client) = ws_pair().await;
        client.send(skipped).await.unwrap();
        client.send(bin(&[7])).await.unwrap();
        let mut buf = [0u8; 4];
        assert!(
            poll_read_once(&mut server, &mut buf).is_pending(),
            "{name}: first poll should be Pending today"
        );
        match poll_read_once(&mut server, &mut buf) {
            Poll::Ready(Ok(n)) => assert_eq!(&buf[..n], &[7], "{name}"),
            other => panic!("{name}: second poll should find the data, got {other:?}"),
        }
    }
}

// Wanted behaviour for finding 14: skip non-binary messages and return the
// next binary data in the same poll (or at least register a wakeup).
#[tokio::test]
#[ignore = "known issue (finding 14): poll_read returns Pending without a wakeup after a text/ping/pong message"]
async fn non_binary_messages_should_not_stall_read() {
    for (name, skipped) in control_messages() {
        let (mut server, mut client) = ws_pair().await;
        client.send(skipped).await.unwrap();
        client.send(bin(&[7])).await.unwrap();
        let mut buf = [0u8; 4];
        match poll_read_once(&mut server, &mut buf) {
            Poll::Ready(Ok(n)) => assert_eq!(&buf[..n], &[7], "{name}"),
            other => panic!("{name}: expected the binary data right away, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocketAdapter: writing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn each_write_becomes_one_binary_message() {
    let (mut server, mut client) = ws_pair().await;
    server.write_all(b"ab").await.unwrap();
    server.write_all(b"cde").await.unwrap();
    server.flush().await.unwrap();
    assert_eq!(next_binary(&mut client).await, b"ab".to_vec());
    assert_eq!(next_binary(&mut client).await, b"cde".to_vec());
}

#[tokio::test]
async fn shutdown_sends_close_message() {
    let (mut server, mut client) = ws_pair().await;
    within(server.shutdown()).await.unwrap();
    let item = within(client.next()).await;
    assert!(
        matches!(item, Some(Ok(Message::Close(_))) | None),
        "expected a close message or end of stream, got {item:?}"
    );
}

#[tokio::test]
async fn write_after_peer_dropped_fails() {
    let (mut server, client) = ws_pair().await;
    drop(client);
    let result = within(async {
        server.write_all(b"x").await?;
        server.flush().await
    })
    .await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Framing on top of the adapter
// ---------------------------------------------------------------------------

// Pinned: `send_message` does two writes (prefix, then body) and the adapter
// turns every write into its own websocket message, so a websocket peer sees
// 4 bytes and then the JSON as two separate messages. A client must treat the
// websocket as a byte stream and reassemble. Should flip if the adapter
// buffers until flush.
#[tokio::test]
async fn framing_send_is_prefix_message_then_body_message() {
    let (mut server, mut client) = ws_pair().await;
    let msg = SocketMessage::Query {
        query: SocketQuery::Info,
    };
    within(framing::send_message(&mut server, &msg))
        .await
        .unwrap();
    let all = frame(&msg);
    assert_eq!(next_binary(&mut client).await, all[..4].to_vec());
    assert_eq!(next_binary(&mut client).await, all[4..].to_vec());
}

#[tokio::test]
async fn framing_receive_whole_frame_in_one_message() {
    for msg in sample_messages() {
        let (mut server, mut client) = ws_pair().await;
        client.send(bin(&frame(&msg))).await.unwrap();
        let got = within(framing::receive_message(&mut server)).await.unwrap();
        assert_eq!(json(&got), json(&msg));
    }
}

#[tokio::test]
async fn framing_receive_prefix_and_body_in_separate_messages() {
    for msg in sample_messages() {
        let (mut server, mut client) = ws_pair().await;
        let all = frame(&msg);
        client.send(bin(&all[..4])).await.unwrap();
        client.send(bin(&all[4..])).await.unwrap();
        let got = within(framing::receive_message(&mut server)).await.unwrap();
        assert_eq!(json(&got), json(&msg));
    }
}

#[tokio::test]
async fn framing_receive_two_frames_in_one_message() {
    let (mut server, mut client) = ws_pair().await;
    let first = SocketMessage::Handshake;
    let second = SocketMessage::Unauthorized;
    let mut both = frame(&first);
    both.extend_from_slice(&frame(&second));
    client.send(bin(&both)).await.unwrap();
    let got_first = within(framing::receive_message(&mut server)).await.unwrap();
    let got_second = within(framing::receive_message(&mut server)).await.unwrap();
    assert_eq!(json(&got_first), json(&first));
    assert_eq!(json(&got_second), json(&second));
}

#[tokio::test]
async fn framing_receive_one_byte_messages() {
    let (mut server, mut client) = ws_pair().await;
    let msg = SocketMessage::AuthenticationFailed {
        reason: AuthFailedReason::WrongCredentials,
    };
    for byte in frame(&msg) {
        client.send(bin(&[byte])).await.unwrap();
    }
    let got = within(framing::receive_message(&mut server)).await.unwrap();
    assert_eq!(json(&got), json(&msg));
}

#[tokio::test]
async fn framing_large_message_both_directions() {
    let (mut server, mut client) = ws_pair().await;
    let msg = SocketMessage::Query {
        query: SocketQuery::Unit {
            unit_name: "a".repeat(200_000),
        },
    };
    let expected = frame(&msg);

    // client -> server, whole frame in one websocket message
    client.send(bin(&expected)).await.unwrap();
    let got = within(framing::receive_message(&mut server)).await.unwrap();
    assert_eq!(json(&got), json(&msg));

    // server -> client, reassembled from however many messages arrive
    within(framing::send_message(&mut server, &msg))
        .await
        .unwrap();
    let mut received = Vec::new();
    while received.len() < expected.len() {
        received.extend_from_slice(&next_binary(&mut client).await);
    }
    assert_eq!(received, expected);
}

#[tokio::test]
async fn framing_receive_after_close_is_socket_error() {
    let (mut server, mut client) = ws_pair().await;
    client.send(Message::Close(None)).await.unwrap();
    match within(framing::receive_message(&mut server)).await {
        Err(Error::Socket(text)) => assert!(text.contains("Connection error"), "{text}"),
        other => panic!("expected Error::Socket, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Transport: TCP
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tcp_write_reaches_peer() {
    let (server, mut peer) = tcp_pair().await;
    let mut transport = Transport::new_tcp(server);
    transport.write_all(b"hello").await.unwrap();
    transport.flush().await.unwrap();
    let mut buf = [0u8; 5];
    within(peer.read_exact(&mut buf)).await.unwrap();
    assert_eq!(&buf, b"hello");
}

#[tokio::test]
async fn tcp_read_gets_peer_bytes() {
    let (server, mut peer) = tcp_pair().await;
    let mut transport = Transport::new_tcp(server);
    peer.write_all(b"world").await.unwrap();
    let mut buf = [0u8; 5];
    within(transport.read_exact(&mut buf)).await.unwrap();
    assert_eq!(&buf, b"world");
}

#[tokio::test]
async fn tcp_framing_round_trip_both_directions() {
    let (server, client) = tcp_pair().await;
    let mut a = Transport::new_tcp(server);
    let mut b = Transport::new_tcp(client);
    for msg in sample_messages() {
        within(framing::send_message(&mut a, &msg)).await.unwrap();
        let got = within(framing::receive_message(&mut b)).await.unwrap();
        assert_eq!(json(&got), json(&msg));

        within(framing::send_message(&mut b, &msg)).await.unwrap();
        let got = within(framing::receive_message(&mut a)).await.unwrap();
        assert_eq!(json(&got), json(&msg));
    }
}

#[tokio::test]
async fn tcp_close_connection_gives_peer_eof() {
    let (server, mut peer) = tcp_pair().await;
    let mut transport = Transport::new_tcp(server);
    within(framing::close_connection(&mut transport))
        .await
        .unwrap();
    let mut buf = [0u8; 4];
    let n = within(peer.read(&mut buf)).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn tcp_peer_drop_is_socket_error() {
    let (server, peer) = tcp_pair().await;
    let mut transport = Transport::new_tcp(server);
    drop(peer);
    match within(framing::receive_message(&mut transport)).await {
        Err(Error::Socket(text)) => assert!(text.contains("Connection error"), "{text}"),
        other => panic!("expected Error::Socket, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Transport: WebSocket (real upgrade over loopback TCP)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_transport_receives_frames() {
    let (mut transport, mut client) = ws_over_tcp().await;
    for msg in sample_messages() {
        client.send(bin(&frame(&msg))).await.unwrap();
        let got = within(framing::receive_message(&mut transport))
            .await
            .unwrap();
        assert_eq!(json(&got), json(&msg));
    }
}

// Pinned, same quirk as `framing_send_is_prefix_message_then_body_message`.
#[tokio::test]
async fn ws_transport_sends_prefix_and_body_as_two_messages() {
    let (mut transport, mut client) = ws_over_tcp().await;
    let msg = SocketMessage::Handshake;
    within(framing::send_message(&mut transport, &msg))
        .await
        .unwrap();
    let all = frame(&msg);
    assert_eq!(next_binary(&mut client).await, all[..4].to_vec());
    assert_eq!(next_binary(&mut client).await, all[4..].to_vec());
}

#[tokio::test]
async fn ws_transport_close_connection_sends_close() {
    let (mut transport, mut client) = ws_over_tcp().await;
    within(framing::close_connection(&mut transport))
        .await
        .unwrap();
    let item = within(client.next()).await;
    assert!(
        matches!(item, Some(Ok(Message::Close(_))) | None),
        "expected a close message or end of stream, got {item:?}"
    );
}

// Same exchange as `listener::handle_client`: server sends Handshake, client
// answers HandshakeResponse. (`handle_client` itself is private to `listener`,
// so it is covered in a later step.)
#[tokio::test]
async fn ws_transport_handshake_exchange() {
    let (mut transport, mut client) = ws_over_tcp().await;
    within(framing::send_message(
        &mut transport,
        &SocketMessage::Handshake,
    ))
    .await
    .unwrap();
    let mut received = Vec::new();
    let expected = frame(&SocketMessage::Handshake);
    while received.len() < expected.len() {
        received.extend_from_slice(&next_binary(&mut client).await);
    }
    assert_eq!(received, expected);

    client
        .send(bin(&frame(&SocketMessage::HandshakeResponse)))
        .await
        .unwrap();
    let got = within(framing::receive_message(&mut transport))
        .await
        .unwrap();
    assert_eq!(json(&got), json(&SocketMessage::HandshakeResponse));
}

#[tokio::test]
async fn ws_transport_client_close_is_socket_error() {
    let (mut transport, mut client) = ws_over_tcp().await;
    client.send(Message::Close(None)).await.unwrap();
    match within(framing::receive_message(&mut transport)).await {
        Err(Error::Socket(text)) => assert!(text.contains("Connection error"), "{text}"),
        other => panic!("expected Error::Socket, got {other:?}"),
    }
}
