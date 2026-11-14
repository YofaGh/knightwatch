#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

//! The manager loop over real TCP clients, driven step by step so the client
//! map can be checked. Queries, commands, `Shutdown` and event broadcast read
//! `get_config()`, which unit tests cannot set (finding 21), so only the
//! config-free paths are here.

use std::time::Duration;

use kw_types::socket::{AuthFailedReason, Frame, SocketAction, SocketMessage};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use super::{ChannelReceivers, ClientManager};
use crate::{
    config::DisplayUser,
    socket::{
        client::ClientId,
        event::ClientManagerEvent,
        framing::{receive_message, send_message},
        transport::Transport,
    },
};

// ───────────────────────── Helpers ─────────────────────────

struct Rig {
    manager: ClientManager,
    receivers: ChannelReceivers,
}

impl Rig {
    fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(16);
        let (_sub_tx, sub_rx) = mpsc::channel(16);
        let mut manager = ClientManager::new(event_rx, event_tx, sub_rx, CancellationToken::new());
        let receivers = manager.take_receivers().unwrap();
        Self { manager, receivers }
    }

    /// Adds a server-side TCP client and returns its id and the peer stream.
    async fn connect(&mut self) -> (ClientId, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let before: Vec<ClientId> = self.manager.clients.keys().copied().collect();
        self.manager
            .handle_event(ClientManagerEvent::AddClient {
                transport: Transport::new_tcp(server),
            })
            .await;
        let id = *self
            .manager
            .clients
            .keys()
            .find(|k| !before.contains(k))
            .expect("a new client");
        (id, peer)
    }

    /// Handles the next request the readers forwarded.
    async fn pump_request(&mut self) {
        let req = within(self.receivers.request.recv()).await.unwrap();
        self.manager.handle_request(req).await;
    }

    /// Handles the next manager event (connect or disconnect).
    async fn pump_event(&mut self) {
        let ev = within(self.receivers.event.recv()).await.unwrap();
        self.manager.handle_event(ev).await;
    }

    fn client(&self, id: ClientId) -> &super::Client {
        self.manager.get_client(id).expect("client is registered")
    }
}

async fn within<F: Future>(fut: F) -> F::Output {
    timeout(Duration::from_secs(5), fut)
        .await
        .expect("timed out")
}

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

fn action(id: u64, action: SocketAction) -> Frame {
    Frame::reply(Some(id), SocketMessage::Action { action })
}

async fn send(peer: &mut TcpStream, frame: &Frame) {
    send_message(peer, frame).await.unwrap();
}

async fn recv(peer: &mut TcpStream) -> serde_json::Value {
    let frame: Frame = within(receive_message(peer)).await.unwrap();
    json(&frame)
}

fn reply(id: u64, message: SocketMessage) -> serde_json::Value {
    json(&Frame::reply(Some(id), message))
}

fn login() -> SocketAction {
    SocketAction::Login {
        username: "alice".into(),
        password: "pw".into(),
    }
}

// ───────────────────────── Clients ─────────────────────────

#[tokio::test]
async fn a_new_client_starts_logged_out_with_events_on_and_ticks_off() {
    let mut rig = Rig::new();
    let (id, _peer) = rig.connect().await;
    let c = rig.client(id);
    assert!(!c.is_authenticated());
    assert!(c.get_display_user().is_none());
    assert!(c.events_enabled());
    assert!(!c.ticks_enabled());
}

#[tokio::test]
async fn each_client_gets_its_own_id() {
    let mut rig = Rig::new();
    let (a, _pa) = rig.connect().await;
    let (b, _pb) = rig.connect().await;
    assert_ne!(a, b);
    assert_eq!(rig.manager.clients.len(), 2);
}

#[tokio::test]
async fn receivers_can_only_be_taken_once() {
    let mut rig = Rig::new();
    assert!(rig.manager.take_receivers().is_err());
}

#[tokio::test]
async fn a_peer_hanging_up_removes_the_client() {
    let mut rig = Rig::new();
    let (id, peer) = rig.connect().await;
    drop(peer);
    rig.pump_event().await;
    assert!(rig.manager.get_client(id).is_none());
}

#[tokio::test]
async fn closing_a_client_from_the_server_ends_the_peer_stream() {
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    rig.manager
        .handle_event(ClientManagerEvent::ClientDisconnected { client_id: id })
        .await;
    assert!(rig.manager.get_client(id).is_none());
    let next: crate::prelude::Result<Frame> = within(receive_message(&mut peer)).await;
    assert!(next.is_err(), "the peer should see the stream end");
}

#[tokio::test]
async fn disconnecting_an_unknown_client_is_harmless() {
    let mut rig = Rig::new();
    let (id, _peer) = rig.connect().await;
    rig.manager
        .handle_event(ClientManagerEvent::ClientDisconnected {
            client_id: ClientId::new_v4(),
        })
        .await;
    assert!(rig.manager.get_client(id).is_some());
}

#[tokio::test]
async fn one_client_leaving_keeps_the_others() {
    let mut rig = Rig::new();
    let (a, pa) = rig.connect().await;
    let (b, mut pb) = rig.connect().await;
    drop(pa);
    rig.pump_event().await;
    assert!(rig.manager.get_client(a).is_none());

    send(&mut pb, &action(1, SocketAction::Logout)).await;
    rig.pump_request().await;
    assert_eq!(recv(&mut pb).await, reply(1, SocketMessage::NotLoggedIn));
    assert!(rig.manager.get_client(b).is_some());
}

