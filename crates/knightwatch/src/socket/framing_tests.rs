#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::time::Duration;

use kw_types::socket::{
    AuthFailedReason, SocketAction, SocketCommand, SocketCommandResponse, SocketMessage,
    SocketQuery, SocketQueryResponse,
};
use tokio::io::AsyncWriteExt;

use super::framing::{receive_message, send_message};
use crate::errors::Error;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn json(msg: &SocketMessage) -> String {
    serde_json::to_string(msg).unwrap()
}

/// Build a raw frame from a body: 4-byte big-endian length + body.
fn frame(body: &[u8]) -> Vec<u8> {
    let len = u32::try_from(body.len()).unwrap();
    let mut out = len.to_be_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

async fn encode(msg: &SocketMessage) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    send_message(&mut buf, msg).await.unwrap();
    buf
}

async fn receive_from(bytes: &[u8]) -> Result<SocketMessage, Error> {
    let mut reader = bytes;
    receive_message(&mut reader).await
}

fn is_socket_err<T>(res: &Result<T, Error>) -> bool {
    matches!(res, Err(Error::Socket(_)))
}

fn is_other_err<T>(res: &Result<T, Error>) -> bool {
    matches!(res, Err(Error::Other(_)))
}

/// One value per kind of message, including every unit variant, nested
/// payloads, `Option`s, `Duration`, negative numbers and non-ASCII text.
fn sample_messages() -> Vec<SocketMessage> {
    vec![
        SocketMessage::Handshake,
        SocketMessage::HandshakeResponse,
        SocketMessage::Action {
            action: SocketAction::Login {
                username: "admin".into(),
                password: "p@ss \"quoted\" \\ slash".into(),
            },
        },
        SocketMessage::Action {
            action: SocketAction::Logout,
        },
        SocketMessage::Action {
            action: SocketAction::Shutdown,
        },
        SocketMessage::Action {
            action: SocketAction::SetEventPreferences {
                events_enabled: Some(true),
                ticks_enabled: None,
            },
        },
        SocketMessage::Query {
            query: SocketQuery::Info,
        },
        SocketMessage::Query {
            query: SocketQuery::Root { root_pid: 42 },
        },
        SocketMessage::Query {
            query: SocketQuery::Unit {
                unit_name: "ssh.service".into(),
            },
        },
        SocketMessage::Query {
            query: SocketQuery::Container {
                id_or_name: "caf\u{e9}-\u{65e5}\u{672c}-\u{1f980}".into(),
            },
        },
        SocketMessage::QueryResponse {
            response: SocketQueryResponse::RootPids {
                pids: vec![1, 2, u32::MAX],
            },
        },
        SocketMessage::QueryResponse {
            response: SocketQueryResponse::IsProcessDone { done: None },
        },
        SocketMessage::QueryResponse {
            response: SocketQueryResponse::ScreenCapturePollStatus { status: None },
        },
        SocketMessage::Command {
            command: SocketCommand::TrackPid { pid: 7 },
        },
        SocketMessage::Command {
            command: SocketCommand::ScreenCapturePollInterval {
                interval: Duration::from_millis(2500),
            },
        },
        SocketMessage::Command {
            command: SocketCommand::StopContainer {
                id_or_name: "web".into(),
                timeout_secs: Some(-1),
            },
        },
        SocketMessage::CommandResponse {
            success: true,
            err: None,
            response: SocketCommandResponse::TrackPid,
        },
        SocketMessage::CommandResponse {
            success: false,
            err: Some("boom".into()),
            response: SocketCommandResponse::KillTree {
                killed_pids: vec![10, 11],
            },
        },
        SocketMessage::AuthenticationFailed {
            reason: AuthFailedReason::WrongCredentials,
        },
        SocketMessage::AuthenticationFailed {
            reason: AuthFailedReason::InternalServerError,
        },
        SocketMessage::AuthenticationSucceed,
        SocketMessage::ShutdownNotEnabled,
        SocketMessage::Unauthorized,
        SocketMessage::AlreadyAuthenticated,
        SocketMessage::ShuttingDown,
        SocketMessage::SetEventPreferences,
    ]
}

// ---------------------------------------------------------------------------
// send_message: exact wire format
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_handshake_writes_prefix_then_json() {
    let bytes = encode(&SocketMessage::Handshake).await;
    // body is the JSON string "Handshake" with quotes: 11 bytes
    let mut expected = vec![0, 0, 0, 11];
    expected.extend_from_slice(b"\"Handshake\"");
    assert_eq!(bytes, expected);
}

