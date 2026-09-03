use bollard::Docker;
use futures::StreamExt;
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};
use tokio::sync::{broadcast, mpsc};

use kw_types::{
    docker::{ContainerSnapshot, ContainerStats, ContainerStatus, DockerSortKey},
    polling::Poll,
};

use super::{
    commands::{
        DockerCommandAction, DockerTrackerChannels, DockerTrackerCommand, DockerTrackerQuery,
    },
    event::DockerTrackerEvent,
};
use crate::prelude::*;

/// State held by the tracker between ticks.
struct DockerTrackerState {
    /// All containers seen on the last tick, keyed by full ID.
    containers: HashMap<String, ContainerSnapshot>,
    /// IDs seen on the previous tick — used for appeared/disappeared diffing.
    prev_container_ids: HashSet<String>,
    /// Whether this is the very first tick.
    first_tick: bool,
    /// Top containers sorted by CPU, refreshed each tick.
    last_top_by_cpu: Vec<String>,
    /// Top containers sorted by memory, refreshed each tick.
    last_top_by_memory: Vec<String>,
}

impl DockerTrackerState {
    pub fn new() -> Self {
        Self {
            containers: HashMap::new(),
            prev_container_ids: HashSet::new(),
            first_tick: true,
            last_top_by_cpu: Vec::new(),
            last_top_by_memory: Vec::new(),
        }
    }
}

struct DockerTracker {
    docker: Docker,
    state: DockerTrackerState,
    channels: DockerTrackerChannels,
    poll: Poll,
    limit_containers: usize,
}

impl DockerTracker {
    pub fn new(docker: Docker) -> Self {
        Self {
            docker,
            state: DockerTrackerState::new(),
            channels: DockerTrackerChannels::new(),
            poll: Poll::new(5),
            limit_containers: 20,
        }
    }

    fn emit_event(&self, event: DockerTrackerEvent) {
        // Err means no subscribers — that's fine.
        let _ = self.channels.event_tx.send(event);
    }

    // -------------------------------------------------------------------------
    // Main select loop
    // -------------------------------------------------------------------------