// ───────────────────────── Actions ─────────────────────────

#[tokio::test]
async fn the_reply_carries_the_request_id() {
    let mut rig = Rig::new();
    let (_id, mut peer) = rig.connect().await;
    send(&mut peer, &action(7, SocketAction::Logout)).await;
    rig.pump_request().await;
    assert_eq!(recv(&mut peer).await, reply(7, SocketMessage::NotLoggedIn));
}

#[tokio::test]
async fn a_request_without_an_id_gets_a_reply_without_one() {
    let mut rig = Rig::new();
    let (_id, mut peer) = rig.connect().await;
    let frame = Frame::push(SocketMessage::Action {
        action: SocketAction::Logout,
    });
    send(&mut peer, &frame).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        json(&Frame::push(SocketMessage::NotLoggedIn))
    );
}

#[tokio::test]
async fn replies_keep_request_order() {
    let mut rig = Rig::new();
    let (_id, mut peer) = rig.connect().await;
    for id in 1..=3 {
        send(&mut peer, &action(id, SocketAction::Logout)).await;
    }
    for _ in 1..=3 {
        rig.pump_request().await;
    }
    for id in 1..=3 {
        assert_eq!(recv(&mut peer).await, reply(id, SocketMessage::NotLoggedIn));
    }
}

#[tokio::test]
async fn the_reply_goes_only_to_the_client_that_asked() {
    let mut rig = Rig::new();
    let (_a, mut pa) = rig.connect().await;
    let (_b, mut pb) = rig.connect().await;
    send(&mut pa, &action(1, SocketAction::Logout)).await;
    rig.pump_request().await;
    assert_eq!(recv(&mut pa).await, reply(1, SocketMessage::NotLoggedIn));
    let other = timeout(
        Duration::from_millis(200),
        receive_message::<_, Frame>(&mut pb),
    )
    .await;
    assert!(other.is_err(), "the other client should get nothing");
}

#[tokio::test]
async fn login_with_no_users_loaded_is_wrong_credentials() {
    // Nothing in the test process calls `load_users()`.
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    send(&mut peer, &action(1, login())).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        reply(
            1,
            SocketMessage::AuthenticationFailed {
                reason: AuthFailedReason::WrongCredentials,
            }
        )
    );
    assert!(!rig.client(id).is_authenticated());
}

#[tokio::test]
async fn login_when_already_logged_in_is_refused_before_any_user_lookup() {
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    rig.manager
        .get_client_mut(id)
        .unwrap()
        .set_authentication(true);
    send(&mut peer, &action(1, login())).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        reply(1, SocketMessage::AlreadyAuthenticated)
    );
}

#[tokio::test]
async fn logout_clears_the_login_and_the_user() {
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    let c = rig.manager.get_client_mut(id).unwrap();
    c.set_authentication(true);
    c.set_display_user(DisplayUser {
        username: "alice".into(),
        telegram_chat_id: None,
    });

    send(&mut peer, &action(1, SocketAction::Logout)).await;
    rig.pump_request().await;
    assert_eq!(recv(&mut peer).await, reply(1, SocketMessage::LoggedOut));
    assert!(!rig.client(id).is_authenticated());
    assert!(rig.client(id).get_display_user().is_none());

    send(&mut peer, &action(2, SocketAction::Logout)).await;
    rig.pump_request().await;
    assert_eq!(recv(&mut peer).await, reply(2, SocketMessage::NotLoggedIn));
}

#[tokio::test]
async fn event_preferences_set_only_the_given_fields() {
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;

    let both = SocketAction::SetEventPreferences {
        events_enabled: Some(false),
        ticks_enabled: Some(true),
    };
    send(&mut peer, &action(1, both)).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        reply(1, SocketMessage::SetEventPreferences)
    );
    assert!(!rig.client(id).events_enabled());
    assert!(rig.client(id).ticks_enabled());

    let ticks_only = SocketAction::SetEventPreferences {
        events_enabled: None,
        ticks_enabled: Some(false),
    };
    send(&mut peer, &action(2, ticks_only)).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        reply(2, SocketMessage::SetEventPreferences)
    );
    assert!(!rig.client(id).events_enabled(), "untouched");
    assert!(!rig.client(id).ticks_enabled());
}

#[tokio::test]
async fn event_preferences_do_not_need_a_login() {
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    let none = SocketAction::SetEventPreferences {
        events_enabled: None,
        ticks_enabled: None,
    };
    send(&mut peer, &action(1, none)).await;
    rig.pump_request().await;
    assert_eq!(
        recv(&mut peer).await,
        reply(1, SocketMessage::SetEventPreferences)
    );
    assert!(rig.client(id).events_enabled());
    assert!(!rig.client(id).ticks_enabled());
}

#[tokio::test]
async fn a_request_from_a_client_that_already_left_is_dropped() {
    // Finding 19: no reply and no log for an unknown client.
    let mut rig = Rig::new();
    let (id, mut peer) = rig.connect().await;
    send(&mut peer, &action(1, SocketAction::Logout)).await;
    let req = within(rig.receivers.request.recv()).await.unwrap();
    rig.manager
        .handle_event(ClientManagerEvent::ClientDisconnected { client_id: id })
        .await;
    rig.manager.handle_request(req).await;
    assert!(rig.manager.clients.is_empty());
}
