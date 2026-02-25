use std::{collections::HashSet, sync::OnceLock};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tokio::{
    sync::{mpsc, oneshot},
    time::Duration,
};

use crate::prelude::*;

/// Events emitted by the tracker on its broadcast bus.
/// Subscribers receive these without polling.
#[derive(Debug, Clone)]
pub enum ProcessTrackerEvent {
    /// Emitted on the very first tick; contains everything we found.
    InitialSnapshot {
        root: ProcessSnapshot,
        children: Vec<ProcessSnapshot>,
    },
    /// One or more new child processes appeared.
    ChildrenAppeared(Vec<ProcessSnapshot>),
    /// One or more child PIDs exited.
    ChildrenExited(Vec<u32>),
    /// All descendants have exited (root may still be alive).
    AllChildrenGone,
    /// The root process itself has exited.
    RootExited {
        pid: u32,
    },
    RefreshTick,
}

/// One-shot queries callers can send to read tracker state synchronously.
#[derive(Debug)]
pub enum ProcessTrackerQuery {
    /// Returns a snapshot of the root process (None if already gone).
    GetRoot {
        response: oneshot::Sender<Option<ProcessSnapshot>>,
    },
    /// Returns snapshots of all currently live descendants.
    GetChildren {
        response: oneshot::Sender<Vec<ProcessSnapshot>>,
    },
    /// Returns true when no live descendants remain.
    IsWorkDone { response: oneshot::Sender<bool> },
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessState {
    Running,
    Sleeping,
    Other(String),
    Gone,
}

impl From<ProcessStatus> for ProcessState {
    fn from(status: ProcessStatus) -> Self {
        match status {
            ProcessStatus::Run => ProcessState::Running,
            ProcessStatus::Sleep | ProcessStatus::Idle => ProcessState::Sleeping,
            other => ProcessState::Other(format!("{other:?}")),
        }
    }
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ProcessState::Running => write!(f, "running"),
            ProcessState::Sleeping => write!(f, "sleeping"),
            ProcessState::Other(s) => write!(f, "other({s})"),
            ProcessState::Gone => write!(f, "gone"),
        }
    }
}

/// Lightweight per-process data captured each tick.
#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub state: ProcessState,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
}

pub struct ProcessTrackerChannels {
    pub query_tx: mpsc::Sender<ProcessTrackerQuery>,
    pub query_rx: Option<mpsc::Receiver<ProcessTrackerQuery>>,
    pub event_tx: mpsc::Sender<ProcessTrackerEvent>,
    pub event_rx: Option<mpsc::Receiver<ProcessTrackerEvent>>,
}

impl ProcessTrackerChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (event_tx, event_rx) = mpsc::channel(1024);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    pub fn take_receivers(
        &mut self,
    ) -> Result<(
        mpsc::Receiver<ProcessTrackerQuery>,
        mpsc::Receiver<ProcessTrackerEvent>,
    )> {
        let query_rx = self
            .query_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Query receiver already taken".into()))?;
        let event_rx = self
            .event_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Event receiver already taken".into()))?;
        Ok((query_rx, event_rx))
    }
}

pub struct ProcessTrackerState {
    root_pid: u32,
    prev_child_pids: HashSet<u32>,
    work_done: bool,
    last_root: Option<ProcessSnapshot>,
    last_children: Vec<ProcessSnapshot>,
}

impl ProcessTrackerState {
    pub fn new(root_pid: u32) -> Self {
        Self {
            root_pid,
            prev_child_pids: HashSet::new(),
            work_done: false,
            last_root: None,
            last_children: Vec::new(),
        }
    }
}

pub struct ProcessTracker {
    state: ProcessTrackerState,
    channels: ProcessTrackerChannels,
    sys: System,
    first_tick: bool,
    poll_interval: Duration,
}

impl ProcessTracker {
    pub fn new(pid: u32) -> Self {
        Self {
            state: ProcessTrackerState::new(pid),
            channels: ProcessTrackerChannels::new(),
            sys: System::new(),
            first_tick: true,
            poll_interval: Duration::from_secs(2),
        }
    }

    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    async fn emit_event(&self, event: ProcessTrackerEvent) {
        self.channels
            .event_tx
            .send(event)
            .await
            .expect("event receiver dropped unexpectedly");
    }

