use std::{collections::HashMap, sync::OnceLock};
use tokio::{
    io::{AsyncRead, WriteHalf},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

use kw_types::{
    event::EventPayload,
    socket::{
        AuthFailedReason, Frame, SocketAction, SocketCommand, SocketCommandResponse, SocketMessage,
        SocketQuery, SocketQueryResponse,
    },
};

use super::{
    client::{Client, ClientConnection, ClientId, ClientWriter, command_reply, send_frame},
    event::ClientManagerEvent,
    message::{CorrelatedSocketMessage, RequestKind, SocketRequest},
    transport::Transport,
};
use crate::{
    docker_tracker, observability::history, prelude::*, process_tracker, screen_capture,
    system_resources, systemd,
};

#[derive(Clone)]
struct ChannelSenders {
    request: mpsc::Sender<SocketRequest>,
    event: mpsc::Sender<ClientManagerEvent>,
}

struct ChannelReceivers {
    request: mpsc::Receiver<SocketRequest>,
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
        let (request_tx, request_rx) = mpsc::channel(1024);
        Self {
            clients: HashMap::new(),
            cancel_token,
            senders: ChannelSenders {
                request: request_tx,
                event: event_tx,
            },
            receivers: Some(ChannelReceivers {
                request: request_rx,
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

    fn broadcast_event(&self, payload: &EventPayload) {
        let is_tick = payload.is_tick();
        let auth_enabled = get_config().args.enable_auth;
        let targets = self
            .clients
            .iter()
            .filter(|(_, c)| !auth_enabled || c.is_authenticated())
            .filter(|(_, c)| c.events_enabled())
            .filter(|(_, c)| !is_tick || c.ticks_enabled());
        for (client_id, client) in targets {
            let message = SocketMessage::Event {
                event: payload.clone(),
            };
            if !client.try_send_event(message) {
                debug!("Dropped event for client {client_id}: queue full or closed");
            }
        }
    }

    pub async fn start(mut self) -> Result<()> {
        let ChannelReceivers {
            mut request,
            mut event,
            mut subsystem_event,
        } = self.take_receivers()?;
        loop {
            tokio::select! {
                Some(req) = request.recv() => {
                    self.handle_request(req).await;
                }
                Some(event_req) = event.recv() => {
                    self.handle_event(event_req).await;
                }
                Some(payload) = subsystem_event.recv() => {
                    self.broadcast_event(&payload);
                }
            }
        }
    }

    async fn handle_request(&mut self, req: SocketRequest) {
        let SocketRequest {
            client_id,
            id,
            kind,
        } = req;
        if let RequestKind::Action(action) = kind {
            let Some(reply) = self.handle_action(client_id, action) else {
                return;
            };
            let Some(client) = self.get_client(client_id) else {
                return;
            };
            let shutting_down = matches!(reply, SocketMessage::ShuttingDown);
            if let Err(err) = client.send_frame(id, reply).await {
                debug!("Failed to handle request err: {err}");
            }
            // cancel only after ShuttingDown is on its way
            if shutting_down {
                self.cancel_token.cancel();
            }
            return;
        }
        let Some(client) = self.get_client(client_id) else {
            return;
        };
        let display_user = client.get_display_user();
        if let Some(reply) = access_check(
            &kind,
            get_config().args.enable_auth,
            client.is_authenticated(),
            display_user.is_some(),
        ) {
            if let Err(err) = client.send_frame(id, reply).await {
                debug!("Failed to handle request err: {err}");
            }
            return;
        }
        let Some(writer) = client.writer() else {
            return;
        };
        // TODO: add a Semaphore if a client can flood the server
        tokio::spawn(async move {
            let reply = match kind {
                RequestKind::Query(query) => run_query(query).await,
                // access_check guarantees a display user for commands
                RequestKind::Command(command) => match display_user {
                    Some(user) => run_command(user, command).await,
                    None => return,
                },
                RequestKind::Action(_) => return, // handled above
            };
            if let Err(err) = send_frame(&writer, Frame::reply(id, reply)).await {
                debug!("Failed to handle request err: {err}");
            }
        });
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

    /// Returns the reply to send, or `None` when the client is unknown.
    fn handle_action(
        &mut self,
        client_id: ClientId,
        action: SocketAction,
    ) -> Option<SocketMessage> {
        let client = self.get_client_mut(client_id)?;
        let wrong_credentials = SocketMessage::AuthenticationFailed {
            reason: AuthFailedReason::WrongCredentials,
        };
        let reply = match action {
            SocketAction::Login { username, password } => {
                if client.is_authenticated() {
                    return Some(SocketMessage::AlreadyAuthenticated);
                }
                let Some(users) = get_users().filter(|u| !u.users.is_empty()) else {
                    return Some(wrong_credentials);
                };
                let Some(user) = users.find(&username).cloned() else {
                    return Some(wrong_credentials);
                };
                if !matches!(users.verify_password(&username, &password), Ok(true)) {
                    return Some(wrong_credentials);
                }
                client.set_authentication(true);
                client.set_display_user(user.into());
                SocketMessage::AuthenticationSucceed
            }
            SocketAction::Logout => {
                if client.is_authenticated() {
                    client.clear_display_user();
                    client.set_authentication(false);
                    SocketMessage::LoggedOut
                } else {
                    SocketMessage::NotLoggedIn
                }
            }
            SocketAction::Shutdown => {
                let config = &get_config().args;
                if !config.enable_shutdown {
                    SocketMessage::ShutdownNotEnabled
                } else if config.enable_auth && !client.is_authenticated() {
                    SocketMessage::Unauthorized
                } else {
                    SocketMessage::ShuttingDown
                }
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
                SocketMessage::SetEventPreferences
            }
        };
        Some(reply)
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
        let reader_handle = spawn_client_reader(
            client_id,
            reader,
            self.senders.request.clone(),
            self.senders.event.clone(),
            message_writer_tx.clone(),
            reader_shutdown_rx,
        );
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
                            Some(CorrelatedSocketMessage { frame, response_tx }) => {
                                let result = super::framing::send_message(&mut writer, &frame).await;
                                let mut should_break = false;
                                if result.is_err() {
                                    should_break = true;
                                    warn!("Failed to send message to client {client_id}: {result:?}");
                                    Self::notify_client_disconnection(&event_tx, client_id).await;
                                }
                                if let Some(response_tx) = response_tx {
                                    let _ = response_tx.send(result);
                                }
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

/// `Some(Unauthorized)` when the client may not run this request. Actions do their own checks.
pub fn access_check(
    kind: &RequestKind,
    enable_auth: bool,
    is_authenticated: bool,
    has_display_user: bool,
) -> Option<SocketMessage> {
    let allowed = match kind {
        RequestKind::Action(_) | RequestKind::Query(SocketQuery::Info) => true,
        RequestKind::Query(_) => !enable_auth || is_authenticated,
        RequestKind::Command(_) => is_authenticated && has_display_user,
    };
    (!allowed).then_some(SocketMessage::Unauthorized)
}

async fn run_query(query: SocketQuery) -> SocketMessage {
    let response = match query {
        // Common
        SocketQuery::Info => SocketQueryResponse::Info {
            info: crate::utils::get_info().await,
        },
        SocketQuery::History { query } => SocketQueryResponse::History {
            events: history::query_history(query).await.unwrap_or_default(),
        },
        // Screen Capture
        SocketQuery::Screenshots => SocketQueryResponse::Screenshots {
            screenshots: screen_capture::get_screenshots().await,
        },
        SocketQuery::ScreenCapturePollStatus => SocketQueryResponse::ScreenCapturePollStatus {
            status: screen_capture::get_poll_status().await,
        },
        // Process Tracker
        SocketQuery::RootPids => SocketQueryResponse::RootPids {
            pids: process_tracker::get_root_pids().await,
        },
        SocketQuery::Root { root_pid } => SocketQueryResponse::Root {
            snapshot: process_tracker::get_root(root_pid).await,
        },
        SocketQuery::Children { root_pid } => SocketQueryResponse::Children {
            snapshots: process_tracker::get_children(root_pid).await,
        },
        SocketQuery::IsProcessDone { root_pid } => SocketQueryResponse::IsProcessDone {
            done: process_tracker::is_process_done(root_pid).await,
        },
        SocketQuery::ProcessTree { root_pid } => SocketQueryResponse::ProcessTree {
            tree: process_tracker::get_process_tree(root_pid).await,
        },
        SocketQuery::AllProcessTrees => SocketQueryResponse::AllProcessTrees {
            trees: process_tracker::get_all_process_trees().await,
        },
        SocketQuery::ProcessStatus { root_pid } => SocketQueryResponse::ProcessStatus {
            status: process_tracker::get_process_status(root_pid).await,
        },
        SocketQuery::TopProcesses { by, limit } => SocketQueryResponse::TopProcesses {
            snapshots: process_tracker::get_top_processes(by, limit).await,
        },
        SocketQuery::ProcessTrackerPollStatus => SocketQueryResponse::ProcessTrackerPollStatus {
            status: process_tracker::get_poll_status().await,
        },
        // System Resources
        SocketQuery::SystemSnapshot => SocketQueryResponse::SystemSnapshot {
            snapshot: Box::new(system_resources::get_snapshot().await),
        },
        SocketQuery::Cpu => SocketQueryResponse::Cpu {
            snapshot: system_resources::get_cpu().await,
        },
        SocketQuery::Memory => SocketQueryResponse::Memory {
            snapshot: system_resources::get_memory().await,
        },
        SocketQuery::Disks => SocketQueryResponse::Disks {
            snapshots: system_resources::get_disks().await,
        },
        SocketQuery::Networks => SocketQueryResponse::Networks {
            snapshots: system_resources::get_networks().await,
        },
        SocketQuery::Gpus => SocketQueryResponse::Gpus {
            snapshots: system_resources::get_gpus().await,
        },
        SocketQuery::Battery => SocketQueryResponse::Battery {
            snapshot: system_resources::get_battery().await,
        },
        SocketQuery::HostInfo => SocketQueryResponse::HostInfo {
            info: system_resources::get_host_info().await,
        },
        SocketQuery::Temperatures => SocketQueryResponse::Temperatures {
            snapshots: system_resources::get_temperatures().await,
        },
        SocketQuery::Alarms => SocketQueryResponse::Alarms {
            snapshot: system_resources::get_alarms().await,
        },
        SocketQuery::Thresholds => SocketQueryResponse::Thresholds {
            thresholds: system_resources::get_thresholds().await,
        },
        SocketQuery::RefreshMask => SocketQueryResponse::RefreshMask {
            refresh_mask: system_resources::get_refresh_mask().await,
        },
        SocketQuery::SystemResourcesPollStatus => SocketQueryResponse::SystemResourcesPollStatus {
            status: system_resources::get_poll_status().await,
        },
        // Systemd
        SocketQuery::SystemdSnapshot => SocketQueryResponse::SystemdSnapshot {
            snapshot: systemd::get_snapshot().await,
        },
        SocketQuery::Unit { unit_name } => SocketQueryResponse::Unit {
            snapshot: systemd::get_unit(unit_name).await,
        },
        SocketQuery::UnitsByActiveState { state } => SocketQueryResponse::UnitsByActiveState {
            snapshots: systemd::get_units_by_active_state(state).await,
        },
        SocketQuery::FailedUnits => SocketQueryResponse::FailedUnits {
            snapshots: systemd::get_failed_units().await,
        },
        SocketQuery::SystemdPollStatus => SocketQueryResponse::SystemdPollStatus {
            status: systemd::get_poll_status().await,
        },
        // Docker
        SocketQuery::ListContainers => SocketQueryResponse::ListContainers {
            snapshots: docker_tracker::list_containers().await,
        },
        SocketQuery::Container { id_or_name } => SocketQueryResponse::Container {
            snapshot: docker_tracker::get_container(id_or_name).await,
        },
        SocketQuery::TopContainers { by, limit } => SocketQueryResponse::TopContainers {
            snapshots: docker_tracker::get_top_containers(by, limit).await,
        },
        SocketQuery::DockerTrackerPollStatus => SocketQueryResponse::DockerTrackerPollStatus {
            status: docker_tracker::get_poll_status().await,
        },
    };
    SocketMessage::QueryResponse { response }
}

async fn run_command(user: DisplayUser, command: SocketCommand) -> SocketMessage {
    use SocketCommandResponse as R;
    match command {
        // Screen Capture
        SocketCommand::ScreenCapturePollInterval { interval } => command_reply(
            screen_capture::set_poll_interval(user, interval).await,
            R::ScreenPollInterval,
        ),
        SocketCommand::ScreenCapturePollPause => {
            command_reply(screen_capture::pause_poll(user).await, R::ScreenPollPause)
        }
        SocketCommand::ScreenCapturePollResume => {
            command_reply(screen_capture::resume_poll(user).await, R::ScreenPollResume)
        }
        // Process Tracker: kill replies carry the result payload, so they are built by hand
        SocketCommand::KillProcess { pid, signal } => {
            let result = process_tracker::kill_process(user, pid, signal).await;
            let (success, os_result, err) = match result {
                Ok(val) => (true, val, None),
                Err(e) => (false, false, Some(e)),
            };
            SocketMessage::CommandResponse {
                success,
                err: err.map(|err| err.to_string()),
                response: R::KillProcess { os_result },
            }
        }
        SocketCommand::KillTree { root_pid } => {
            let result = process_tracker::kill_tree(user, root_pid).await;
            let (success, killed_pids, err) = match result {
                Ok(val) => (true, val, None),
                Err(e) => (false, vec![], Some(e)),
            };
            SocketMessage::CommandResponse {
                success,
                err: err.map(|err| err.to_string()),
                response: R::KillTree { killed_pids },
            }
        }
        SocketCommand::TrackPid { pid } => {
            command_reply(process_tracker::track_pid(user, pid).await, R::TrackPid)
        }
        SocketCommand::UntrackPid { pid } => {
            command_reply(process_tracker::untrack_pid(user, pid).await, R::UntrackPid)
        }
        SocketCommand::ProcessTrackerPollInterval { interval } => command_reply(
            process_tracker::set_poll_interval(user, interval).await,
            R::ProcessTrackerPollInterval,
        ),
        SocketCommand::ProcessTrackerPollPause => command_reply(
            process_tracker::pause_poll(user).await,
            R::ProcessTrackerPollPause,
        ),
        SocketCommand::ProcessTrackerPollResume => command_reply(
            process_tracker::resume_poll(user).await,
            R::ProcessTrackerPollResume,
        ),
        // System Resources
        SocketCommand::SetThresholds { thresholds } => command_reply(
            system_resources::set_thresholds(user, thresholds).await,
            R::SetThresholds,
        ),
        SocketCommand::SetRefreshMask { refresh_mask } => command_reply(
            system_resources::set_refresh_mask(user, refresh_mask).await,
            R::SetRefreshMask,
        ),
        SocketCommand::SystemResourcesPollInterval { interval } => command_reply(
            system_resources::set_poll_interval(user, interval).await,
            R::SystemResourcesPollInterval,
        ),
        SocketCommand::SystemResourcesPollPause => command_reply(
            system_resources::pause_poll(user).await,
            R::SystemResourcesPollPause,
        ),
        SocketCommand::SystemResourcesPollResume => command_reply(
            system_resources::resume_poll(user).await,
            R::SystemResourcesPollResume,
        ),
        // Systemd
        SocketCommand::ControlUnit { unit_name, action } => command_reply(
            systemd::control_unit(user, unit_name, action).await,
            R::ControlUnit,
        ),
        SocketCommand::SystemdPollInterval { interval } => command_reply(
            systemd::set_poll_interval(user, interval).await,
            R::SystemdPollInterval,
        ),
        SocketCommand::SystemdPollPause => {
            command_reply(systemd::pause_poll(user).await, R::SystemdPollPause)
        }
        SocketCommand::SystemdPollResume => {
            command_reply(systemd::resume_poll(user).await, R::SystemdPollResume)
        }
        // Docker
        SocketCommand::StopContainer {
            id_or_name,
            timeout_secs,
        } => command_reply(
            docker_tracker::stop_container(user, id_or_name, timeout_secs).await,
            R::StopContainer,
        ),
        SocketCommand::KillContainer { id_or_name, signal } => command_reply(
            docker_tracker::kill_container(user, id_or_name, signal).await,
            R::KillContainer,
        ),
        SocketCommand::StartContainer { id_or_name } => command_reply(
            docker_tracker::start_container(user, id_or_name).await,
            R::StartContainer,
        ),
        SocketCommand::RestartContainer {
            id_or_name,
            timeout_secs,
        } => command_reply(
            docker_tracker::restart_container(user, id_or_name, timeout_secs).await,
            R::RestartContainer,
        ),
        SocketCommand::PauseContainer { id_or_name } => command_reply(
            docker_tracker::pause_container(user, id_or_name).await,
            R::PauseContainer,
        ),
        SocketCommand::UnpauseContainer { id_or_name } => command_reply(
            docker_tracker::unpause_container(user, id_or_name).await,
            R::UnpauseContainer,
        ),
        SocketCommand::DockerTrackerPollInterval { interval } => command_reply(
            docker_tracker::set_poll_interval(user, interval).await,
            R::DockerTrackerPollInterval,
        ),
        SocketCommand::DockerTrackerPollPause => command_reply(
            docker_tracker::pause_poll(user).await,
            R::DockerTrackerPollPause,
        ),
        SocketCommand::DockerTrackerPollResume => command_reply(
            docker_tracker::resume_poll(user).await,
            R::DockerTrackerPollResume,
        ),
    }
}

pub fn spawn_client_reader<R: AsyncRead + Unpin + Send + 'static>(
    client_id: ClientId,
    mut reader: R,
    requests: mpsc::Sender<SocketRequest>,
    manager_events: mpsc::Sender<ClientManagerEvent>,
    writer: ClientWriter,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> JoinHandle<R> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    warn!("Receiver shutting down for client {client_id}");
                    break;
                }
                result = super::framing::receive_message::<_, Frame>(&mut reader) => {
                    let Frame { id, message } = match result {
                        Ok(frame) => frame,
                        // Error::Other is bad JSON and the stream is still at the next frame; anything else (I/O, oversized) is fatal.
                        Err(Error::Other(_)) => {
                            let _ = send_frame(&writer, Frame::push(SocketMessage::InvalidMessage)).await;
                            continue;
                        }
                        Err(err) => {
                            warn!("Client {client_id} disconnected err: {err}");
                            ClientManager::notify_client_disconnection(&manager_events, client_id).await;
                            break;
                        }
                    };
                    let kind = match message {
                        SocketMessage::Action { action } => RequestKind::Action(action),
                        SocketMessage::Query { query } => RequestKind::Query(query),
                        SocketMessage::Command { command } => RequestKind::Command(command),
                        _ => {
                            let _ = send_frame(&writer, Frame::reply(id, SocketMessage::InvalidMessage)).await;
                            continue;
                        }
                    };
                    let _ = requests.send(SocketRequest { client_id, id, kind }).await;
                }
            }
        }
        reader
    })
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

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
