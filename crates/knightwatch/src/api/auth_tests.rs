#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    middleware::from_fn,
    routing::{get, post},
};
use clap::Parser;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use super::{
    end_points::common::logout,
    middleware::auth_middleware,
    session::{Session, get_sessions},
    utils::should_enable_auth,
};
use crate::config::{AppConfig, CliArgs, PersistentConfig, get_users};

const FOREVER: Duration = Duration::MAX;

// ───────────────────────── Helpers ─────────────────────────

fn unique(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4())
}

fn add_session(user: &str, token: &str) {
    get_sessions()
        .write()
        .unwrap()
        .insert(Session::new(user.to_string(), token.to_string()));
}

/// True if the session was still in the store (and removes it).
fn take_session(token: &str) -> bool {
    get_sessions().write().unwrap().remove_by_token(token)
}

fn session_is_valid(token: &str) -> bool {
    get_sessions()
        .write()
        .unwrap()
        .get_valid(token, FOREVER, FOREVER)
        .is_some()
}

async fn serve(app: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

struct Reply {
    status: u16,
    text: String,
}

async fn send(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)]) -> Reply {
    let mut lines = vec![
        format!("{method} {path} HTTP/1.1"),
        format!("Host: {addr}"),
        "Connection: close".to_string(),
        "Content-Length: 0".to_string(),
    ];
    for (name, value) in headers {
        lines.push(format!("{name}: {value}"));
    }
    let request = format!("{}\r\n\r\n", lines.join("\r\n"));

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut buf))
        .await
        .expect("the server did not answer in time")
        .unwrap();

    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("no status line in the reply");
    Reply { status, text }
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// `/protected` sits behind `auth_middleware` and counts how often its handler runs.
/// `/open` is outside the layer.
fn protected_router(hits: &Arc<AtomicUsize>) -> Router {
    let counter = Arc::clone(hits);
    Router::new()
        .route(
            "/protected",
            get(move || {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    "secret"
                }
            }),
        )
        .layer(from_fn(auth_middleware))
        .route("/open", get(|| async { "open" }))
}

fn logout_router() -> Router {
    Router::new().route("/logout", post(logout))
}

fn config(args: &[&str]) -> AppConfig {
    let mut argv = vec!["knightwatch"];
    argv.extend_from_slice(args);
    AppConfig {
        args: CliArgs::try_parse_from(argv).expect("the test arguments should parse"),
        persistent: PersistentConfig::default(),
    }
}

fn auth_on(args: &[&str]) -> bool {
    should_enable_auth(&config(args))
}

// ───────────────────────── auth_middleware (no users loaded) ─────────────────────────

#[test]
fn no_users_are_loaded_in_the_test_process() {
    // The tests below rely on this. If a test ever calls `load_users()`, this fails first.
    assert!(get_users().is_none());
}

#[tokio::test]
async fn routes_outside_the_layer_are_reachable() {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;
    let reply = send(addr, "GET", "/open", &[]).await;
    assert_eq!(reply.status, 200);
    assert!(reply.text.contains("open"));
}

