#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use tempfile::TempDir;

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

/// A client whose helper end is returned for the test to script. The `TempDir` holds
/// the socket file.
fn client() -> (SystemdHelperClient, BufReader<UnixStream>, TempDir) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("helper.sock");
    std::fs::write(&socket_path, "").unwrap();
    let child = Command::new("true").spawn().unwrap();
    let client = SystemdHelperClient::from_parts(ours, socket_path, child);
    (client, BufReader::new(theirs), dir)
}

/// Reads one request line and answers it with `reply`.
async fn answer(helper: &mut BufReader<UnixStream>, reply: &str) -> String {
    let mut request = String::new();
    helper.read_line(&mut request).await.unwrap();
    helper.get_mut().write_all(reply.as_bytes()).await.unwrap();
    request
}

async fn control_answered(reply: &str) -> Result<()> {
    let (client, mut helper, _dir) = client();
    let (result, _request) = tokio::join!(
        client.control("nginx.service", ServiceAction::Restart),
        answer(&mut helper, reply),
    );
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// control
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn control_sends_one_json_line_naming_the_unit_and_action() {
    let (client, mut helper, _dir) = client();

    let (result, request) = tokio::join!(
        client.control("nginx.service", ServiceAction::Restart),
        answer(&mut helper, "{\"type\":\"Ok\"}\n"),
    );

    result.unwrap();
    assert!(request.ends_with('\n'));
    let request: HelperRequest = serde_json::from_str(&request).unwrap();
    let HelperRequest::Control { unit_name, action } = request else {
        panic!("wrong request: {request:?}");
    };
    assert_eq!(unit_name, "nginx.service");
    assert_eq!(action, ServiceAction::Restart);
}

#[tokio::test]
async fn a_newline_in_a_unit_name_cannot_start_a_second_request() {
    let (client, mut helper, _dir) = client();

    let (_result, request) = tokio::join!(
        client.control("a\nb", ServiceAction::Stop),
        answer(&mut helper, "{\"type\":\"Ok\"}\n"),
    );

    assert_eq!(request.matches('\n').count(), 1);
}

#[tokio::test]
async fn a_helper_error_comes_back_as_an_error_with_its_message() {
    let err = control_answered("{\"type\":\"Err\",\"message\":\"no such unit\"}\n")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("no such unit"), "{err}");
}

#[tokio::test]
async fn a_pong_is_an_unexpected_answer_to_control() {
    let err = control_answered("{\"type\":\"Pong\"}\n").await.unwrap_err();

    assert!(err.to_string().contains("unexpected response"), "{err}");
}

#[tokio::test]
async fn an_undecodable_answer_is_an_error() {
    let err = control_answered("not json\n").await.unwrap_err();

    assert!(err.to_string().contains("failed to decode"), "{err}");
}

#[tokio::test]
async fn a_closed_helper_is_an_error() {
    let (client, helper, _dir) = client();
    drop(helper);

    assert!(
        client
            .control("nginx.service", ServiceAction::Start)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn requests_are_answered_in_turn_on_one_connection() {
    let (client, mut helper, _dir) = client();
    let ok = "{\"type\":\"Ok\"}\n";
    let script = async {
        answer(&mut helper, ok).await;
        answer(&mut helper, ok).await;
    };

    let (first, second, ()) = tokio::join!(
        client.control("a.service", ServiceAction::Start),
        client.control("b.service", ServiceAction::Stop),
        script,
    );

    first.unwrap();
    second.unwrap();
}

#[tokio::test(start_paused = true)]
#[ignore = "finding 67: no timeout, so a hung helper hangs this and every later command"]
async fn a_hung_helper_is_an_error() {
    let (client, _helper, _dir) = client();

    let result = tokio::time::timeout(
        Duration::from_hours(1),
        client.control("nginx.service", ServiceAction::Start),
    )
    .await;

    assert!(matches!(result, Ok(Err(_))));
}

// ─────────────────────────────────────────────────────────────────────────────
// paths and cleanup
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dropping_the_client_removes_its_socket_file() {
    let (client, _helper, dir) = client();
    let socket_path = dir.path().join("helper.sock");
    assert!(socket_path.exists());

    drop(client);

    assert!(!socket_path.exists());
}

#[test]
fn each_server_process_gets_its_own_socket_path() {
    let path = helper_socket_path(42);

    assert_eq!(path.file_name().unwrap(), "systemd-helper-42.sock");
    assert_ne!(path, helper_socket_path(43));
}

#[test]
fn the_helper_binary_is_named_the_same_wherever_it_is_found() {
    assert_eq!(
        locate_helper_binary().unwrap().file_name().unwrap(),
        "kw-systemd-helper"
    );
}
