use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::client::ClientId;
use crate::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketMessage {
    Handshake,
    HandshakeResponse,
    Action {
        action: SocketAction,
    },
    Query {
        query: SocketQuery,
    },
    QueryResponse {
        response: SocketQueryResponse,
    },
    Command {
        command: SocketCommand,
    },
    CommandResponse {
        success: bool,
        err: Option<Error>,
        response: SocketCommandResponse,
    },
    Event {
        event: crate::events::EventPayload,
    },
    AuthenticationFailed {
        reason: AuthFailedReason,
    },
    AuthenticationSucceed,
    ShutdownNotEnabled,
    Unauthorized,
    ShuttingDown,
    SetEventPreferences,
}

#[derive(Debug)]
pub struct SocketQueryRequset {
    pub client_id: ClientId,
    pub query: SocketQuery,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQuery {
    /// Common
    Info,
    /// Screen
    Screenshots,
    ScreenPollStatus,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQueryResponse {
    /// Common
    Info { info: kw_types::api::InfoResponse },
    /// Screen
    Screenshots {
        screenshots: Vec<kw_types::screen::Screenshot>,
    },
    ScreenPollStatus {
        status: Option<kw_types::polling::PollStatus>,
    },
}

#[derive(Debug)]
pub struct SocketActionRequset {
    pub client_id: ClientId,
    pub action: SocketAction,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketAction {
    Login { username: String, password: String },
    Logout,
    Shutdown,
    SetEventPreferences {
        events_enabled: Option<bool>,
        ticks_enabled: Option<bool>,
    },
}

#[derive(Debug)]
pub struct SocketCommandRequset {
    pub client_id: ClientId,
    pub command: SocketCommand,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommand {
    /// Screen
    ScreenPollInterval {
        interval: Duration,
    },
    ScreenPollPause,
    ScreenPollResume,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommandResponse {
    /// Screen
    ScreenPollInterval,
    ScreenPollPause,
    ScreenPollResume,
}

pub struct CorrelatedSocketMessage {
    pub message: SocketMessage,
    pub response_tx: tokio::sync::oneshot::Sender<Result<(), crate::errors::Error>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuthFailedReason {
    WrongCredentials,
    InternalServerError,
}
