#[cfg(target_os = "linux")]
use procfs::process::{FDTarget, Process};

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::Duration,
};

use super::{enums::*, structs::*};
use crate::prelude::*;

// Linux-only helper functions
#[cfg(target_os = "linux")]
fn collect_file_descriptors(pid: u32) -> Vec<FileDescriptorInfo> {
    let mut fds = Vec::new();
    if let Ok(process) = Process::new(pid as i32) {
        if let Ok(fd_iter) = process.fd() {
            for fd_info in fd_iter.flatten() {
                let fd_type = match &fd_info.target {
                    FDTarget::Path(_) => FDType::File,
                    FDTarget::Socket(_) => FDType::Socket,
                    FDTarget::Pipe(_) => FDType::Pipe,
                    _ => FDType::Other,
                };
                fds.push(FileDescriptorInfo {
                    fd: fd_info.fd,
                    target: format!("{:?}", fd_info.target),
                    fd_type,
                });
            }
        }
    }
    fds
}

#[cfg(target_os = "linux")]
fn collect_io_stats(pid: u32) -> Option<IOStats> {
    Process::new(pid as i32)
        .ok()
        .and_then(|p| p.io().ok())
        .map(|io| IOStats {
            read_bytes: io.read_bytes,
            write_bytes: io.write_bytes,
            read_chars: io.rchar,
            write_chars: io.wchar,
        })
}

#[cfg(target_os = "linux")]
fn collect_extended_info(pid: u32) -> (Option<String>, Vec<String>) {
    let process = Process::new(pid as i32).ok();
    let cwd = process
        .as_ref()
        .and_then(|p| p.cwd().ok())
        .map(|path| path.to_string_lossy().into_owned());
    let cmdline = process
        .as_ref()
        .and_then(|p| p.cmdline().ok())
        .unwrap_or_default();
    (cwd, cmdline)
}

impl ProcessTrackerChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        // capacity 64: events are cheap and subscribers should keep up
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            event_tx,
        }
    }

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<ProcessTrackerQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::ProcessTracker("Query receiver already taken".into()))
    }
}

pub struct ProcessTrackerState {
    root_pid: u32,
    prev_child_pids: HashSet<u32>,
    work_done: bool,
    children_ever_seen: bool,
    last_root: Option<ProcessSnapshot>,
    last_children: Vec<ProcessSnapshot>,
    last_top_by_memory: Vec<ProcessSnapshot>,
    last_top_by_cpu: Vec<ProcessSnapshot>,
}

impl ProcessTrackerState {
    pub fn new(root_pid: u32) -> Self {
        Self {
            root_pid,
            prev_child_pids: HashSet::new(),
            work_done: false,
            children_ever_seen: false,
            last_root: None,
            last_children: Vec::new(),
            last_top_by_memory: Vec::new(),
            last_top_by_cpu: Vec::new(),
        }
    }
}

pub struct ProcessTracker {
    state: ProcessTrackerState,
    channels: ProcessTrackerChannels,
    sys: System,
    first_tick: bool,
    poll_interval: Duration,
    poll_interval_timer: Option<tokio::time::Interval>,
    track_top_processes: bool,
    limit_processes: usize,
}

impl ProcessTracker {
    pub fn new(pid: u32) -> Self {
        let config = get_config();
        Self {
            state: ProcessTrackerState::new(pid),
            channels: ProcessTrackerChannels::new(),
            sys: System::new(),
            first_tick: true,
            poll_interval: Duration::from_secs(2),
            poll_interval_timer: None,
            track_top_processes: config.args.top_processes,
            limit_processes: config.args.limit_processes,
        }
    }

    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    fn emit_event(&self, event: ProcessTrackerEvent) {
        // Err means no subscribers are listening right now — that's fine.
        let _ = self.channels.event_tx.send(event);
    }

