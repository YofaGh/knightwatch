use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{
    message::{CorrelatedSocketMessage, SocketMessage},
    transport::Transport,
};
use crate::prelude::*;

pub struct Client {
    id: ClientId,
    is_authenticated: bool,
    display_user: Option<DisplayUser>,
    pub reader_shutdown_tx: Option<oneshot::Sender<()>>,
    pub writer_shutdown_tx: Option<oneshot::Sender<()>>,
    pub reader_handle: Option<JoinHandle<ReadHalf<Transport>>>,
    pub writer_handle: Option<JoinHandle<WriteHalf<Transport>>>,
    pub message_writer_tx: Option<mpsc::Sender<CorrelatedSocketMessage>>,
}

impl Client {
    pub fn new(
        id: ClientId,
        reader_shutdown_tx: oneshot::Sender<()>,
        writer_shutdown_tx: oneshot::Sender<()>,
        reader_handle: JoinHandle<ReadHalf<Transport>>,
        writer_handle: JoinHandle<WriteHalf<Transport>>,
        message_writer_tx: mpsc::Sender<CorrelatedSocketMessage>,
    ) -> Self {
        Self {
            id,
            is_authenticated: false,
            display_user: None,
            reader_shutdown_tx: Some(reader_shutdown_tx),
            writer_shutdown_tx: Some(writer_shutdown_tx),
            reader_handle: Some(reader_handle),
            writer_handle: Some(writer_handle),
            message_writer_tx: Some(message_writer_tx),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.is_authenticated
    }

    pub fn set_authentication(&mut self, authentication: bool) {
        self.is_authenticated = authentication;
    }

    pub fn set_display_user(&mut self, display_user: DisplayUser) {
        self.display_user = Some(display_user)
    }

    pub fn clear_display_user(&mut self) {
        self.display_user = None;
    }

    pub fn get_display_user(&self) -> Option<DisplayUser> {
        self.display_user.clone()
    }

    pub async fn send_message(&self, message: SocketMessage) -> Result<()> {
        send_message_to_client(self
            .message_writer_tx
            .as_ref()
            .ok_or_else(|| Error::Socket("Client message_writer_tx is None".into()))?, message).await
    }
}

pub async fn send_message_to_client(
    sender: &mpsc::Sender<CorrelatedSocketMessage>,
    message: SocketMessage,
) -> Result<()> {
    let (response_tx, response_rx) = oneshot::channel();
    let correlated_message = CorrelatedSocketMessage {
        message,
        response_tx,
    };
    sender
        .send(correlated_message)
        .await
        .map_err(|_| Error::Socket("Failed to send message to client".to_string()))?;
    match response_rx.await {
        Ok(result) => result,
        Err(_) => Err(Error::Socket(
            "Response channel closed for client".to_string(),
        )),
    }
}
