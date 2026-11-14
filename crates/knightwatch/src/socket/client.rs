use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use kw_types::socket::{Frame, SocketCommandResponse, SocketMessage};

use super::{message::CorrelatedSocketMessage, transport::Transport};
use crate::prelude::*;

pub type ClientId = uuid::Uuid;
pub type ClientWriter = mpsc::Sender<CorrelatedSocketMessage>;

pub struct ClientConnection {
    pub reader_shutdown_tx: oneshot::Sender<()>,
    pub writer_shutdown_tx: oneshot::Sender<()>,
    pub reader_handle: JoinHandle<ReadHalf<Transport>>,
    pub writer_handle: JoinHandle<WriteHalf<Transport>>,
    pub message_writer_tx: ClientWriter,
}

pub struct Client {
    #[allow(unused)]
    id: ClientId,
    is_authenticated: bool,
    events_enabled: bool,
    ticks_enabled: bool,
    display_user: Option<DisplayUser>,
    connection: Option<ClientConnection>,
}

impl Client {
    pub const fn new(id: ClientId, connection: ClientConnection) -> Self {
        Self {
            id,
            is_authenticated: false,
            events_enabled: true,
            ticks_enabled: false,
            display_user: None,
            connection: Some(connection),
        }
    }

    pub const fn is_authenticated(&self) -> bool {
        self.is_authenticated
    }

    pub const fn set_authentication(&mut self, authentication: bool) {
        self.is_authenticated = authentication;
    }

    pub fn set_display_user(&mut self, display_user: DisplayUser) {
        self.display_user = Some(display_user);
    }

    pub fn clear_display_user(&mut self) {
        self.display_user = None;
    }

    pub fn get_display_user(&self) -> Option<DisplayUser> {
        self.display_user.clone()
    }

    pub const fn take_connection(&mut self) -> Option<ClientConnection> {
        self.connection.take()
    }

    pub const fn events_enabled(&self) -> bool {
        self.events_enabled
    }
    pub const fn set_events_enabled(&mut self, enabled: bool) {
        self.events_enabled = enabled;
    }
    pub const fn ticks_enabled(&self) -> bool {
        self.ticks_enabled
    }
    pub const fn set_ticks_enabled(&mut self, enabled: bool) {
        self.ticks_enabled = enabled;
    }

    pub fn writer(&self) -> Option<ClientWriter> {
        self.connection
            .as_ref()
            .map(|c| c.message_writer_tx.clone())
    }

    pub async fn send_frame(&self, id: Option<u64>, message: SocketMessage) -> Result<()> {
        send_frame(
            self.connection
                .as_ref()
                .map(|c| &c.message_writer_tx)
                .ok_or_else(|| Error::Socket("Client message_writer_tx is None".into()))?,
            Frame::reply(id, message),
        )
        .await
    }

    /// Queues an event without waiting. False when the queue is full or there is no connection.
    pub fn try_send_event(&self, message: SocketMessage) -> bool {
        self.connection.as_ref().is_some_and(|c| {
            c.message_writer_tx
                .try_send(CorrelatedSocketMessage {
                    frame: Frame::push(message),
                    response_tx: None,
                })
                .is_ok()
        })
    }
}

pub fn command_reply(result: Result<()>, response: SocketCommandResponse) -> SocketMessage {
    SocketMessage::CommandResponse {
        success: result.is_ok(),
        err: result.err().map(|err| err.to_string()),
        response,
    }
}

pub async fn send_frame(sender: &ClientWriter, frame: Frame) -> Result<()> {
    let (response_tx, response_rx) = oneshot::channel();
    sender
        .send(CorrelatedSocketMessage {
            frame,
            response_tx: Some(response_tx),
        })
        .await
        .map_err(|_| Error::Socket("Failed to send message to client".to_string()))?;
    response_rx.await.unwrap_or_else(|_| {
        Err(Error::Socket(
            "Response channel closed for client".to_string(),
        ))
    })
}
