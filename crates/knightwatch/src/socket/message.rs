use kw_types::socket::{SocketAction, SocketCommand, SocketMessage, SocketQuery};

use super::client::ClientId;
use crate::prelude::*;

#[derive(Debug)]
pub struct SocketQueryRequset {
    pub client_id: ClientId,
    pub query: SocketQuery,
}

#[derive(Debug)]
pub struct SocketActionRequset {
    pub client_id: ClientId,
    pub action: SocketAction,
}

#[derive(Debug)]
pub struct SocketCommandRequset {
    pub client_id: ClientId,
    pub command: SocketCommand,
}

pub struct CorrelatedSocketMessage {
    pub message: SocketMessage,
    pub response_tx: tokio::sync::oneshot::Sender<Result<()>>,
}
