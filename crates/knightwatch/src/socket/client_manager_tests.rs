#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use kw_types::socket::{SocketAction, SocketCommand, SocketMessage, SocketQuery};
use serde_json::{Value, json};

use super::{client_manager::access_check, message::RequestKind};

/// The `access_check` reply as JSON: `null` when allowed.
fn check(kind: &RequestKind, enable_auth: bool, authed: bool, has_user: bool) -> Value {
    serde_json::to_value(access_check(kind, enable_auth, authed, has_user)).unwrap()
}

fn unauthorized() -> Value {
    serde_json::to_value(SocketMessage::Unauthorized).unwrap()
}

#[test]
fn info_query_is_allowed_without_login() {
    let kind = RequestKind::Query(SocketQuery::Info);
    assert_eq!(check(&kind, true, false, false), json!(null));
}

#[test]
fn other_query_needs_login_when_auth_enabled() {
    let kind = RequestKind::Query(SocketQuery::Cpu);
    assert_eq!(check(&kind, true, false, false), unauthorized());
    assert_eq!(check(&kind, false, false, false), json!(null));
}

#[test]
fn command_always_needs_login_and_user() {
    let kind = RequestKind::Command(SocketCommand::TrackPid { pid: 1 });
    assert_eq!(check(&kind, false, false, false), unauthorized());
    assert_eq!(check(&kind, false, true, false), unauthorized());
    assert_eq!(check(&kind, false, true, true), json!(null));
}

#[test]
fn actions_are_never_blocked() {
    let kind = RequestKind::Action(SocketAction::Logout);
    assert_eq!(check(&kind, true, false, false), json!(null));
}
