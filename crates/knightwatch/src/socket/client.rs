use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{message::CorrelatedSocketMessage, transport::Transport};
use crate::prelude::*;

pub struct Client {
    id: ClientId,
    is_authenticated: bool,
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
}
