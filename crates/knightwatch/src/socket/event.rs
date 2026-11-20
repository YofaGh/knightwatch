#[derive(Debug)]
pub enum ClientManagerEvent {
    AddClient {
        transport: super::transport::Transport,
    },
    ClientDisconnected {
        client_id: super::client::ClientId,
    },
    /// A login checked off the manager loop; `id` is the request id to reply to.
    LoginFinished {
        client_id: super::client::ClientId,
        id: Option<u64>,
        outcome: crate::config::LoginOutcome,
    },
}