    async fn start_tracking_loop(mut self) -> Result<()> {
        let mut query_rx = self.channels.take_query_rx()?;
        let mut command_rx = self.channels.take_command_rx()?;
        self.poll.resume();
        // Spawn the Docker event stream listener as a separate task that
        // forwards OOM events onto the broadcast bus via a dedicated channel.
        let (oom_tx, mut oom_rx) = mpsc::channel(32);
        tokio::spawn(docker_event_listener(self.docker.clone(), oom_tx));

        loop {
            let tick = async {
                match self.poll.interval_timer.as_mut() {
                    Some(timer) => timer.tick().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(command) = command_rx.recv() => {
                    self.handle_command(command).await;
                }
                Some((id, name)) = oom_rx.recv() => {
                    warn!(id, name, "container OOM-killed");
                    self.emit_event(DockerTrackerEvent::ContainerOomKilled { id, name });
                }
                _ = tick => {
                    if let Err(e) = self.handle_tick().await {
                        error!(?e, "docker tracker tick failed");
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Poll tick
    // -------------------------------------------------------------------------

    async fn handle_tick(&mut self) -> Result<()> {
        // 1. List all containers (running + non-running so we catch exits).
        let mut filters = HashMap::new();
        filters.insert(
            "status".to_string(),
            vec![
                "created".to_string(),
                "removing".to_string(),
                "running".to_string(),
                "paused".to_string(),
                "restarting".to_string(),
                "exited".to_string(),
                "dead".to_string(),
            ],
        );

        let options = bollard::query_parameters::ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();

        let summaries = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(|err| Error::bollard_error(&err))?;

        // 2. Build fresh snapshot map.
        let mut fresh: HashMap<String, ContainerSnapshot> = summaries
            .iter()
            .filter_map(ContainerSnapshot::from_summary)
            .map(|s| (s.id.clone(), s))
            .collect();

        // 3. Fetch stats for every running container concurrently.
        let running_ids: Vec<String> = fresh
            .values()
            .filter(|s| s.status == ContainerStatus::Running)
            .map(|s| s.id.clone())
            .collect();

        let stats_results = futures::future::join_all(running_ids.iter().map(|id| {
            let docker = self.docker.clone();
            let id = id.clone();
            async move {
                let options = bollard::query_parameters::StatsOptionsBuilder::default()
                    .stream(false)
                    .one_shot(true)
                    .build();
                let stats = docker.stats(&id, Some(options)).next().await;
                (id, stats)
            }
        }))
        .await;

        for (id, stats_opt) in stats_results {
            if let Some(Ok(stats)) = stats_opt
                && let Some(snap) = fresh.get_mut(&id)
            {
                snap.stats = Some(ContainerStats::from_bollard(&stats));
            }
        }

        // 4. Diff: appeared / disappeared / changed.
        let current_ids: HashSet<String> = fresh.keys().cloned().collect();

        let appeared_ids: Vec<String> = current_ids
            .difference(&self.state.prev_container_ids)
            .cloned()
            .collect();
        let disappeared_ids: Vec<String> = self
            .state
            .prev_container_ids
            .difference(&current_ids)
            .cloned()
            .collect();

        // 5. Emit events.
        if self.state.first_tick {
            let mut containers: Vec<ContainerSnapshot> = fresh.values().cloned().collect();
            containers.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            info!(count = containers.len(), "docker initial snapshot");
            for c in &containers {
                info!(
                    id = %c.short_id,
                    name = %c.name,
                    image = %c.image,
                    status = %c.status,
                    health = %c.health,
                    "  └─ container"
                );
            }
            self.emit_event(DockerTrackerEvent::InitialSnapshot { containers });
        } else {
            if !appeared_ids.is_empty() {
                let appeared: Vec<ContainerSnapshot> = appeared_ids
                    .iter()
                    .filter_map(|id| fresh.get(id).cloned())
                    .collect();
                for c in &appeared {
                    info!(id = %c.short_id, name = %c.name, image = %c.image, "container appeared");
                }
                self.emit_event(DockerTrackerEvent::ContainersAppeared {
                    containers: appeared,
                });
            }

            if !disappeared_ids.is_empty() {
                let disappeared: Vec<ContainerSnapshot> = disappeared_ids
                    .iter()
                    .filter_map(|id| self.state.containers.get(id).cloned())
                    .collect();
                for c in &disappeared {
                    warn!(id = %c.short_id, name = %c.name, "container disappeared");
                }
                self.emit_event(DockerTrackerEvent::ContainersDisappeared {
                    containers: disappeared,
                });
            }

            for (id, snap) in &fresh {
                if let Some(prev) = self.state.containers.get(id) {
                    if snap.status != prev.status {
                        info!(
                            id = %snap.short_id, name = %snap.name,
                            old = %prev.status, new = %snap.status,
                            "container status changed"
                        );
                        self.emit_event(DockerTrackerEvent::ContainerStatusChanged {
                            container: snap.clone(),
                            previous: prev.status.clone(),
                        });
                    }
                    if snap.health != prev.health {
                        info!(
                            id = %snap.short_id, name = %snap.name,
                            old = %prev.health, new = %snap.health,
                            "container health changed"
                        );
                        self.emit_event(DockerTrackerEvent::ContainerHealthChanged {
                            container: snap.clone(),
                            previous: prev.health.clone(),
                        });
                    }
                }
            }
        }

        // 6. Update tops.
        self.update_top_containers(&fresh);

        // 7. Commit state.
        self.state.containers = fresh;
        self.state.prev_container_ids = current_ids;
        self.state.first_tick = false;

        Ok(())
    }

    fn update_top_containers(&mut self, fresh: &HashMap<String, ContainerSnapshot>) {
        let mut with_stats: Vec<&ContainerSnapshot> =
            fresh.values().filter(|s| s.stats.is_some()).collect();

        with_stats.sort_unstable_by(|a, b| {
            let ca = a.stats.as_ref().map_or(0.0, |s| s.cpu_percent);
            let cb = b.stats.as_ref().map_or(0.0, |s| s.cpu_percent);
            cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
        });
        self.state.last_top_by_cpu = with_stats
            .iter()
            .take(self.limit_containers)
            .map(|s| s.id.clone()) // <-- just the ID
            .collect();

        with_stats.sort_unstable_by(|a, b| {
            let ma = a.stats.as_ref().map_or(0, |s| s.memory_bytes);
            let mb = b.stats.as_ref().map_or(0, |s| s.memory_bytes);
            mb.cmp(&ma)
        });
        self.state.last_top_by_memory = with_stats
            .iter()
            .take(self.limit_containers)
            .map(|s| s.id.clone()) // <-- just the ID
            .collect();
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    fn handle_query(&self, query: DockerTrackerQuery) {
        match query {
            DockerTrackerQuery::ListContainers { response } => {
                let mut containers: Vec<ContainerSnapshot> =
                    self.state.containers.values().cloned().collect();
                containers.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                let _ = response.send(containers);
            }
            DockerTrackerQuery::GetContainer {
                id_or_name,
                response,
            } => {
                let found = self
                    .state
                    .containers
                    .values()
                    .find(|c| c.id.starts_with(&id_or_name) || c.name == id_or_name)
                    .cloned();
                let _ = response.send(found);
            }
            DockerTrackerQuery::GetTopContainers {
                by,
                limit,
                response,
            } => {
                let limit = if limit == 0 {
                    self.limit_containers
                } else {
                    limit
                };
                let ids = match by {
                    DockerSortKey::Cpu => &self.state.last_top_by_cpu,
                    DockerSortKey::Memory => &self.state.last_top_by_memory,
                };
                let result: Vec<ContainerSnapshot> = ids
                    .iter()
                    .take(limit)
                    .filter_map(|id| self.state.containers.get(id).cloned())
                    .collect();
                let _ = response.send(result);
            }
            DockerTrackerQuery::PollStatus { response } => {
                let _ = response.send(Some((&self.poll).into()));
            }
        }
    }

    // -------------------------------------------------------------------------
    // Commands
    // -------------------------------------------------------------------------

    async fn handle_command(&mut self, command: DockerTrackerCommand) {
        match command {
            DockerTrackerCommand::StopContainer {
                user,
                id_or_name,
                timeout_secs,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let opts = timeout_secs.map(|t| {
                    bollard::query_parameters::StopContainerOptionsBuilder::default()
                        .t(t)
                        .build()
                });
                let result = self
                    .docker
                    .stop_container(&target, opts)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Stop {
                        id_or_name,
                        container_name,
                        timeout_secs,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::KillContainer {
                user,
                id_or_name,
                signal,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let opts = signal.as_deref().map(|s| {
                    bollard::query_parameters::KillContainerOptionsBuilder::default()
                        .signal(s)
                        .build()
                });
                let result = self
                    .docker
                    .kill_container(&target, opts)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Kill {
                        id_or_name,
                        container_name,
                        signal,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::StartContainer {
                user,
                id_or_name,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let result = self
                    .docker
                    .start_container(&target, None)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Start {
                        id_or_name,
                        container_name,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::RestartContainer {
                user,
                id_or_name,
                timeout_secs,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let opts = timeout_secs.map(|t| {
                    bollard::query_parameters::RestartContainerOptionsBuilder::default()
                        .t(t)
                        .build()
                });
                let result = self
                    .docker
                    .restart_container(&target, opts)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Restart {
                        id_or_name,
                        container_name,
                        timeout_secs,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::PauseContainer {
                user,
                id_or_name,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let result = self
                    .docker
                    .pause_container(&target)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Pause {
                        id_or_name,
                        container_name,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::UnpauseContainer {
                user,
                id_or_name,
                response,
            } => {
                let target = self.resolve_id(&id_or_name);
                let container_name = self.lookup_container_name(&id_or_name);
                let result = self
                    .docker
                    .unpause_container(&target)
                    .await
                    .map_err(|err| Error::bollard_error(&err));
                self.emit_command_event(
                    user,
                    DockerCommandAction::Unpause {
                        id_or_name,
                        container_name,
                    },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::SetPollInterval {
                user,
                interval,
                response,
            } => {
                self.poll.set_interval(interval);
                info!(
                    ms = interval.as_millis(),
                    "docker tracker poll interval updated"
                );
                let result = Ok(());
                self.emit_command_event(
                    user,
                    DockerCommandAction::SetPollInterval { interval },
                    &result,
                );
                let _ = response.send(result);
            }

            DockerTrackerCommand::PausePoll { user, response } => {
                self.poll.pause();
                info!("docker tracker polling paused");
                let result = Ok(());
                self.emit_command_event(user, DockerCommandAction::PausePoll, &result);
                let _ = response.send(result);
            }

            DockerTrackerCommand::ResumePoll { user, response } => {
                self.poll.resume();
                info!("docker tracker polling resumed");
                let result = Ok(());
                self.emit_command_event(user, DockerCommandAction::ResumePoll, &result);
                let _ = response.send(result);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn resolve_id(&self, id_or_name: &str) -> String {
        self.state
            .containers
            .values()
            .find(|c| c.id.starts_with(id_or_name) || c.name == id_or_name)
            .map_or_else(|| id_or_name.to_owned(), |c| c.id.clone())
    }

    /// Looks up the current name of a container by id or name, if tracked.
    fn lookup_container_name(&self, id_or_name: &str) -> Option<String> {
        self.state
            .containers
            .values()
            .find(|c| c.id.starts_with(id_or_name) || c.name == id_or_name)
            .map(|c| c.name.clone())
    }

    /// Emits a `CommandExecuted` event for any mutating command, container-scoped
    /// or not — target info already lives on `action`.
    fn emit_command_event(
        &self,
        user: DisplayUser,
        action: DockerCommandAction,
        result: &Result<()>,
    ) {
        let (success, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };

        if success {
            info!(
                %user,
                action = %action,
                "docker command executed"
            );
        } else {
            warn!(
                %user,
                action = %action,
                error,
                "docker command failed"
            );
        }

        self.emit_event(DockerTrackerEvent::CommandExecuted {
            user,
            action,
            success,
            error,
        });
    }
}

// ============================================================================
// Docker event stream listener (OOM events only)
// ============================================================================

async fn docker_event_listener(docker: Docker, oom_tx: mpsc::Sender<(String, String)>) {
    let mut filters = HashMap::new();
    filters.insert("type".to_string(), vec!["container".to_string()]);
    filters.insert("event".to_string(), vec!["oom".to_string()]);

    let opts = bollard::query_parameters::EventsOptionsBuilder::default()
        .filters(&filters)
        .build();

    let mut stream = docker.events(Some(opts));

    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => {
                let id = event
                    .actor
                    .as_ref()
                    .and_then(|a| a.id.clone())
                    .unwrap_or_default();
                let name = event
                    .actor
                    .as_ref()
                    .and_then(|a| a.attributes.as_ref())
                    .and_then(|attrs| attrs.get("name"))
                    .cloned()
                    .unwrap_or_else(|| id.chars().take(12).collect());

                if oom_tx.send((id, name)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                error!(?e, "docker event stream error; reconnecting in 5 s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                break;
            }
        }
    }
}

// ============================================================================
// Static handles
// ============================================================================

pub static DOCKER_TRACKER_QUERY_SENDER: OnceLock<mpsc::Sender<DockerTrackerQuery>> =
    OnceLock::new();
pub static DOCKER_TRACKER_EVENT_SENDER: OnceLock<broadcast::Sender<DockerTrackerEvent>> =
    OnceLock::new();
pub static DOCKER_TRACKER_COMMAND_SENDER: OnceLock<mpsc::Sender<DockerTrackerCommand>> =
    OnceLock::new();

static DOCKER_TRACKER: OnceLock<Mutex<Option<DockerTracker>>> = OnceLock::new();

pub fn init_docker_tracker() {
    let config = get_config();
    if !config.args.docker {
        return;
    }
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            error!(
                ?e,
                "failed to connect to Docker daemon — skipping docker tracker"
            );
            return;
        }
    };
    let tracker = DockerTracker::new(docker);
    let _ = DOCKER_TRACKER_QUERY_SENDER.set(tracker.channels.query_tx.clone());
    let _ = DOCKER_TRACKER_EVENT_SENDER.set(tracker.channels.event_tx.clone());
    if config.args.allow_docker_commands {
        let _ = DOCKER_TRACKER_COMMAND_SENDER.set(tracker.channels.command_tx.clone());
    }
    let _ = DOCKER_TRACKER.set(Mutex::new(Some(tracker)));
}

pub fn start_docker_tracker() {
    let Some(tracker) = DOCKER_TRACKER
        .get()
        .and_then(|cell| cell.lock().ok())
        .and_then(|mut guard| guard.take())
    else {
        return;
    };
    tokio::spawn(async move {
        if let Err(e) = tracker.start_tracking_loop().await {
            error!(?e, "docker tracker loop exited with error");
        }
    });
    info!("Docker Tracker started");
}