#[tokio::test]
async fn send_prefix_is_big_endian_body_length_for_every_sample() {
    for msg in sample_messages() {
        let bytes = encode(&msg).await;
        let (prefix, body) = bytes.split_at(4);
        let declared = u32::from_be_bytes(prefix.try_into().unwrap());
        assert_eq!(
            usize::try_from(declared).unwrap(),
            body.len(),
            "prefix mismatch for {msg:?}"
        );
        assert_eq!(body, json(&msg).as_bytes(), "body mismatch for {msg:?}");
    }
}

#[tokio::test]
async fn send_prefix_is_big_endian_for_bodies_over_255_bytes() {
    // a body longer than 255 bytes needs the second prefix byte
    let msg = SocketMessage::Query {
        query: SocketQuery::Container {
            id_or_name: "x".repeat(300),
        },
    };
    let bytes = encode(&msg).await;
    let body_len = bytes.len() - 4;
    assert!(body_len > 255);
    let expected_prefix = u32::try_from(body_len).unwrap().to_be_bytes();
    assert_eq!(&bytes[..4], &expected_prefix);
    assert_eq!(bytes[0], 0);
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], u8::try_from(body_len >> 8).unwrap());
}

#[tokio::test]
async fn send_login_exact_json() {
    let msg = SocketMessage::Action {
        action: SocketAction::Login {
            username: "u".into(),
            password: "p".into(),
        },
    };
    let bytes = encode(&msg).await;
    assert_eq!(
        std::str::from_utf8(&bytes[4..]).unwrap(),
        r#"{"Action":{"action":{"Login":{"username":"u","password":"p"}}}}"#
    );
}

#[tokio::test]
async fn send_poll_interval_uses_secs_and_nanos() {
    let msg = SocketMessage::Command {
        command: SocketCommand::ScreenCapturePollInterval {
            interval: Duration::from_millis(2500),
        },
    };
    let bytes = encode(&msg).await;
    assert_eq!(
        std::str::from_utf8(&bytes[4..]).unwrap(),
        r#"{"Command":{"command":{"ScreenCapturePollInterval":{"interval":{"secs":2,"nanos":500000000}}}}}"#
    );
}

#[tokio::test]
async fn send_command_response_exact_json() {
    let msg = SocketMessage::CommandResponse {
        success: false,
        err: Some("boom".into()),
        response: SocketCommandResponse::KillProcess { os_result: false },
    };
    let bytes = encode(&msg).await;
    assert_eq!(
        std::str::from_utf8(&bytes[4..]).unwrap(),
        r#"{"CommandResponse":{"success":false,"err":"boom","response":{"KillProcess":{"os_result":false}}}}"#
    );
}

#[tokio::test]
async fn send_appends_frames_without_separator() {
    let mut buf: Vec<u8> = Vec::new();
    send_message(&mut buf, &SocketMessage::Handshake)
        .await
        .unwrap();
    send_message(&mut buf, &SocketMessage::ShuttingDown)
        .await
        .unwrap();
    let first = frame(b"\"Handshake\"");
    let second = frame(b"\"ShuttingDown\"");
    let mut expected = first;
    expected.extend_from_slice(&second);
    assert_eq!(buf, expected);
}

#[tokio::test]
async fn send_to_closed_peer_is_socket_error() {
    let (mut a, b) = tokio::io::duplex(64);
    drop(b);
    let res = send_message(&mut a, &SocketMessage::Handshake).await;
    assert!(is_socket_err(&res), "got {res:?}");
}

// ---------------------------------------------------------------------------
// round trips
// ---------------------------------------------------------------------------

#[tokio::test]
async fn round_trip_every_sample_message() {
    for msg in sample_messages() {
        let bytes = encode(&msg).await;
        let back = receive_from(&bytes).await.unwrap();
        assert_eq!(json(&back), json(&msg), "round trip changed {msg:?}");
    }
}

#[tokio::test]
async fn round_trip_through_duplex_stream() {
    let (mut a, mut b) = tokio::io::duplex(4096);
    for msg in sample_messages() {
        send_message(&mut a, &msg).await.unwrap();
        let back = receive_message(&mut b).await.unwrap();
        assert_eq!(json(&back), json(&msg));
    }
}

