use std::{collections::HashMap, sync::OnceLock};
use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

use super::{
    client::Client,
    event::ClientManagerEvent,
    message::{
        AuthFailedReason, CorrelatedSocketMessage, SocketAction, SocketActionRequset,
        SocketCommand, SocketCommandRequset, SocketCommandResponse, SocketMessage, SocketQuery,
        SocketQueryRequset, SocketQueryResponse,
    },
    transport::Transport,
};
use crate::{prelude::*, screen_capture};

struct ClientManager {
    clients: HashMap<ClientId, Client>,
    cancel_token: CancellationToken,
    action_rx: Option<mpsc::Receiver<SocketActionRequset>>,
    action_tx: mpsc::Sender<SocketActionRequset>,
    query_rx: Option<mpsc::Receiver<SocketQueryRequset>>,
    query_tx: mpsc::Sender<SocketQueryRequset>,
    command_rx: Option<mpsc::Receiver<SocketCommandRequset>>,
    command_tx: mpsc::Sender<SocketCommandRequset>,
    event_rx: Option<mpsc::Receiver<ClientManagerEvent>>,
    event_tx: mpsc::Sender<ClientManagerEvent>,
}

impl ClientManager {
    pub fn new(
        event_rx: mpsc::Receiver<ClientManagerEvent>,
        event_tx: mpsc::Sender<ClientManagerEvent>,
        cancel_token: CancellationToken,
    ) -> Self {
        let (action_tx, action_rx) = mpsc::channel(1024);
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(1024);
        Self {
            clients: HashMap::new(),
            cancel_token,
            action_rx: Some(action_rx),
            action_tx,
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
        let (mut command_rx, mut query_rx, mut event_rx, mut action_rx) = self.take_receivers()?;
        loop {
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
                Some(action_req) = action_rx.recv() => {
                    if let Err(err) = self.handle_action(action_req).await {
                        error!("Failed to handle action err: {err}");
                    }
                }
            }
        }
    }

    pub async fn handle_query(&self, query_req: SocketQueryRequset) -> Result<()> {
        let Some(client) = self.get_client(query_req.client_id) else {
            return Ok(());
        };
        match query_req.query {
            SocketQuery::Info => {
                let args = &crate::prelude::get_config().args;
                let response = kw_types::api::InfoResponse {
                    auth_enabled: args.enable_auth,
                    shutdown_enabled: args.enable_shutdown,
                    blind: args.is_blind(),
                    pid: crate::process_tracker::get_root_pids().await,
                    top_processes: args.top_processes,
                    limit_processes: args.limit_processes,
                    telegram_bot: args.telegram,
                    system_resources: args.system_resources,
                    systemd: args.systemd,
                    docker: args.docker,
                    allow_process_commands: args.allow_process_commands,
                    allow_screen_commands: args.is_screen_commands_allowed(),
                    allow_system_resources_commands: args.allow_system_resources_commands,
                    allow_systemd_commands: args.allow_systemd_commands,
                    allow_docker_commands: args.allow_docker_commands,
                };
                return client
                    .send_message(SocketMessage::QueryResponse {
                        response: SocketQueryResponse::Info { info: response },
                    })
                    .await;
            }
            _ => {}
        }
        if get_config().args.enable_auth && !client.is_authenticated() {
            return client.send_message(SocketMessage::Unauthorized).await;
        }
        match query_req.query {
            SocketQuery::Info => Ok(()),
            SocketQuery::Screenshots => {
                let screenshots = screen_capture::get_screenshots().await;
                return client
                    .send_message(SocketMessage::QueryResponse {
                        response: SocketQueryResponse::Screenshots { screenshots },
                    })
                    .await;
            }
            SocketQuery::ScreenPollStatus => {
                let status = screen_capture::get_poll_status().await;
                return client
                    .send_message(SocketMessage::QueryResponse {
                        response: SocketQueryResponse::ScreenPollStatus { status },
                    })
                    .await;
            }
        }
        // Ok(())
    }

    pub async fn handle_command(&self, command_req: SocketCommandRequset) -> Result<()> {
        let Some(client) = self.get_client(command_req.client_id) else {
            return Ok(());
        };
        if !client.is_authenticated() {
            return client.send_message(SocketMessage::Unauthorized).await;
        }
        let Some(display_user) = client.get_display_user() else {
            return client.send_message(SocketMessage::Unauthorized).await;
        };
        match command_req.command {
            SocketCommand::ScreenPollInterval { interval } => {
                let result = screen_capture::set_poll_interval(display_user, interval).await;
                return client
                    .send_message(SocketMessage::CommandResponse {
                        success: result.is_ok(),
                        err: result.err(),
                        response: SocketCommandResponse::ScreenPollInterval,
                    })
                    .await;
            }
            SocketCommand::ScreenPollPause => {
                let result = screen_capture::pause_poll(display_user).await;
                return client
                    .send_message(SocketMessage::CommandResponse {
                        success: result.is_ok(),
                        err: result.err(),
                        response: SocketCommandResponse::ScreenPollPause,
                    })
                    .await;
            }
            SocketCommand::ScreenPollResume => {
                let result = screen_capture::pause_poll(display_user).await;
                return client
                    .send_message(SocketMessage::CommandResponse {
                        success: result.is_ok(),
                        err: result.err(),
                        response: SocketCommandResponse::ScreenPollResume,
                    })
                    .await;
            }
        }
    }