    async fn start_tracking_loop(mut self) -> Result<()> {
        let mut query_rx = self
            .channels
            .take_query_rx()
            .expect("Failed to take query receiver");
        self.poll_interval_timer = Some(tokio::time::interval(self.poll_interval));
        loop {
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                _ = async { self.poll_interval_timer.as_mut().unwrap().tick().await }, if self.poll_interval_timer.is_some() => {
                    self.handle_tick().await;
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
            ProcessTrackerQuery::GetTopProcesses {
                by,
                limit,
                response,
            } => {
                let limit = if limit == 0 || limit > self.limit_processes {
                    self.limit_processes
                } else {
                    limit
                };
                let result = match by {
                    SortKey::Memory => self
                        .state
                        .last_top_by_memory
                        .iter()
                        .take(limit)
                        .cloned()
                        .collect(),
                    SortKey::Cpu => self
                        .state
                        .last_top_by_cpu
                        .iter()
                        .take(limit)
                        .cloned()
                        .collect(),
                };
                let _ = response.send(result);
            }
        }
    }

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
        #[cfg(target_os = "linux")]
        let (cwd, cmdline) = collect_extended_info(self.state.root_pid);
        let root_snap = self.sys.process(root_pid_sysinfo).map(|p| ProcessSnapshot {
            pid: self.state.root_pid,
            name: p.name().to_string_lossy().into_owned(),
            state: ProcessState::from(p.status()),
            cpu_usage: p.cpu_usage(),
            memory_bytes: p.memory(),
            #[cfg(target_os = "linux")]
            cwd,
            #[cfg(target_os = "linux")]
            cmdline,
            #[cfg(target_os = "linux")]
            open_files: collect_file_descriptors(self.state.root_pid),
            #[cfg(target_os = "linux")]
            io_stats: collect_io_stats(self.state.root_pid),
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
                // Mark the last known root snapshot as Gone before clearing it.
                if let Some(ref mut snap) = self.state.last_root {
                    snap.state = ProcessState::Gone;
                }
                self.emit_event(ProcessTrackerEvent::RootExited {
                    pid: self.state.root_pid,
                });
                self.poll_interval_timer = None;
            }
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
            });
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
                self.emit_event(ProcessTrackerEvent::ChildrenAppeared(appeared_snaps));
            }

            // Disappeared
            if !disappeared_pids.is_empty() {
                for pid in &disappeared_pids {
                    warn!(pid, "child process exited");
                }
                self.emit_event(ProcessTrackerEvent::ChildrenExited(
                    disappeared_pids.clone(),
                ));
            }
        }

        // ----------------------------------------------------------------
        // Track whether we've ever seen children.
        // ----------------------------------------------------------------
        if !current_child_pids.is_empty() {
            self.state.children_ever_seen = true;
        }

        // ----------------------------------------------------------------
        // All children gone? Only fire on the transition, and only after
        // we've seen at least one child.
        // ----------------------------------------------------------------
        let was_non_empty = !self.state.prev_child_pids.is_empty();
        let now_empty = current_child_pids.is_empty();

        if self.state.children_ever_seen && was_non_empty && now_empty {
            info!(
                root_pid = self.state.root_pid,
                "all child processes have exited — work is done"
            );
            self.state.work_done = true;
            self.emit_event(ProcessTrackerEvent::AllChildrenGone);
        }

        self.state.last_children = child_snaps;
        self.state.prev_child_pids = current_child_pids;
        if self.track_top_processes {
            self.set_top_processes();
        }
        self.first_tick = false;
    }

    fn set_top_processes(&mut self) {
        let mut all: Vec<(u32, f32, u64)> = self
            .sys
            .processes()
            .values()
            .map(|p| (p.pid().as_u32(), p.cpu_usage(), p.memory()))
            .collect();
        let mut cache: HashMap<u32, ProcessSnapshot> = HashMap::new();
        let mut get_or_create =
            |pid: u32, cpu_usage: f32, memory_bytes: u64| -> Option<ProcessSnapshot> {
                if let Some(cached) = cache.get(&pid) {
                    return Some(cached.clone());
                }
                self.sys.process(Pid::from_u32(pid)).map(|p| {
                    #[cfg(target_os = "linux")]
                    let (cwd, cmdline) = collect_extended_info(pid);
                    let process = ProcessSnapshot {
                        pid,
                        name: p.name().to_string_lossy().into_owned(),
                        state: ProcessState::from(p.status()),
                        cpu_usage,
                        memory_bytes,
                        #[cfg(target_os = "linux")]
                        cwd,
                        #[cfg(target_os = "linux")]
                        cmdline,
                        #[cfg(target_os = "linux")]
                        open_files: collect_file_descriptors(pid),
                        #[cfg(target_os = "linux")]
                        io_stats: collect_io_stats(pid),
                    };
                    cache.insert(pid, process.clone());
                    process
                })
            };
        all.sort_unstable_by(|a, b| b.2.cmp(&a.2));
        self.state.last_top_by_memory = all
            .iter()
            .take(self.limit_processes)
            .filter_map(|&(pid, cpu_usage, memory_bytes)| {
                get_or_create(pid, cpu_usage, memory_bytes)
            })
            .collect();
        all.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.state.last_top_by_cpu = all
            .iter()
            .take(self.limit_processes)
            .filter_map(|&(pid, cpu_usage, memory_bytes)| {
                get_or_create(pid, cpu_usage, memory_bytes)
            })
            .collect();
    }

    fn collect_descendants(&self, root_pid: u32) -> Vec<ProcessSnapshot> {
        let root = Pid::from_u32(root_pid);
        let mut result = Vec::new();
        let mut queue = vec![root];
        while let Some(parent) = queue.pop() {
            for (pid, proc) in self.sys.processes() {
                if proc.parent() == Some(parent) && *pid != root {
                    let pid_u32 = pid.as_u32();

                    // Basic snapshot that works on all platforms
                    #[cfg(not(target_os = "linux"))]
                    {
                        result.push(ProcessSnapshot {
                            pid: pid_u32,
                            name: proc.name().to_string_lossy().into_owned(),
                            state: ProcessState::from(proc.status()),
                            cpu_usage: proc.cpu_usage(),
                            memory_bytes: proc.memory(),
                        });
                    }

                    // Extended snapshot for Linux
                    #[cfg(target_os = "linux")]
                    {
                        let (cwd, cmdline) = collect_extended_info(pid_u32);

                        result.push(ProcessSnapshot {
                            pid: pid_u32,
                            name: proc.name().to_string_lossy().into_owned(),
                            state: ProcessState::from(proc.status()),
                            cpu_usage: proc.cpu_usage(),
                            memory_bytes: proc.memory(),
                            cwd,
                            cmdline,
                            open_files: collect_file_descriptors(pid_u32),
                            io_stats: collect_io_stats(pid_u32),
                        });
                    }

                    queue.push(*pid);
                }
            }
        }
        result
    }
}