#[tokio::test]
async fn round_trip_large_message_over_small_buffer() {
    // body much larger than the pipe buffer, so the reader sees many chunks
    let msg = SocketMessage::QueryResponse {
        response: SocketQueryResponse::RootPids {
            pids: (0..20_000).collect(),
        },
    };
    let expected = json(&msg);
    let (mut a, mut b) = tokio::io::duplex(64);
    let writer = tokio::spawn(async move {
        send_message(&mut a, &msg).await.unwrap();
    });
    let back = receive_message(&mut b).await.unwrap();
    writer.await.unwrap();
    assert_eq!(json(&back), expected);
}

#[tokio::test]
async fn round_trip_unicode_and_escapes() {
    let msg = SocketMessage::Query {
        query: SocketQuery::Unit {
            unit_name: "line\nbreak\t\"quote\" \u{0}nul \u{1f980} \u{e9}".into(),
        },
    };
    let bytes = encode(&msg).await;
    let back = receive_from(&bytes).await.unwrap();
    assert_eq!(json(&back), json(&msg));
}

// ---------------------------------------------------------------------------
// receive_message: stream behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn receive_reads_back_to_back_frames_in_order() {
    let msgs = sample_messages();
    let mut stream: Vec<u8> = Vec::new();
    for msg in &msgs {
        stream.extend_from_slice(&encode(msg).await);
    }
    let mut reader: &[u8] = &stream;
    for msg in &msgs {
        let back = receive_message(&mut reader).await.unwrap();
        assert_eq!(json(&back), json(msg));
    }
    // stream is exhausted: next read is a connection error, not a hang or panic
    assert!(reader.is_empty());
    let res = receive_message(&mut reader).await;
    assert!(is_socket_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_leaves_following_bytes_untouched() {
    let mut stream = frame(b"\"Handshake\"");
    stream.extend_from_slice(b"tail");
    let mut reader: &[u8] = &stream;
    receive_message(&mut reader).await.unwrap();
    assert_eq!(reader, b"tail");
}

#[tokio::test]
async fn receive_works_when_bytes_arrive_one_at_a_time() {
    // duplex(1) lets at most one byte through per write, so both the prefix
    // and the body are split across many reads
    let msgs = sample_messages();
    let mut stream: Vec<u8> = Vec::new();
    for msg in &msgs {
        stream.extend_from_slice(&encode(msg).await);
    }
    let (mut a, mut b) = tokio::io::duplex(1);
    let writer = tokio::spawn(async move {
        a.write_all(&stream).await.unwrap();
        // `a` is dropped here, closing the stream
    });
    for msg in &msgs {
        let back = receive_message(&mut b).await.unwrap();
        assert_eq!(json(&back), json(msg));
    }
    writer.await.unwrap();
    let res = receive_message(&mut b).await;
    assert!(is_socket_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_works_when_prefix_and_body_are_written_separately() {
    let bytes = encode(&SocketMessage::AuthenticationSucceed).await;
    let (mut a, mut b) = tokio::io::duplex(1024);
    let (prefix, body) = bytes.split_at(4);
    let (prefix, body) = (prefix.to_vec(), body.to_vec());
    let writer = tokio::spawn(async move {
        a.write_all(&prefix).await.unwrap();
        tokio::task::yield_now().await;
        a.write_all(&body).await.unwrap();
    });
    let back = receive_message(&mut b).await.unwrap();
    writer.await.unwrap();
    assert_eq!(json(&back), json(&SocketMessage::AuthenticationSucceed));
}

// ---------------------------------------------------------------------------
// receive_message: connection errors (Error::Socket)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn receive_from_empty_stream_is_socket_error() {
    let res = receive_from(&[]).await;
    assert!(is_socket_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_truncated_prefix_is_socket_error() {
    for len in 1..4 {
        let res = receive_from(&vec![0u8; len]).await;
        assert!(is_socket_err(&res), "prefix of {len} bytes gave {res:?}");
    }
}

#[tokio::test]
async fn receive_prefix_only_is_socket_error() {
    let res = receive_from(&11u32.to_be_bytes()).await;
    assert!(is_socket_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_truncated_body_is_socket_error() {
    let full = frame(b"\"Handshake\"");
    for cut in 4..full.len() {
        let res = receive_from(&full[..cut]).await;
        assert!(is_socket_err(&res), "cut at {cut} gave {res:?}");
    }
}

#[tokio::test]
async fn receive_peer_closing_mid_body_is_socket_error() {
    let (mut a, mut b) = tokio::io::duplex(64);
    let writer = tokio::spawn(async move {
        a.write_all(&100u32.to_be_bytes()).await.unwrap();
        a.write_all(&[b'x'; 10]).await.unwrap();
        // dropped: peer sees EOF after 10 of 100 bytes
    });
    let res = receive_message(&mut b).await;
    writer.await.unwrap();
    assert!(is_socket_err(&res), "got {res:?}");
}

#[tokio::test]
async fn socket_error_text_mentions_connection() {
    let res = receive_from(&[]).await;
    let text = res.unwrap_err().to_string();
    assert!(text.starts_with("Connection error"), "got {text:?}");
}

// ---------------------------------------------------------------------------
// receive_message: bad bodies (Error::Other)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn receive_zero_length_frame_is_other_error() {
    let res = receive_from(&frame(b"")).await;
    assert!(is_other_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_malformed_json_is_other_error() {
    let bodies: [&[u8]; 5] = [
        b"not json",
        b"{",
        b"\"Handshake",
        b"\xff\xfe\xfd",
        b"{\"Action\":{\"action\":",
    ];
    for body in bodies {
        let res = receive_from(&frame(body)).await;
        assert!(is_other_err(&res), "body {body:?} gave {res:?}");
    }
}

#[tokio::test]
async fn receive_valid_json_of_wrong_shape_is_other_error() {
    let bodies: [&[u8]; 8] = [
        b"null",
        b"42",
        b"[]",
        b"{}",
        b"\"Handshakes\"",
        b"{\"Nope\":1}",
        b"{\"Action\":{\"action\":\"NotAnAction\"}}",
        // a variant that carries data, sent without it
        b"\"Action\"",
    ];
    for body in bodies {
        let res = receive_from(&frame(body)).await;
        assert!(is_other_err(&res), "body {body:?} gave {res:?}");
    }
}

#[tokio::test]
async fn receive_rejects_trailing_garbage_inside_frame() {
    let res = receive_from(&frame(b"\"Handshake\" x")).await;
    assert!(is_other_err(&res), "got {res:?}");
}

#[tokio::test]
async fn receive_accepts_whitespace_around_json() {
    let back = receive_from(&frame(b"  \n\"Handshake\"\t ")).await.unwrap();
    assert_eq!(json(&back), json(&SocketMessage::Handshake));
}

#[tokio::test]
async fn bad_body_does_not_poison_the_next_frame() {
    // the bad frame is fully consumed, so the stream stays in sync
    let mut stream = frame(b"garbage");
    stream.extend_from_slice(&frame(b"\"Unauthorized\""));
    let mut reader: &[u8] = &stream;
    let first = receive_message(&mut reader).await;
    assert!(is_other_err(&first), "got {first:?}");
    let second = receive_message(&mut reader).await.unwrap();
    assert_eq!(json(&second), json(&SocketMessage::Unauthorized));
}

// ---------------------------------------------------------------------------
// known issues (ignored; they describe the wanted behaviour)
// ---------------------------------------------------------------------------

/// `receive_message` allocates whatever the 4-byte prefix declares (up to
/// 4 GiB) before reading. A client can make the server reserve huge buffers.
/// Wanted: reject an oversized declared length with an error and without
/// consuming the body. This assumes the chosen limit is below 256 MiB.
#[tokio::test]
#[ignore = "no maximum frame size: receive_message trusts the length prefix"]
async fn receive_rejects_oversized_declared_length() {
    let declared: u32 = 0x1000_0000; // 256 MiB
    let mut stream = declared.to_be_bytes().to_vec();
    let extra = 1 << 20;
    stream.extend(vec![0u8; extra]);
    let mut reader: &[u8] = &stream;
    let res = receive_message(&mut reader).await;
    assert!(res.is_err());
    assert_eq!(
        reader.len(),
        extra,
        "the body of an oversized frame should not be read"
    );
}

/// On a bad body, the error text includes the raw frame bytes
/// (`{message_buf:?}`), printed as decimal numbers. A malformed `Login`
/// frame therefore puts the password into the error, and likely the logs.
/// Wanted: the error text must not contain frame contents.
#[tokio::test]
#[ignore = "deserialize error prints the raw frame bytes, which can include a password"]
async fn bad_frame_error_does_not_leak_body_bytes() {
    // truncated Login frame with the password "hunter2"
    let body = br#"{"Action":{"action":{"Login":{"username":"u","password":"hunter2""#;
    let res = receive_from(&frame(body)).await;
    let text = res.unwrap_err().to_string();
    // "hunter2" as decimal bytes, the way Vec<u8> is printed with {:?}
    assert!(
        !text.contains("104, 117, 110, 116, 101, 114, 50"),
        "leaked: {text}"
    );
    assert!(!text.contains("hunter2"), "leaked: {text}");
}
