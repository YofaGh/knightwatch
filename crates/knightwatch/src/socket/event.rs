#[derive(Debug)]
pub enum SocketServerEvent {
    AddClient { transport: super::transport::Transport },
    ClientDisconnected { client_id: crate::types::ClientId },
}
