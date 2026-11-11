#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use axum::Router;
use clap::Parser;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

use super::{handlers::with_security_headers, routers::create_routers};
use crate::config::{AppConfig, CliArgs, PersistentConfig};

// ───────────────────────── Helpers ─────────────────────────

fn config(args: &[&str]) -> AppConfig {
    let mut cli = vec!["knightwatch"];
    cli.extend_from_slice(args);
    AppConfig {
        args: CliArgs::try_parse_from(cli).expect("the test arguments should parse"),
        persistent: PersistentConfig::default(),
    }
}

async fn serve(app: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// Serves `create_routers` for these flags and returns its cancel token too.
async fn serve_flags(args: &[&str]) -> (SocketAddr, CancellationToken) {
    let token = CancellationToken::new();
    let app = create_routers(&config(args), token.clone());
    (serve(app).await, token)
}

struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

/// Sends `content_length` in the header but only `body` on the wire, so a large declared
/// length can be tested without sending the bytes.
async fn send_raw(
    addr: SocketAddr,
    method: &str,
    path: &str,
    extra_headers: &[&str],
    content_length: usize,
    body: &str,
) -> Reply {
    let mut lines = vec![
        format!("{method} {path} HTTP/1.1"),
        format!("Host: {addr}"),
        "Connection: close".to_string(),
        format!("Content-Length: {content_length}"),
    ];
    lines.extend(extra_headers.iter().map(|h| (*h).to_string()));
    let request = format!("{}\r\n\r\n{body}", lines.join("\r\n"));

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut buf))
        .await
        .expect("the server did not answer in time")
        .unwrap();

    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("no status line in the reply");
    Reply {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

async fn send(addr: SocketAddr, method: &str, path: &str) -> Reply {
    send_raw(addr, method, path, &[], 0, "").await
}

async fn status(addr: SocketAddr, method: &str, path: &str) -> u16 {
    send(addr, method, path).await.status
}

// ───────────────────────── Always mounted ─────────────────────────

#[tokio::test]
async fn health_is_open_even_with_auth_on() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    let reply = send(addr, "GET", "/api/health").await;
    assert_eq!(reply.status, 200);
    let json: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(json["status"], "healthy");
    assert_eq!(json["version"], crate::utils::get_version());
}

#[tokio::test]
async fn read_routes_are_open_by_default() {
    let (addr, _) = serve_flags(&[]).await;
    let reply = send(addr, "GET", "/api/supported-signals").await;
    assert_eq!(reply.status, 200);
    assert!(reply.body.starts_with('['), "body: {}", reply.body);
}

#[tokio::test]
async fn read_routes_need_auth_with_the_auth_flag() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    assert_eq!(status(addr, "GET", "/api/supported-signals").await, 401);
    assert_eq!(status(addr, "GET", "/api/root_pids").await, 401);
}

#[tokio::test]
async fn sse_routes_need_auth_with_the_auth_flag() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    for path in [
        "/api/sse",
        "/api/sse/process-tracker",
        "/api/sse/system-resources",
        "/api/sse/docker-tracker",
    ] {
        assert_eq!(status(addr, "GET", path).await, 401, "{path}");
    }
}

#[tokio::test]
async fn unknown_paths_are_404() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    assert_eq!(status(addr, "GET", "/api/nope").await, 404);
    assert_eq!(status(addr, "GET", "/nope").await, 404);
}

#[tokio::test]
async fn read_routes_reject_other_methods() {
    let (addr, _) = serve_flags(&[]).await;
    assert_eq!(status(addr, "POST", "/api/supported-signals").await, 405);
}

// ───────────────────────── Login route ─────────────────────────

#[tokio::test]
async fn login_is_not_mounted_by_default() {
    let (addr, _) = serve_flags(&[]).await;
    assert_eq!(status(addr, "POST", "/api/auth/login").await, 404);
}

#[tokio::test]
async fn login_is_mounted_with_the_auth_flag() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    // GET on a POST-only route is 405, which only happens when the route exists.
    assert_eq!(status(addr, "GET", "/api/auth/login").await, 405);
}

#[tokio::test]
async fn login_with_no_users_is_404() {
    // Pinned: the handler answers 404 (not 401) when no users exist.
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    let body = r#"{"username":"alice","password":"pw"}"#;
    let reply = send_raw(
        addr,
        "POST",
        "/api/auth/login",
        &["Content-Type: application/json"],
        body.len(),
        body,
    )
    .await;
    assert_eq!(reply.status, 404);
}

#[tokio::test]
async fn login_with_a_bad_body_is_a_client_error() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    let body = "{not json";
    let reply = send_raw(
        addr,
        "POST",
        "/api/auth/login",
        &["Content-Type: application/json"],
        body.len(),
        body,
    )
    .await;
    assert_eq!(reply.status, 400);
}

#[tokio::test]
async fn bodies_over_10_mib_are_413() {
    let (addr, _) = serve_flags(&["--enable-auth"]).await;
    let reply = send_raw(
        addr,
        "POST",
        "/api/auth/login",
        &["Content-Type: application/json"],
        10 * 1024 * 1024 + 1,
        "",
    )
    .await;
    assert_eq!(reply.status, 413);
}

