use std::{collections::HashMap, sync::OnceLock};
use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

use super::{
    client::{Client, ClientConnection, ClientId},
    event::ClientManagerEvent,
    message::{
        AuthFailedReason, CorrelatedSocketMessage, SocketAction, SocketActionRequset,
        SocketCommand, SocketCommandRequset, SocketCommandResponse, SocketMessage, SocketQuery,
        SocketQueryRequset, SocketQueryResponse,
    },
    transport::Transport,
};
use crate::{events::EventPayload, prelude::*, screen_capture};

#[derive(Clone)]
struct ChannelSenders {
    action_tx: mpsc::Sender<SocketActionRequset>,
    query_tx: mpsc::Sender<SocketQueryRequset>,
    command_tx: mpsc::Sender<SocketCommandRequset>,
    event_tx: mpsc::Sender<ClientManagerEvent>,
}

struct ChannelReceivers {
    action_rx: mpsc::Receiver<SocketActionRequset>,
    query_rx: mpsc::Receiver<SocketQueryRequset>,
    command_rx: mpsc::Receiver<SocketCommandRequset>,
    event_rx: mpsc::Receiver<ClientManagerEvent>,
    subsystem_event_rx: mpsc::Receiver<EventPayload>,
}

struct ClientManager {
    clients: HashMap<ClientId, Client>,
    cancel_token: CancellationToken,
    senders: ChannelSenders,
    receivers: Option<ChannelReceivers>,
}

impl ClientManager {
    pub fn new(
        event_rx: mpsc::Receiver<ClientManagerEvent>,
        event_tx: mpsc::Sender<ClientManagerEvent>,
        subsystem_event_rx: mpsc::Receiver<EventPayload>,
        cancel_token: CancellationToken,
    ) -> Self {
        let (action_tx, action_rx) = mpsc::channel(1024);
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(1024);
        Self {
            clients: HashMap::new(),
            cancel_token,
            senders: ChannelSenders {
                action_tx,
                query_tx,
                command_tx,
                event_tx,
            },
            receivers: Some(ChannelReceivers {
                action_rx,
                query_rx,
                command_rx,
                event_rx,
                subsystem_event_rx,
            }),
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
        let connection = self.setup_client_connection(transport, client_id);
        let client = Client::new(client_id, connection);
        self.clients.insert(client_id, client);
    }

    async fn broadcast_event(&self, payload: EventPayload) {
        let is_tick = payload.is_tick();
        let sends = self
            .clients
            .values()
            .filter(|c| c.is_authenticated() && c.events_enabled())
            .filter(|c| !is_tick || c.ticks_enabled())
            .map(|client| {
                let message = SocketMessage::Event {
                    event: payload.clone(),
                };
                async move {
                    if let Err(err) = client.send_message(message).await {
                        warn!("Failed to broadcast event to client: {err}");
                    }
                }
            });
        futures::future::join_all(sends).await;
    }

    pub async fn start(mut self) -> Result<()> {
        let ChannelReceivers {
            mut command_rx,
            mut query_rx,
            mut event_rx,
            mut action_rx,
            mut subsystem_event_rx,
        } = self.take_receivers()?;
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
                Some(payload) = subsystem_event_rx.recv() => {
                    self.broadcast_event(payload).await;
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
            SocketAction::SetEventPreferences {
                events_enabled,
                ticks_enabled,
            } => {
                if let Some(v) = events_enabled {
                    client.set_events_enabled(v);
                }
                if let Some(v) = ticks_enabled {
                    client.set_ticks_enabled(v);
                }
                return client
                    .send_message(SocketMessage::SetEventPreferences)
                    .await;
            }
        }
        Ok(())
    }

    pub fn take_receivers(&mut self) -> Result<ChannelReceivers> {
        self.receivers
            .take()
            .ok_or_else(|| Error::Socket("Channel receivers already taken".into()))
    }

    pub fn setup_client_connection(
        &mut self,
        transport: Transport,
        client_id: ClientId,
    ) -> ClientConnection {
        let (reader, writer) = tokio::io::split(transport);
        let (reader_shutdown_tx, reader_shutdown_rx) = oneshot::channel();
        let (writer_shutdown_tx, writer_shutdown_rx) = oneshot::channel();
        let (message_writer_tx, message_writer_rx) = mpsc::channel(1024);
        let reader_handle = self.setup_client_receiver(client_id, reader, reader_shutdown_rx);
        let writer_handle =
            self.setup_client_sender(client_id, writer, message_writer_rx, writer_shutdown_rx);
        ClientConnection {
            reader_shutdown_tx,
            writer_shutdown_tx,
            reader_handle,
            writer_handle,
            message_writer_tx,
        }
    }

    pub fn setup_client_receiver(
        &self,
        client_id: ClientId,
        mut reader: ReadHalf<Transport>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> JoinHandle<ReadHalf<Transport>> {
        let senders = self.senders.clone();
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
                                        let _ = senders.action_tx.send(SocketActionRequset {
                                            client_id,
                                            action
                                        }).await;
                                    }
                                    SocketMessage::Query { query } => {
                                        let _ = senders.query_tx.send(SocketQueryRequset {
                                            client_id,
                                            query
                                        }).await;
                                    }
                                    SocketMessage::Command { command } => {
                                        let _ = senders.command_tx.send(SocketCommandRequset {
                                            client_id,
                                            command
                                        }).await;
                                    }
                                    _ => {}
                                }
                            }
                            Err(err) => {
                                warn!("Client {client_id} disconnected err: {err}");
                                Self::notify_client_disconnection(&senders.event_tx, client_id);
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
        let event_tx = self.senders.event_tx.clone();
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
        let Some(connection) = client.take_connection() else {
            return;
        };
        let _ = connection.reader_shutdown_tx.send(());
        let _ = connection.writer_shutdown_tx.send(());
        match (
            timeout(Duration::from_secs(5), connection.reader_handle).await,
            timeout(Duration::from_secs(5), connection.writer_handle).await,
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
        drop(connection.message_writer_tx);
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
    let (subsystem_event_tx, subsystem_event_rx) = mpsc::channel(1024);
    super::dispatcher::spawn_event_dispatcher(subsystem_event_tx, cancel_token.clone());
    let client_manager = ClientManager::new(event_rx, event_tx, subsystem_event_rx, cancel_token);
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