    pub async fn handle_event(&mut self, event_req: ClientManagerEvent) {
        match event_req {
            ClientManagerEvent::AddClient { transport } => {
                self.add_client(transport);
            }
            ClientManagerEvent::ClientDisconnected { client_id } => {
                self.close_client_connection(client_id).await;
            }
        }
    }

    pub async fn handle_action(&mut self, action_req: SocketActionRequset) -> Result<()> {
        let Some(client) = self.get_client_mut(action_req.client_id) else {
            return Ok(());
        };
        match action_req.action {
            SocketAction::Login { username, password } => {
                if client.is_authenticated() {
                    return Ok(());
                }
                let Some(users) = crate::config::get_users().filter(|u| !u.users.is_empty()) else {
                    return client
                        .send_message(SocketMessage::AuthenticationFailed {
                            reason: AuthFailedReason::WrongCredentials,
                        })
                        .await;
                };
                let Some(user) = users.find(&username).cloned() else {
                    return client
                        .send_message(SocketMessage::AuthenticationFailed {
                            reason: AuthFailedReason::WrongCredentials,
                        })
                        .await;
                };
                match users.verify_password(&username, &password) {
                    Ok(true) => {}
                    _ => {
                        return client
                            .send_message(SocketMessage::AuthenticationFailed {
                                reason: AuthFailedReason::WrongCredentials,
                            })
                            .await;
                    }
                }
                let result = client
                    .send_message(SocketMessage::AuthenticationSucceed)
                    .await;
                if result.is_ok() {
                    client.set_authentication(true);
                    client.set_display_user(user.into());
                }
                return result;
            }
            SocketAction::Logout => {
                client.clear_display_user();
                client.set_authentication(false);
            }
            SocketAction::Shutdown => {
                let config = &get_config().args;
                if !config.enable_shutdown {
                    return client.send_message(SocketMessage::ShutdownNotEnabled).await;
                }
                if config.enable_auth && !client.is_authenticated() {
                    return client.send_message(SocketMessage::Unauthorized).await;
                }
                let result = client.send_message(SocketMessage::ShuttingDown).await;
                self.cancel_token.cancel();
                return result;
            }
        }
        Ok(())
    }

    pub fn take_receivers(
        &mut self,
    ) -> Result<(
        mpsc::Receiver<SocketCommandRequset>,
        mpsc::Receiver<SocketQueryRequset>,
        mpsc::Receiver<ClientManagerEvent>,
        mpsc::Receiver<SocketActionRequset>,
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
        let action_rx = self
            .action_rx
            .take()
            .ok_or_else(|| Error::Socket("Action receiver already taken".into()))?;
        Ok((command_rx, query_rx, event_rx, action_rx))
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
        let action_tx = self.action_tx.clone();
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
                                    SocketMessage::Action { action } => {
                                        let _ = action_tx.send(SocketActionRequset {
                                            client_id,
                                            action
                                        }).await;
                                    }
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
        event_tx: &mpsc::Sender<ClientManagerEvent>,
        client_id: ClientId,
    ) {
        let _ = event_tx.send(ClientManagerEvent::ClientDisconnected { client_id });
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

pub static CLIENT_MANAGER_EVENT_SENDER: OnceLock<mpsc::Sender<ClientManagerEvent>> =
    OnceLock::new();

pub fn init_client_manager(cancel_token: CancellationToken) {
    let args = &get_config().args;
    if !args.tcp_socket && !args.ws_socket {
        return;
    }
    let (event_tx, event_rx) = mpsc::channel(1024);
    let _ = CLIENT_MANAGER_EVENT_SENDER.set(event_tx.clone());
    let client_manager = ClientManager::new(event_rx, event_tx, cancel_token);
    tokio::spawn(async move {
        if let Err(e) = client_manager.start().await {
            error!(?e, "Client manager exited with error");
        }
    });
    info!("Client manager started");
}

pub async fn add_client(transport: Transport) -> Result<()> {
    let _ = CLIENT_MANAGER_EVENT_SENDER
        .get()
        .ok_or_else(|| Error::Socket("Client manager event server not initialized".into()))?
        .send(ClientManagerEvent::AddClient { transport })
        .await;
    Ok(())
}
