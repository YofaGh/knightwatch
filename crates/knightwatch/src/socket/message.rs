use kw_types::socket;

use super::client::ClientId;

#[derive(Debug)]
pub enum RequestKind {
    Action(socket::SocketAction),
    Query(socket::SocketQuery),
    Command(socket::SocketCommand),
}

#[derive(Debug)]
pub struct SocketRequest {
    pub client_id: ClientId,
    pub id: Option<u64>,
    pub kind: RequestKind,
}

pub struct CorrelatedSocketMessage {
    pub frame: socket::Frame,
    /// `None` means fire-and-forget (events).
    pub response_tx: Option<tokio::sync::oneshot::Sender<Result<(), crate::errors::Error>>>,
}
