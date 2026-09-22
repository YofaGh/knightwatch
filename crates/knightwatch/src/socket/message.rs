use kw_types::socket;

use super::client::ClientId;

#[derive(Debug)]
pub struct SocketQueryRequset {
    pub client_id: ClientId,
    pub query: socket::SocketQuery,
}

#[derive(Debug)]
pub struct SocketActionRequset {
    pub client_id: ClientId,
    pub action: socket::SocketAction,
}

#[derive(Debug)]
pub struct SocketCommandRequset {
    pub client_id: ClientId,
    pub command: socket::SocketCommand,
}

pub struct CorrelatedSocketMessage {
    pub message: socket::SocketMessage,
    pub response_tx: tokio::sync::oneshot::Sender<Result<(), crate::errors::Error>>,
}