#[tokio::test]
async fn protected_route_without_a_token_is_401_and_the_handler_does_not_run() {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;
    let reply = send(addr, "GET", "/protected", &[]).await;
    assert_eq!(reply.status, 401);
    assert!(!reply.text.contains("secret"));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_live_session_token_is_still_401_when_no_users_are_loaded() {
    // Fails closed: without loaded users the middleware refuses everything, even a token that
    // exists in the session store.
    let token = unique("mw-live");
    add_session("alice", &token);
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;

    let reply = send(addr, "GET", "/protected", &[("Authorization", &bearer(&token))]).await;

    assert_eq!(reply.status, 401);
    assert!(!reply.text.contains("secret"));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    // The refused request did not consume or evict the session.
    assert!(take_session(&token));
}

#[tokio::test]
async fn an_unknown_token_is_401() {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;
    let reply = send(
        addr,
        "GET",
        "/protected",
        &[("Authorization", &bearer(&unique("mw-unknown")))],
    )
    .await;
    assert_eq!(reply.status, 401);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_authorization_headers_are_401() {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;
    for value in ["Basic dXNlcjpwYXNz", "Bearer", "just-a-token", "Token abc"] {
        let reply = send(addr, "GET", "/protected", &[("Authorization", value)]).await;
        assert_eq!(reply.status, 401, "Authorization: {value}");
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_refusal_does_not_leak_a_reason_in_the_body() {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve(protected_router(&hits)).await;
    let reply = send(addr, "GET", "/protected", &[]).await;
    let body = reply.text.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body.trim().is_empty(), "unexpected body: {body:?}");
}

// ───────────────────────── logout ─────────────────────────

#[tokio::test]
async fn logout_removes_the_session_and_only_that_one() {
    let mine = unique("logout-mine");
    let other = unique("logout-other");
    add_session("alice", &mine);
    add_session("bob", &other);
    let addr = serve(logout_router()).await;

    let reply = send(addr, "POST", "/logout", &[("Authorization", &bearer(&mine))]).await;

    assert_eq!(reply.status, 200);
    assert!(!take_session(&mine), "the session should already be gone");
    assert!(session_is_valid(&other));
    assert!(take_session(&other));
}

#[tokio::test]
async fn logout_with_an_unknown_token_is_still_200() {
    let addr = serve(logout_router()).await;
    let reply = send(
        addr,
        "POST",
        "/logout",
        &[("Authorization", &bearer(&unique("logout-unknown")))],
    )
    .await;
    assert_eq!(reply.status, 200);
}

#[tokio::test]
async fn logging_out_twice_is_200_both_times() {
    let token = unique("logout-twice");
    add_session("alice", &token);
    let addr = serve(logout_router()).await;
    let header = bearer(&token);

    let first = send(addr, "POST", "/logout", &[("Authorization", &header)]).await;
    let second = send(addr, "POST", "/logout", &[("Authorization", &header)]).await;

    assert_eq!(first.status, 200);
    assert_eq!(second.status, 200);
    assert!(!take_session(&token));
}

#[tokio::test]
async fn logout_without_an_authorization_header_is_400() {
    // Pinned: the typed-header extractor rejects with 400, not 401.
    let addr = serve(logout_router()).await;
    let reply = send(addr, "POST", "/logout", &[]).await;
    assert_eq!(reply.status, 400);
}

#[tokio::test]
async fn logout_with_a_non_bearer_scheme_is_400_and_keeps_sessions() {
    let token = unique("logout-basic");
    add_session("alice", &token);
    let addr = serve(logout_router()).await;

    let reply = send(
        addr,
        "POST",
        "/logout",
        &[("Authorization", "Basic dXNlcjpwYXNz")],
    )
    .await;

    assert_eq!(reply.status, 400);
    assert!(session_is_valid(&token));
    assert!(take_session(&token));
}

#[tokio::test]
async fn logout_only_accepts_post() {
    let token = unique("logout-get");
    add_session("alice", &token);
    let addr = serve(logout_router()).await;

    let reply = send(addr, "GET", "/logout", &[("Authorization", &bearer(&token))]).await;

    assert_eq!(reply.status, 405);
    assert!(session_is_valid(&token));
    assert!(take_session(&token));
}

// ───────────────────────── should_enable_auth ─────────────────────────

#[test]
fn defaults_do_not_enable_auth() {
    assert!(!auth_on(&[]));
}

#[test]
fn the_auth_flag_enables_auth() {
    assert!(auth_on(&["--enable-auth"]));
}

#[test]
fn process_commands_enable_auth_by_themselves() {
    assert!(auth_on(&["--allow-process-commands"]));
}

#[test]
fn tracker_flags_alone_do_not_enable_auth() {
    assert!(!auth_on(&["--system-resources"]));
    assert!(!auth_on(&["--docker"]));
    assert!(!auth_on(&["--top-processes"]));
}

#[test]
fn system_resources_commands_need_the_tracker_to_enable_auth() {
    // Pinned quirk: `create_routers` still mounts the command routes (behind the middleware)
    // with only `--allow-system-resources-commands`, but `/api/auth` is not mounted, so nobody
    // can log in and the routes always answer 401. Safe, but confusing. Should this flip?
    assert!(!auth_on(&["--allow-system-resources-commands"]));
    assert!(auth_on(&[
        "--system-resources",
        "--allow-system-resources-commands"
    ]));
}

#[test]
fn docker_commands_need_the_tracker_to_enable_auth() {
    // Same quirk as above for docker.
    assert!(!auth_on(&["--allow-docker-commands"]));
    assert!(auth_on(&["--docker", "--allow-docker-commands"]));
}

#[test]
fn any_single_condition_is_enough() {
    assert!(auth_on(&[
        "--system-resources",
        "--allow-system-resources-commands",
        "--docker"
    ]));
    assert!(auth_on(&["--enable-auth", "--docker"]));
}

#[test]
fn unrelated_flags_do_not_enable_auth() {
    assert!(!auth_on(&[
        "--enable-shutdown",
        "--no-dashboard",
        "--telegram",
        "--tcp-socket",
        "--ws-socket",
        "--port",
        "9000"
    ]));
}

#[cfg(feature = "screenshot")]
#[test]
fn screen_commands_enable_auth_unless_blind() {
    assert!(auth_on(&["--allow-screen-commands"]));
    // Pinned quirk: blind plus screen commands leaves the screen command routes mounted with
    // no login route (see the system resources test above).
    assert!(!auth_on(&["--blind", "--allow-screen-commands"]));
    assert!(auth_on(&["--blind", "--enable-auth"]));
    assert!(auth_on(&[
        "--blind",
        "--allow-screen-commands",
        "--allow-process-commands"
    ]));
}

#[cfg(feature = "screenshot")]
#[test]
fn blind_alone_does_not_enable_auth() {
    assert!(!auth_on(&["--blind"]));
}

#[cfg(target_os = "linux")]
#[test]
fn systemd_commands_need_systemd_to_enable_auth() {
    assert!(!auth_on(&["--allow-systemd-commands"]));
    assert!(!auth_on(&["--systemd"]));
    assert!(auth_on(&["--systemd", "--allow-systemd-commands"]));
}