static PROCESS_TRACKER_QUERY_SENDER: OnceLock<mpsc::Sender<ProcessTrackerQuery>> = OnceLock::new();
static PROCESS_TRACKER_EVENT_SENDER: OnceLock<broadcast::Sender<ProcessTrackerEvent>> =
    OnceLock::new();

pub fn init_process_tracker() {
    let Some(pid) = get_config().args.pid else {
        return;
    };
    let process_tracker = ProcessTracker::new(pid);
    PROCESS_TRACKER_QUERY_SENDER
        .set(process_tracker.channels.query_tx.clone())
        .unwrap();
    PROCESS_TRACKER_EVENT_SENDER
        .set(process_tracker.channels.event_tx.clone())
        .unwrap();
    tokio::spawn(async move {
        if let Err(e) = process_tracker.start_tracking_loop().await {
            error!(?e, "process tracker loop exited with error");
        }
    });
    info!("Process Tracker started with PID: {pid}");
}

/// Subscribe to tracker events (e.g. from a Telegram bot or WebSocket handler).
/// Returns `None` if the tracker was not started (no `--pid` given).
pub fn subscribe_events() -> Option<broadcast::Receiver<ProcessTrackerEvent>> {
    PROCESS_TRACKER_EVENT_SENDER.get().map(|tx| tx.subscribe())
}

fn get_process_tracker_query_sender() -> Option<&'static mpsc::Sender<ProcessTrackerQuery>> {
    PROCESS_TRACKER_QUERY_SENDER.get()
}

/// Get the current root process snapshot.
pub async fn get_root() -> Option<ProcessSnapshot> {
    let tx_ref = get_process_tracker_query_sender()?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetRoot { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

/// Get snapshots of all currently live child processes.
pub async fn get_children() -> Vec<ProcessSnapshot> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetChildren { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Returns true when all children have exited (work is considered done).
pub async fn is_work_done() -> bool {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return true; // no tracker = no work to wait for
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::IsWorkDone { response: tx })
        .await;
    rx.await.unwrap_or(true)
}

/// Get the top N processes sorted by the given key.
/// Returns an empty vec if the tracker was not started.
pub async fn get_top_processes(by: SortKey, limit: usize) -> Vec<ProcessSnapshot> {
    let Some(tx_ref) = get_process_tracker_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ProcessTrackerQuery::GetTopProcesses {
            by,
            limit,
            response: tx,
        })
        .await;
    rx.await.unwrap_or_default()
}
