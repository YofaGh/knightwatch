use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::{
    sync::{broadcast, mpsc},
    time::Duration,
};

use super::{enums::*, structs::*};
use crate::prelude::*;

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

struct ProcessTrackerState {
    root_processes: HashMap<u32, RootProcess>,
    last_top_by_memory: Vec<ProcessSnapshot>,
    last_top_by_cpu: Vec<ProcessSnapshot>,
}

impl ProcessTrackerState {
    pub fn new(root_pids: Vec<u32>) -> Self {
        Self {
            root_processes: root_pids
                .into_iter()
                .map(|pid| (pid, RootProcess::new(pid)))
                .collect(),
            last_top_by_memory: Vec::new(),
            last_top_by_cpu: Vec::new(),
        }
    }
}

struct ProcessTracker {
    state: ProcessTrackerState,
    channels: ProcessTrackerChannels,
    sys: System,
    poll_interval: Duration,
    poll_interval_timer: Option<tokio::time::Interval>,
    track_top_processes: bool,
    limit_processes: usize,
}

impl ProcessTracker {
    pub fn new(pids: Vec<u32>) -> Self {
        let config = get_config();
        Self {
            state: ProcessTrackerState::new(pids),
            channels: ProcessTrackerChannels::new(),
            sys: System::new(),
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

    fn get_root_process_mut(&mut self, root_pid: &u32) -> Result<&mut RootProcess> {
        self.state
            .root_processes
            .get_mut(root_pid)
            .ok_or(Error::ProcessTracker(format!(
                "Not Tracking a process with pid {root_pid}"
            )))
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
            ProcessTrackerQuery::GetRoot { root_pid, response } => {
                let _ = response.send(
                    self.state
                        .root_processes
                        .get(&root_pid)
                        .and_then(|rp| rp.last_root.clone()),
                );
            }
            ProcessTrackerQuery::GetChildren { root_pid, response } => {
                let _ = response.send(
                    self.state
                        .root_processes
                        .get(&root_pid)
                        .map(|process| process.last_children.clone())
                        .unwrap_or_default(),
                );
            }
            ProcessTrackerQuery::IsWorkDone { root_pid, response } => {
                let _ = response.send(
                    self.state
                        .root_processes
                        .get(&root_pid)
                        .map(|process| process.work_done)
                        .unwrap_or_default(),
                );
            }
            ProcessTrackerQuery::GetTrackedPids { response } => {
                let _ = response.send(self.state.root_processes.keys().cloned().collect());
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
        let pids: Vec<u32> = self.state.root_processes.keys().cloned().collect();
        for pid in pids {
            let _ = self.update_root_pid_state(pid);
        }
        if self.track_top_processes {
            self.set_top_processes();
        }
    }

    fn update_root_pid_state(&mut self, root_pid: u32) -> Result<()> {
        // ----------------------------------------------------------------
        // Check root.
        // ----------------------------------------------------------------
        if self
            .state
            .root_processes
            .get(&root_pid)
            .is_none_or(|r| r.work_done)
        {
            return Ok(());
        }
        let root_snap = self.sys.process(Pid::from_u32(root_pid)).map(Into::into);

        if root_snap.is_some() {
            self.get_root_process_mut(&root_pid)?.root_appeared = true;
        } else {
            self.emit_event(ProcessTrackerEvent::RootExited { pid: root_pid });
            let root_process = self.get_root_process_mut(&root_pid)?;
            root_process.root_exited = true;
            if root_process.first_tick {
                error!(
                    root_pid,
                    "root process not found on first tick — bad PID or extremely short-lived process"
                );
            } else {
                warn!(root_pid, "root process exited");
            }
            if let Some(ref mut snap) = root_process.last_root {
                snap.state = ProcessState::Gone;
            }
        }

        // ----------------------------------------------------------------
        // Collect full descendant subtree.
        // ----------------------------------------------------------------
        let child_snaps = self.collect_descendants(root_pid);
        let current_child_pids: HashSet<u32> = child_snaps.iter().map(|s| s.pid).collect();

        let root_process = self.get_root_process_mut(&root_pid)?;

        root_process.last_root = root_snap.clone();
        // ----------------------------------------------------------------
        // Diff against previous tick.
        // ----------------------------------------------------------------
        let appeared_pids: Vec<u32> = current_child_pids
            .difference(&root_process.prev_child_pids)
            .copied()
            .collect();
        let disappeared_pids: Vec<u32> = root_process
            .prev_child_pids
            .difference(&current_child_pids)
            .copied()
            .collect();

        // ----------------------------------------------------------------
        // Emit events.
        // ----------------------------------------------------------------
        if root_process.first_tick {
            self.emit_event(ProcessTrackerEvent::InitialSnapshot {
                root: root_snap.clone().unwrap(),
                children: child_snaps.clone(),
            });
            if child_snaps.is_empty() {
                info!(
                    root_pid,
                    "no child processes found yet — waiting for them to spawn"
                );
            } else {
                info!(
                    root_pid,
                    count = child_snaps.len(),
                    "discovered initial child processes"
                );
                for child in &child_snaps {
                    info!(root_pid, pid = child.pid, name = %child.name, state = %child.state, "  └─ child");
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
                    info!(root_pid, pid = s.pid, name = %s.name, "child process appeared");
                }
                self.emit_event(ProcessTrackerEvent::ChildrenAppeared {
                    pid: root_pid,
                    children: appeared_snaps,
                });
            }

            // Disappeared
            if !disappeared_pids.is_empty() {
                for pid in &disappeared_pids {
                    warn!(root_pid, pid, "child process exited");
                }
                self.emit_event(ProcessTrackerEvent::ChildrenExited {
                    pid: root_pid,
                    children: disappeared_pids,
                });
            }
        }

        let root_process = self.get_root_process_mut(&root_pid)?;

        // ----------------------------------------------------------------
        // Track whether we've ever seen children.
        // ----------------------------------------------------------------
        if !current_child_pids.is_empty() {
            root_process.children_ever_seen = true;
        }

        // ----------------------------------------------------------------
        // All children gone? Only fire on the transition, and only after
        // we've seen at least one child.
        // ----------------------------------------------------------------
        let all_children_gone = root_process.children_ever_seen
            && !root_process.prev_child_pids.is_empty()
            && current_child_pids.is_empty();
        let work_done =
            root_process.root_exited && root_process.root_appeared && current_child_pids.is_empty();

        if all_children_gone {
            info!(root_pid, "all child processes have exited");
            self.emit_event(ProcessTrackerEvent::AllChildrenGone { pid: root_pid });
        }
        if work_done {
            info!(root_pid, "work is done");
            self.get_root_process_mut(&root_pid)?.work_done = true;
            self.emit_event(ProcessTrackerEvent::WorkComplete { pid: root_pid });
        }

        let root_process = self.get_root_process_mut(&root_pid)?;
        root_process.last_children = child_snaps;
        root_process.prev_child_pids = current_child_pids;
        root_process.first_tick = false;
        Ok(())
    }

    fn set_top_processes(&mut self) {
        let mut all: Vec<(u32, f32, u64)> = self
            .sys
            .processes()
            .values()
            .map(|p| (p.pid().as_u32(), p.cpu_usage(), p.memory()))
            .collect();
        let mut cache: HashMap<u32, ProcessSnapshot> = HashMap::new();
        let mut get_or_create = |pid: u32| -> Option<ProcessSnapshot> {
            if let Some(cached) = cache.get(&pid) {
                return Some(cached.clone());
            }
            self.sys.process(Pid::from_u32(pid)).map(|p| {
                let process = ProcessSnapshot::from(p);
                cache.insert(pid, process.clone());
                process
            })
        };
        all.sort_unstable_by(|a, b| b.2.cmp(&a.2));
        self.state.last_top_by_memory = all
            .iter()
            .take(self.limit_processes)
            .filter_map(|&(pid, _, _)| get_or_create(pid))
            .collect();
        all.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.state.last_top_by_cpu = all
            .iter()
            .take(self.limit_processes)
            .filter_map(|&(pid, _, _)| get_or_create(pid))
            .collect();
    }

    fn collect_descendants(&self, root_pid: u32) -> Vec<ProcessSnapshot> {
        let root = Pid::from_u32(root_pid);
        let mut children_map: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, proc) in self.sys.processes() {
            if let Some(parent) = proc.parent() {
                children_map.entry(parent).or_default().push(*pid);
            }
        }
        let mut result = Vec::new();
        let mut queue = vec![root];
        while let Some(parent) = queue.pop() {
            if let Some(children) = children_map.get(&parent) {
                for pid in children {
                    if let Some(proc) = self.sys.process(*pid) {
                        result.push(ProcessSnapshot::from(proc));
                        queue.push(*pid);
                    }
                }
            }
        }
        result
    }
}

pub static PROCESS_TRACKER_QUERY_SENDER: OnceLock<mpsc::Sender<ProcessTrackerQuery>> =
    OnceLock::new();
pub static PROCESS_TRACKER_EVENT_SENDER: OnceLock<broadcast::Sender<ProcessTrackerEvent>> =
    OnceLock::new();

pub fn init_process_tracker() {
    let config = get_config();
    if config.args.pid.is_empty() && !config.args.top_processes {
        return;
    }
    let process_tracker = ProcessTracker::new(config.args.pid.clone());
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
    info!("Process Tracker started");
    let pids: Vec<String> = config.args.pid.iter().map(|p| p.to_string()).collect();
    if !pids.is_empty() {
        info!(
            pids = pids.join(", "),
            "Tracking process trees rooted at PID(s)"
        );
    }
    if config.args.top_processes {
        info!(
            "Tracking top processes (limit {})",
            config.args.limit_processes
        );
    }
}
