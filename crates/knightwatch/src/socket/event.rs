#[derive(Debug)]
pub enum ClientManagerEvent {
    AddClient { transport: super::transport::Transport },
    ClientDisconnected { client_id: crate::types::ClientId },
}