    async fn start_tracking_loop(mut self) -> Result<()> {
        let (mut query_rx, mut event_rx) = self
            .channels
            .take_receivers()
            .expect("Failed to take receivers");
        let mut poll_interval = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(event) = event_rx.recv() => {
                    if let Err(err) = self.handle_event(event).await {
                    }
                }
                _ = poll_interval.tick() => {
                    self.emit_event(ProcessTrackerEvent::RefreshTick).await;
                }
            }
        }
    }

    fn handle_query(&self, query: ProcessTrackerQuery) {
        match query {
            ProcessTrackerQuery::GetRoot { response } => {
                let _ = response.send(self.state.last_root.clone());
            }
            ProcessTrackerQuery::GetChildren { response } => {
                let _ = response.send(self.state.last_children.clone());
            }
            ProcessTrackerQuery::IsWorkDone { response } => {
                let _ = response.send(self.state.work_done);
            }
        }
    }

    async fn handle_event(&mut self, event: ProcessTrackerEvent) -> Result<()> {
        match event {
            ProcessTrackerEvent::RefreshTick => {
                self.handle_tick().await;
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns `false` when the tracker should stop.
    async fn handle_tick(&mut self) {
        // ----------------------------------------------------------------
        // Refresh all processes (need parent links to walk subtree).
        // ----------------------------------------------------------------
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        // ----------------------------------------------------------------
        // Check root.
        // ----------------------------------------------------------------
        let root_pid_sysinfo = Pid::from_u32(self.state.root_pid);
        let root_snap = self.sys.process(root_pid_sysinfo).map(|p| ProcessSnapshot {
            pid: self.state.root_pid,
            name: p.name().to_string_lossy().into_owned(),
            state: ProcessState::from(p.status()),
            cpu_usage: p.cpu_usage(),
            memory_bytes: p.memory(),
        });

        if root_snap.is_none() {
            if self.first_tick {
                error!(
                    root_pid = self.state.root_pid,
                    "root process not found on first poll — is the PID correct?"
                );
                // Don't emit RootExited on first tick — the PID might not exist yet
            } else {
                warn!(root_pid = self.state.root_pid, "root process exited");
                self.emit_event(ProcessTrackerEvent::RootExited {
                    pid: self.state.root_pid,
                })
                .await;
            }
            // ✅ Remove the self.state.last_root = None line here;
            //    the unconditional assignment below handles it correctly.
        }

        self.state.last_root = root_snap.clone();

        // ----------------------------------------------------------------
        // Collect full descendant subtree.
        // ----------------------------------------------------------------
        let child_snaps = self.collect_descendants(self.state.root_pid);
        let current_child_pids: HashSet<u32> = child_snaps.iter().map(|s| s.pid).collect();

        // ----------------------------------------------------------------
        // Diff against previous tick.
        // ----------------------------------------------------------------
        let appeared_pids: Vec<u32> = current_child_pids
            .difference(&self.state.prev_child_pids)
            .copied()
            .collect();
        let disappeared_pids: Vec<u32> = self
            .state
            .prev_child_pids
            .difference(&current_child_pids)
            .copied()
            .collect();

        // ----------------------------------------------------------------
        // Emit events.
        // ----------------------------------------------------------------
        if self.first_tick {
            self.emit_event(ProcessTrackerEvent::InitialSnapshot {
                root: root_snap.clone().unwrap(),
                children: child_snaps.clone(),
            })
            .await;
            if child_snaps.is_empty() {
                info!("no child processes found yet — waiting for them to spawn");
            } else {
                info!(
                    count = child_snaps.len(),
                    "discovered initial child processes"
                );
                for child in &child_snaps {
                    info!(pid = child.pid, name = %child.name, state = %child.state, "  └─ child");
                }
            }
        } else {
            // Appeared
            if !appeared_pids.is_empty() {
                let appeared_snaps: Vec<ProcessSnapshot> = appeared_pids
                    .iter()
                    .filter_map(|pid| child_snaps.iter().find(|s| s.pid == *pid).cloned())
                    .collect();
                for s in &appeared_snaps {
                    info!(pid = s.pid, name = %s.name, "child process appeared");
                }
                self.emit_event(ProcessTrackerEvent::ChildrenAppeared(appeared_snaps))
                    .await;
            }

            // Disappeared
            if !disappeared_pids.is_empty() {
                for pid in &disappeared_pids {
                    warn!(pid, "child process exited");
                }
                self.emit_event(ProcessTrackerEvent::ChildrenExited(
                    disappeared_pids.clone(),
                ))
                .await;
            }
        }

        // Root heartbeat debug log
        if let Some(root) = &root_snap {
            debug!(
                pid      = root.pid,
                state    = %root.state,
                cpu      = root.cpu_usage,
                mem      = root.memory_bytes,
                children = child_snaps.len(),
                "root heartbeat"
            );
        }

        // Per-child debug heartbeat
        if !self.first_tick {
            for child in &child_snaps {
                debug!(
                    pid   = child.pid,
                    name  = %child.name,
                    state = %child.state,
                    cpu   = child.cpu_usage,
                    mem   = child.memory_bytes,
                    "child heartbeat"
                );
            }
        }

        // ----------------------------------------------------------------
        // All children gone?
        // ----------------------------------------------------------------
        let all_children_gone = current_child_pids.is_empty();
        if all_children_gone && !self.first_tick {
            info!(
                root_pid = self.state.root_pid,
                "all child processes have exited — work is done"
            );
            self.state.work_done = true;
            self.emit_event(ProcessTrackerEvent::AllChildrenGone).await;
            self.state.last_children = Vec::new();
        }

        self.state.last_children = child_snaps;
        self.state.prev_child_pids = current_child_pids;
        self.first_tick = false;
    }

    fn collect_descendants(&self, root_pid: u32) -> Vec<ProcessSnapshot> {
        let root = Pid::from_u32(root_pid);
        let mut result = Vec::new();
        let mut queue = vec![root];
        while let Some(parent) = queue.pop() {
            for (pid, proc) in self.sys.processes() {
                if proc.parent() == Some(parent) && *pid != root {
                    result.push(ProcessSnapshot {
                        pid: pid.as_u32(),
                        name: proc.name().to_string_lossy().into_owned(),
                        state: ProcessState::from(proc.status()),
                        cpu_usage: proc.cpu_usage(),
                        memory_bytes: proc.memory(),
                    });
                    queue.push(*pid);
                }
            }
        }
        result
    }
}

static PROCESS_TRACKER_QUERY_SENDER: OnceLock<mpsc::Sender<ProcessTrackerQuery>> = OnceLock::new();

pub fn init_process_tracker() {
    let config = get_config();
    if let Some(pid) = config.args.pid {
        let process_tracker = ProcessTracker::new(pid);
        PROCESS_TRACKER_QUERY_SENDER
            .set(process_tracker.channels.query_tx.clone())
            .unwrap();
        tokio::spawn(
            async move { if let Err(err) = process_tracker.start_tracking_loop().await {} },
        );
    }
}

fn get_process_tracker_query_sender() -> &'static mpsc::Sender<ProcessTrackerQuery> {
    PROCESS_TRACKER_QUERY_SENDER
        .get()
        .expect("Process tracker query sender not initialized")
}

/// Get the current root process snapshot.
pub async fn get_root() -> Option<ProcessSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_process_tracker_query_sender()
        .send(ProcessTrackerQuery::GetRoot { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

/// Get snapshots of all currently live child processes.
pub async fn get_children() -> Vec<ProcessSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_process_tracker_query_sender()
        .send(ProcessTrackerQuery::GetChildren { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Returns true when all children have exited (work is considered done).
pub async fn is_work_done() -> bool {
    let (tx, rx) = oneshot::channel();
    let _ = get_process_tracker_query_sender()
        .send(ProcessTrackerQuery::IsWorkDone { response: tx })
        .await;
    rx.await.unwrap_or(true)
}
