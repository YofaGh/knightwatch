use serde::{Deserialize, Serialize};

use crate::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketMessage {
    Handshake,
    HandshakeResponse,
    Action { action: SocketAction },
    Query { query: SocketQuery },
    QueryResponse { response: SocketQueryResponse },
    Command { command: SocketCommand },
    Event { event: SocketEvent },
    AuthenticationFailed { reason: AuthFailedReason },
    AuthenticationSucceed,
    ShutdownNotEnabled,
    Unauthorized,
    ShuttingDown,
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
    Screenshots { screenshots: Vec<kw_types::screen::Screenshot> },
    ScreenPollStatus { status: Option<kw_types::polling::PollStatus> }
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
}

#[derive(Debug)]
pub struct SocketCommandRequset {
    pub client_id: ClientId,
    pub command: SocketCommand,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommand {}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketEvent {}

pub struct CorrelatedSocketMessage {
    pub message: SocketMessage,
    pub response_tx: tokio::sync::oneshot::Sender<Result<(), crate::errors::Error>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuthFailedReason {
    WrongCredentials,
    InternalServerError,
}
