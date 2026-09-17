use std::{collections::HashMap, sync::OnceLock};
use tokio::{
    io::{ReadHalf, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

use kw_types::{
    event::EventPayload,
    socket::{
        AuthFailedReason, SocketAction, SocketCommand, SocketCommandResponse, SocketMessage,
        SocketQuery, SocketQueryResponse,
    },
};

use super::{
    client::{Client, ClientConnection, ClientId},
    event::ClientManagerEvent,
    message::{
        CorrelatedSocketMessage, SocketActionRequset, SocketCommandRequset, SocketQueryRequset,
    },
    transport::Transport,
};
use crate::{
    docker_tracker, prelude::*, process_tracker, screen_capture, system_resources, systemd,
};

#[derive(Clone)]
struct ChannelSenders {
    action: mpsc::Sender<SocketActionRequset>,
    query: mpsc::Sender<SocketQueryRequset>,
    command: mpsc::Sender<SocketCommandRequset>,
    event: mpsc::Sender<ClientManagerEvent>,
}

struct ChannelReceivers {
    action: mpsc::Receiver<SocketActionRequset>,
    query: mpsc::Receiver<SocketQueryRequset>,
    command: mpsc::Receiver<SocketCommandRequset>,
    event: mpsc::Receiver<ClientManagerEvent>,
    subsystem_event: mpsc::Receiver<EventPayload>,
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
                action: action_tx,
                query: query_tx,
                command: command_tx,
                event: event_tx,
            },
            receivers: Some(ChannelReceivers {
                action: action_rx,
                query: query_rx,
                command: command_rx,
                event: event_rx,
                subsystem_event: subsystem_event_rx,
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
            mut command,
            mut query,
            mut event,
            mut action,
            mut subsystem_event,
        } = self.take_receivers()?;
        loop {
            tokio::select! {
                Some(query_req) = query.recv() => {
                    if let Err(err) = self.handle_query(query_req).await {
                        error!("Failed to handle query err: {err}");
                    }
                }
                Some(command_req) = command.recv() => {
                    if let Err(err) = self.handle_command(command_req).await {
                        error!("Failed to handle command err: {err}");
                    }
                }
                Some(event_req) = event.recv() => {
                    self.handle_event(event_req).await;
                }
                Some(action_req) = action.recv() => {
                    if let Err(err) = self.handle_action(action_req).await {
                        error!("Failed to handle action err: {err}");
                    }
                }
                Some(payload) = subsystem_event.recv() => {
                    self.broadcast_event(payload).await;
                }
            }
        }
    }

    pub async fn handle_query(&self, query_req: SocketQueryRequset) -> Result<()> {
        let Some(client) = self.get_client(query_req.client_id) else {
            return Ok(());
        };
        if matches!(query_req.query, SocketQuery::Info) {
            return self.send_info(client).await;
        }
        if get_config().args.enable_auth && !client.is_authenticated() {
            return client.send_message(SocketMessage::Unauthorized).await;
        }
        match query_req.query {
            // Common
            SocketQuery::Info => return self.send_info(client).await,
            // Screen Capture
            SocketQuery::Screenshots => {
                let screenshots = screen_capture::get_screenshots().await;
                return client
                    .respond_to_query(SocketQueryResponse::Screenshots { screenshots })
                    .await;
            }
            SocketQuery::ScreenCapturePollStatus => {
                let status = screen_capture::get_poll_status().await;
                return client
                    .respond_to_query(SocketQueryResponse::ScreenCapturePollStatus { status })
                    .await;
            }
            // Process Tracker
            SocketQuery::RootPids => {
                let pids = process_tracker::get_root_pids().await;
                return client
                    .respond_to_query(SocketQueryResponse::RootPids { pids })
                    .await;
            }
            SocketQuery::Root { root_pid } => {
                let snapshot = process_tracker::get_root(root_pid).await;
                return client
                    .respond_to_query(SocketQueryResponse::Root { snapshot })
                    .await;
            }
            SocketQuery::Children { root_pid } => {
                let snapshots = process_tracker::get_children(root_pid).await;
                return client
                    .respond_to_query(SocketQueryResponse::Children { snapshots })
                    .await;
            }
            SocketQuery::IsProcessDone { root_pid } => {
                let done = process_tracker::is_process_done(root_pid).await;
                return client
                    .respond_to_query(SocketQueryResponse::IsProcessDone { done })
                    .await;
            }
            SocketQuery::ProcessTree { root_pid } => {
                let tree = process_tracker::get_process_tree(root_pid).await;
                return client
                    .respond_to_query(SocketQueryResponse::ProcessTree { tree })
                    .await;
            }
            SocketQuery::AllProcessTrees => {
                let trees = process_tracker::get_all_process_trees().await;
                return client
                    .respond_to_query(SocketQueryResponse::AllProcessTrees { trees })
                    .await;
            }
            SocketQuery::ProcessStatus { root_pid } => {
                let status = process_tracker::get_process_status(root_pid).await;
                return client
                    .respond_to_query(SocketQueryResponse::ProcessStatus { status })
                    .await;
            }
            SocketQuery::TopProcesses { by, limit } => {
                let snapshots = process_tracker::get_top_processes(by, limit).await;
                return client
                    .respond_to_query(SocketQueryResponse::TopProcesses { snapshots })
                    .await;
            }
            SocketQuery::ProcessTrackerPollStatus => {
                let status = process_tracker::get_poll_status().await;
                return client
                    .respond_to_query(SocketQueryResponse::ProcessTrackerPollStatus { status })
                    .await;
            }
            // System Resources
            SocketQuery::SystemSnapshot => {
                let snapshot = system_resources::get_snapshot().await;
                return client
                    .respond_to_query(SocketQueryResponse::SystemSnapshot {
                        snapshot: Box::new(snapshot),
                    })
                    .await;
            }
            SocketQuery::Cpu => {
                let snapshot = system_resources::get_cpu().await;
                return client
                    .respond_to_query(SocketQueryResponse::Cpu { snapshot })
                    .await;
            }
            SocketQuery::Memory => {
                let snapshot = system_resources::get_memory().await;
                return client
                    .respond_to_query(SocketQueryResponse::Memory { snapshot })
                    .await;
            }
            SocketQuery::Disks => {
                let snapshots = system_resources::get_disks().await;
                return client
                    .respond_to_query(SocketQueryResponse::Disks { snapshots })
                    .await;
            }
            SocketQuery::Networks => {
                let snapshots = system_resources::get_networks().await;
                return client
                    .respond_to_query(SocketQueryResponse::Networks { snapshots })
                    .await;
            }
            SocketQuery::Gpus => {
                let snapshots = system_resources::get_gpus().await;
                return client
                    .respond_to_query(SocketQueryResponse::Gpus { snapshots })
                    .await;
            }
            SocketQuery::Battery => {
                let snapshot = system_resources::get_battery().await;
                return client
                    .respond_to_query(SocketQueryResponse::Battery { snapshot })
                    .await;
            }
            SocketQuery::HostInfo => {
                let info = system_resources::get_host_info().await;
                return client
                    .respond_to_query(SocketQueryResponse::HostInfo { info })
                    .await;
            }
            SocketQuery::Temperatures => {
                let snapshots = system_resources::get_temperatures().await;
                return client
                    .respond_to_query(SocketQueryResponse::Temperatures { snapshots })
                    .await;
            }
            SocketQuery::Alarms => {
                let snapshot = system_resources::get_alarms().await;
                return client
                    .respond_to_query(SocketQueryResponse::Alarms { snapshot })
                    .await;
            }
            SocketQuery::Thresholds => {
                let thresholds = system_resources::get_thresholds().await;
                return client
                    .respond_to_query(SocketQueryResponse::Thresholds { thresholds })
                    .await;
            }
            SocketQuery::RefreshMask => {
                let refresh_mask = system_resources::get_refresh_mask().await;
                return client
                    .respond_to_query(SocketQueryResponse::RefreshMask { refresh_mask })
                    .await;
            }
            SocketQuery::SystemResourcesPollStatus => {
                let status = system_resources::get_poll_status().await;
                return client
                    .respond_to_query(SocketQueryResponse::SystemResourcesPollStatus { status })
                    .await;
            }
            // Systemd
            SocketQuery::SystemdSnapshot => {
                let snapshot = systemd::get_snapshot().await;
                return client
                    .respond_to_query(SocketQueryResponse::SystemdSnapshot { snapshot })
                    .await;
            }
            SocketQuery::Unit { unit_name } => {
                let snapshot = systemd::get_unit(unit_name).await;
                return client
                    .respond_to_query(SocketQueryResponse::Unit { snapshot })
                    .await;
            }
            SocketQuery::UnitsByActiveState { state } => {
                let snapshots = systemd::get_units_by_active_state(state).await;
                return client
                    .respond_to_query(SocketQueryResponse::UnitsByActiveState { snapshots })
                    .await;
            }
            SocketQuery::FailedUnits => {
                let snapshots = systemd::get_failed_units().await;
                return client
                    .respond_to_query(SocketQueryResponse::FailedUnits { snapshots })
                    .await;
            }
            SocketQuery::SystemdPollStatus => {
                let status = systemd::get_poll_status().await;
                return client
                    .respond_to_query(SocketQueryResponse::SystemdPollStatus { status })
                    .await;
            }
            SocketQuery::ListContainers => {
                let snapshots = docker_tracker::list_containers().await;
                return client
                    .respond_to_query(SocketQueryResponse::ListContainers { snapshots })
                    .await;
            }
            SocketQuery::Container { id_or_name } => {
                let snapshot = docker_tracker::get_container(id_or_name).await;
                return client
                    .respond_to_query(SocketQueryResponse::Container { snapshot })
                    .await;
            }
            SocketQuery::TopContainers { by, limit } => {
                let snapshots = docker_tracker::get_top_containers(by, limit).await;
                return client
                    .respond_to_query(SocketQueryResponse::TopContainers { snapshots })
                    .await;
            }
            SocketQuery::DockerTrackerPollStatus => {
                let status = docker_tracker::get_poll_status().await;
                return client
                    .respond_to_query(SocketQueryResponse::DockerTrackerPollStatus { status })
                    .await;
            }
        }
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
            // Screen Capture
            SocketCommand::ScreenCapturePollInterval { interval } => {
                let result = screen_capture::set_poll_interval(display_user, interval).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ScreenPollInterval)
                    .await;
            }
            SocketCommand::ScreenCapturePollPause => {
                let result = screen_capture::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ScreenPollPause)
                    .await;
            }
            SocketCommand::ScreenCapturePollResume => {
                let result = screen_capture::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ScreenPollResume)
                    .await;
            }
            // Process Tracker
            SocketCommand::KillProcess { pid, signal } => {
                let result = process_tracker::kill_process(display_user, pid, signal).await;
                let (success, os_result, err) = match result {
                    Ok(val) => (true, val, None),
                    Err(e) => (false, false, Some(e)),
                };
                return client
                    .send_message(SocketMessage::CommandResponse {
                        success,
                        err: err.map(|err| err.to_string()),
                        response: SocketCommandResponse::KillProcess { os_result },
                    })
                    .await;
            }
            SocketCommand::KillTree { root_pid } => {
                let result = process_tracker::kill_tree(display_user, root_pid).await;
                let (success, killed_pids, err) = match result {
                    Ok(val) => (true, val, None),
                    Err(e) => (false, vec![], Some(e)),
                };
                return client
                    .send_message(SocketMessage::CommandResponse {
                        success,
                        err: err.map(|err| err.to_string()),
                        response: SocketCommandResponse::KillTree { killed_pids },
                    })
                    .await;
            }
            SocketCommand::TrackPid { pid } => {
                let result = process_tracker::track_pid(display_user, pid).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::TrackPid)
                    .await;
            }
            SocketCommand::UntrackPid { pid } => {
                let result = process_tracker::untrack_pid(display_user, pid).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::UntrackPid)
                    .await;
            }
            SocketCommand::ProcessTrackerPollInterval { interval } => {
                let result = process_tracker::set_poll_interval(display_user, interval).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ProcessTrackerPollInterval)
                    .await;
            }
            SocketCommand::ProcessTrackerPollPause => {
                let result = process_tracker::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ProcessTrackerPollPause)
                    .await;
            }
            SocketCommand::ProcessTrackerPollResume => {
                let result = process_tracker::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ProcessTrackerPollResume)
                    .await;
            }
            // System Resources
            SocketCommand::SetThresholds { thresholds } => {
                let result = system_resources::set_thresholds(display_user, thresholds).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SetThresholds)
                    .await;
            }
            SocketCommand::SetRefreshMask { refresh_mask } => {
                let result = system_resources::set_refresh_mask(display_user, refresh_mask).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SetRefreshMask)
                    .await;
            }
            SocketCommand::SystemResourcesPollInterval { interval } => {
                let result = system_resources::set_poll_interval(display_user, interval).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemResourcesPollInterval)
                    .await;
            }
            SocketCommand::SystemResourcesPollPause => {
                let result = system_resources::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemResourcesPollPause)
                    .await;
            }
            SocketCommand::SystemResourcesPollResume => {
                let result = system_resources::resume_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemResourcesPollResume)
                    .await;
            }
            SocketCommand::ControlUnit { unit_name, action } => {
                let result = systemd::control_unit(display_user, unit_name, action).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::ControlUnit)
                    .await;
            }
            SocketCommand::SystemdPollInterval { interval } => {
                let result = systemd::set_poll_interval(display_user, interval).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemdPollInterval)
                    .await;
            }
            SocketCommand::SystemdPollPause => {
                let result = systemd::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemdPollPause)
                    .await;
            }
            SocketCommand::SystemdPollResume => {
                let result = systemd::resume_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::SystemdPollResume)
                    .await;
            }
            SocketCommand::StopContainer {
                id_or_name,
                timeout_secs,
            } => {
                let result =
                    docker_tracker::stop_container(display_user, id_or_name, timeout_secs).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::StopContainer)
                    .await;
            }
            SocketCommand::KillContainer { id_or_name, signal } => {
                let result = docker_tracker::kill_container(display_user, id_or_name, signal).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::KillContainer)
                    .await;
            }
            SocketCommand::StartContainer { id_or_name } => {
                let result = docker_tracker::start_container(display_user, id_or_name).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::StartContainer)
                    .await;
            }
            SocketCommand::RestartContainer {
                id_or_name,
                timeout_secs,
            } => {
                let result =
                    docker_tracker::restart_container(display_user, id_or_name, timeout_secs).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::RestartContainer)
                    .await;
            }
            SocketCommand::PauseContainer { id_or_name } => {
                let result = docker_tracker::pause_container(display_user, id_or_name).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::PauseContainer)
                    .await;
            }
            SocketCommand::UnpauseContainer { id_or_name } => {
                let result = docker_tracker::unpause_container(display_user, id_or_name).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::UnpauseContainer)
                    .await;
            }
            SocketCommand::DockerTrackerPollInterval { interval } => {
                let result = docker_tracker::set_poll_interval(display_user, interval).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::DockerTrackerPollInterval)
                    .await;
            }
            SocketCommand::DockerTrackerPollPause => {
                let result = docker_tracker::pause_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::DockerTrackerPollPause)
                    .await;
            }
            SocketCommand::DockerTrackerPollResume => {
                let result = docker_tracker::resume_poll(display_user).await;
                return client
                    .respond_to_command(result, SocketCommandResponse::DockerTrackerPollResume)
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

    async fn send_info(&self, client: &Client) -> Result<()> {
        let info = crate::utils::get_info().await;
        client
            .respond_to_query(SocketQueryResponse::Info { info })
            .await
    }

    pub fn take_receivers(&mut self) -> Result<ChannelReceivers> {
        self.receivers
            .take()
            .ok_or_else(|| Error::Socket("Channel receivers already taken".into()))
    }

    pub fn setup_client_connection(
        &self,
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
                                        let _ = senders.action.send(SocketActionRequset {
                                            client_id,
                                            action
                                        }).await;
                                    }
                                    SocketMessage::Query { query } => {
                                        let _ = senders.query.send(SocketQueryRequset {
                                            client_id,
                                            query
                                        }).await;
                                    }
                                    SocketMessage::Command { command } => {
                                        let _ = senders.command.send(SocketCommandRequset {
                                            client_id,
                                            command
                                        }).await;
                                    }
                                    _ => {}
                                }
                            }
                            Err(err) => {
                                warn!("Client {client_id} disconnected err: {err}");
                                Self::notify_client_disconnection(&senders.event, client_id).await;
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
        let event_tx = self.senders.event.clone();
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
                                    Self::notify_client_disconnection(&event_tx, client_id).await;
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

    pub async fn notify_client_disconnection(
        event_tx: &mpsc::Sender<ClientManagerEvent>,
        client_id: ClientId,
    ) {
        let _ = event_tx
            .send(ClientManagerEvent::ClientDisconnected { client_id })
            .await;
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
        .ok_or_else(|| Error::Socket("Client manager event sender not initialized".into()))?
        .send(ClientManagerEvent::AddClient { transport })
        .await;
    Ok(())
}