// ───────────────────────── Shutdown ─────────────────────────

#[tokio::test]
async fn shutdown_is_not_mounted_by_default() {
    let (addr, token) = serve_flags(&[]).await;
    assert_eq!(status(addr, "POST", "/api/shutdown").await, 404);
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn shutdown_without_auth_cancels_the_token_for_anyone() {
    // Pinned: `--enable-shutdown` alone lets any client stop the server (see finding 27).
    let (addr, token) = serve_flags(&["--enable-shutdown"]).await;
    let reply = send(addr, "POST", "/api/shutdown").await;
    assert_eq!(reply.status, 200);
    assert!(token.is_cancelled());
}

#[tokio::test]
async fn shutdown_with_auth_is_401_and_keeps_running() {
    let (addr, token) = serve_flags(&["--enable-shutdown", "--enable-auth"]).await;
    assert_eq!(status(addr, "POST", "/api/shutdown").await, 401);
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn shutdown_is_post_only() {
    let (addr, token) = serve_flags(&["--enable-shutdown"]).await;
    assert_eq!(status(addr, "GET", "/api/shutdown").await, 405);
    assert!(!token.is_cancelled());
}

// ───────────────────────── Command routes ─────────────────────────

const PROCESS_COMMAND: &str = "/api/process/poll/pause";
const RESOURCES_COMMAND: &str = "/api/resources/poll/pause";
const DOCKER_COMMAND: &str = "/api/docker/poll/pause";

#[tokio::test]
async fn command_routes_are_not_mounted_by_default() {
    let (addr, _) = serve_flags(&[]).await;
    for path in [PROCESS_COMMAND, RESOURCES_COMMAND, DOCKER_COMMAND] {
        assert_eq!(status(addr, "POST", path).await, 404, "{path}");
    }
}

#[tokio::test]
async fn command_routes_always_need_auth_even_without_the_auth_flag() {
    let (addr, _) = serve_flags(&["--allow-process-commands"]).await;
    assert_eq!(status(addr, "POST", PROCESS_COMMAND).await, 401);
    assert_eq!(status(addr, "POST", "/api/process/kill/1").await, 401);
    // The flag also mounts login, so a user could get a token.
    assert_eq!(status(addr, "GET", "/api/auth/login").await, 405);
    // Other trackers' commands stay unmounted.
    assert_eq!(status(addr, "POST", DOCKER_COMMAND).await, 404);
}

#[tokio::test]
async fn command_flags_without_the_tracker_mount_routes_but_no_login() {
    // Finding 31: the routes exist behind the middleware but nobody can log in, so they always
    // answer 401. Fails closed.
    for (flag, path) in [
        ("--allow-system-resources-commands", RESOURCES_COMMAND),
        ("--allow-docker-commands", DOCKER_COMMAND),
    ] {
        let (addr, _) = serve_flags(&[flag]).await;
        assert_eq!(status(addr, "POST", path).await, 401, "{flag}");
        assert_eq!(status(addr, "GET", "/api/auth/login").await, 404, "{flag}");
    }
}

#[tokio::test]
async fn command_flags_with_the_tracker_mount_login() {
    for (args, path) in [
        (
            ["--system-resources", "--allow-system-resources-commands"],
            RESOURCES_COMMAND,
        ),
        (["--docker", "--allow-docker-commands"], DOCKER_COMMAND),
    ] {
        let (addr, _) = serve_flags(&args).await;
        assert_eq!(status(addr, "POST", path).await, 401, "{args:?}");
        assert_eq!(
            status(addr, "GET", "/api/auth/login").await,
            405,
            "{args:?}"
        );
    }
}

#[cfg(feature = "screenshot")]
#[tokio::test]
async fn screen_command_routes_follow_their_flag() {
    let (off, _) = serve_flags(&[]).await;
    assert_eq!(status(off, "POST", "/api/screen/poll/pause").await, 404);
    let (on, _) = serve_flags(&["--allow-screen-commands"]).await;
    assert_eq!(status(on, "POST", "/api/screen/poll/pause").await, 401);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn systemd_command_routes_follow_their_flag() {
    let (off, _) = serve_flags(&[]).await;
    assert_eq!(status(off, "POST", "/api/systemd/poll/pause").await, 404);
    let (on, _) = serve_flags(&["--allow-systemd-commands"]).await;
    assert_eq!(status(on, "POST", "/api/systemd/poll/pause").await, 401);
}

// ───────────────────────── Security headers ─────────────────────────

#[tokio::test]
async fn security_headers_are_on_every_reply() {
    let app = with_security_headers(create_routers(&config(&[]), CancellationToken::new()));
    let addr = serve(app).await;
    for path in ["/api/health", "/api/nope"] {
        let reply = send(addr, "GET", path).await;
        assert_eq!(
            reply.header("x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
        assert_eq!(reply.header("x-frame-options"), Some("DENY"), "{path}");
        assert_eq!(
            reply.header("referrer-policy"),
            Some("no-referrer"),
            "{path}"
        );
    }
}
