use std::{collections::HashMap, sync::OnceLock};
use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, timeout},
};

use super::{
    client::Client,
    event::SocketServerEvent,
    message::{CorrelatedSocketMessage, SocketCommandRequset, SocketMessage, SocketQueryRequset},
    transport::Transport,
};
use crate::prelude::*;

struct Server {
    clients: HashMap<ClientId, Client>,
    query_rx: Option<mpsc::Receiver<SocketQueryRequset>>,
    query_tx: mpsc::Sender<SocketQueryRequset>,
    command_rx: Option<mpsc::Receiver<SocketCommandRequset>>,
    command_tx: mpsc::Sender<SocketCommandRequset>,
    event_rx: Option<mpsc::Receiver<SocketServerEvent>>,
    event_tx: mpsc::Sender<SocketServerEvent>,
}

impl Server {
    pub fn new(
        event_rx: mpsc::Receiver<SocketServerEvent>,
        event_tx: mpsc::Sender<SocketServerEvent>,
    ) -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(1024);
        Self {
            clients: HashMap::new(),
            query_rx: Some(query_rx),
            query_tx,
            command_rx: Some(command_rx),
            command_tx,
            event_rx: Some(event_rx),
            event_tx,
        }
    }

    pub fn get_client(&self, client_id: ClientId) -> Option<&Client> {
        self.clients.get(&client_id)
    }

    pub fn get_client_mut(&mut self, client_id: ClientId) -> Option<&mut Client> {
        self.clients.get_mut(&client_id)
    }

    pub fn add_client(&mut self, transport: Transport) {
        let client_id = ClientId::new_v4();
        let (
            reader_shutdown_tx,
            writer_shutdown_tx,
            reader_handle,
            writer_handle,
            message_writer_tx,
        ) = self.setup_client_connection(transport, client_id);
        let client = Client::new(
            client_id,
            reader_shutdown_tx,
            writer_shutdown_tx,
            reader_handle,
            writer_handle,
            message_writer_tx,
        );
        self.clients.insert(client_id, client);
    }

    pub async fn start(mut self) -> Result<()> {
        let (mut command_rx, mut query_rx, mut event_rx) = self.take_receivers()?;
        tokio::select! {
            Some(query_req) = query_rx.recv() => {
                if let Err(err) = self.handle_query(query_req).await {
                    error!("Failed to handle query err: {err}");
                }
            }
            Some(command_req) = command_rx.recv() => {
                if let Err(err) = self.handle_command(command_req).await {
                    error!("Failed to handle command err: {err}");
                }
            }
            Some(event_req) = event_rx.recv() => {
                self.handle_event(event_req).await;
            }
        }
        Ok(())
    }

    pub async fn handle_query(&self, query_req: SocketQueryRequset) -> Result<()> {
        Ok(())
    }
    
    pub async fn handle_command(&self, command_req: SocketCommandRequset) -> Result<()> {
        Ok(())
    }

    pub async fn handle_event(&mut self, event_req: SocketServerEvent) {
        match event_req {
            SocketServerEvent::AddClient { transport } => {
                self.add_client(transport);
            }
            SocketServerEvent::ClientDisconnected { client_id } => {
                self.close_client_connection(client_id).await;
            }
        }
    }

    pub fn take_receivers(
        &mut self,
    ) -> Result<(
        mpsc::Receiver<SocketCommandRequset>,
        mpsc::Receiver<SocketQueryRequset>,
        mpsc::Receiver<SocketServerEvent>,
    )> {
        let command_rx = self
            .command_rx
            .take()
            .ok_or_else(|| Error::Socket("Command receiver already taken".into()))?;
        let query_rx = self
            .query_rx
            .take()
            .ok_or_else(|| Error::Socket("Query receiver already taken".into()))?;
        let event_rx = self
            .event_rx
            .take()
            .ok_or_else(|| Error::Socket("Event receiver already taken".into()))?;
        Ok((command_rx, query_rx, event_rx))
    }

    pub fn setup_client_connection(
        &mut self,
        transport: Transport,
        client_id: ClientId,
    ) -> (
        oneshot::Sender<()>,
        oneshot::Sender<()>,
        JoinHandle<ReadHalf<Transport>>,
        JoinHandle<WriteHalf<Transport>>,
        mpsc::Sender<CorrelatedSocketMessage>,
    ) {
        let (reader, writer) = tokio::io::split(transport);
        let (reader_shutdown_tx, reader_shutdown_rx) = oneshot::channel();
        let (writer_shutdown_tx, writer_shutdown_rx) = oneshot::channel();
        let (message_writer_tx, message_writer_rx) = mpsc::channel(1024);
        let reader_handle = self.setup_client_receiver(client_id, reader, reader_shutdown_rx);
        let writer_handle =
            self.setup_client_sender(client_id, writer, message_writer_rx, writer_shutdown_rx);
        (
            reader_shutdown_tx,
            writer_shutdown_tx,
            reader_handle,
            writer_handle,
            message_writer_tx,
        )
    }

    pub fn setup_client_receiver(
        &self,
        client_id: ClientId,
        mut reader: ReadHalf<Transport>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> JoinHandle<ReadHalf<Transport>> {
        let query_tx = self.query_tx.clone();
        let command_tx = self.command_tx.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        warn!("Receiver shutting down for client {client_id}");
                        break;
                    }
                    message_result = super::framing::receive_message(&mut reader) => {
                        match message_result {
                            Ok(message) => {
                                match message {
                                    SocketMessage::Query { query } => {
                                        let _ = query_tx.send(SocketQueryRequset {
                                            client_id,
                                            query
                                        }).await;
                                    }
                                    SocketMessage::Command { command } => {
                                        let _ = command_tx.send(SocketCommandRequset {
                                            client_id,
                                            command
                                        }).await;
                                    }
                                    _ => {}
                                }
                            }
                            Err(err) => {
                                warn!("Client {client_id} disconnected err: {err}");
                                Self::notify_client_disconnection(&event_tx, client_id);
                                break;
                            }
                        }
                    }
                }
            }
            reader
        })
    }

    pub fn setup_client_sender(
        &self,
        client_id: ClientId,
        mut writer: WriteHalf<Transport>,
        mut receiver: mpsc::Receiver<CorrelatedSocketMessage>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> JoinHandle<WriteHalf<Transport>> {
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        warn!("Sender shutting down for client {client_id}");
                        break;
                    }
                    correlated_msg = receiver.recv() => {
                        match correlated_msg {
                            Some(CorrelatedSocketMessage { message, response_tx }) => {
                                let result = super::framing::send_message(&mut writer, &message).await;
                                let mut should_break = false;
                                if result.is_err() {
                                    should_break = true;
                                    warn!("Failed to send message to client {client_id}: {result:?}");
                                    Self::notify_client_disconnection(&event_tx, client_id);
                                }
                                let _ = response_tx.send(result);
                                if should_break {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
            writer
        })
    }

    pub fn notify_client_disconnection(
        event_tx: &mpsc::Sender<SocketServerEvent>,
        client_id: ClientId,
    ) {
        let _ = event_tx.send(SocketServerEvent::ClientDisconnected { client_id });
    }

    async fn close_client_connection(&mut self, client_id: ClientId) {
        let Some(mut client) = self.clients.remove(&client_id) else {
            return;
        };
        if let Some(tx) = client.reader_shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = client.writer_shutdown_tx.take() {
            let _ = tx.send(());
        }
        let (Some(reader_handle), Some(writer_handle)) =
            (client.reader_handle.take(), client.writer_handle.take())
        else {
            return;
        };
        match (
            timeout(Duration::from_secs(5), reader_handle).await,
            timeout(Duration::from_secs(5), writer_handle).await,
        ) {
            (Ok(Ok(reader)), Ok(Ok(writer))) => {
                if let Err(err) =
                    super::framing::close_connection(&mut reader.unsplit(writer)).await
                {
                    warn!(
                        "Error shutting down stream for client {:?}: {err:?}",
                        client_id
                    );
                }
            }
            _ => {
                warn!(
                    "Timeout or error closing connection for client {:?}",
                    client_id
                );
            }
        }
        if let Some(message_writer_tx) = client.message_writer_tx.take() {
            drop(message_writer_tx);
        }
    }
}

pub static SOCKET_SERVER_EVENT_SENDER: OnceLock<mpsc::Sender<SocketServerEvent>> = OnceLock::new();

pub fn init_socket_server() {
    let args = &get_config().args;
    if !args.tcp_socket && !args.ws_socket {
        return;
    }
    let (event_tx, event_rx) = mpsc::channel(1024);
    let _ = SOCKET_SERVER_EVENT_SENDER.set(event_tx.clone());
    let server = Server::new(event_rx, event_tx);
    tokio::spawn(async move {
        if let Err(e) = server.start().await {
            error!(?e, "server exited with error");
        }
    });
}

pub async fn add_client(transport: Transport) -> Result<()> {
    let _ = SOCKET_SERVER_EVENT_SENDER
        .get()
        .ok_or_else(|| Error::Socket("Socket server event server not initialized".into()))?
        .send(SocketServerEvent::AddClient { transport })
        .await;
    Ok(())
}
