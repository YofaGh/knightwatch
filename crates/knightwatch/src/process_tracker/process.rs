use std::collections::HashSet;

use kw_types::process::ProcessSnapshot;

use crate::utils::now_rfc3339;

pub struct RootProcess {
    pub root_pid: u32,
    pub first_tick: bool,
    pub root_appeared: bool,
    pub prev_child_pids: HashSet<u32>,
    pub work_done: bool,
    pub root_exited: bool,
    pub children_ever_seen: bool,
    pub last_root: Option<ProcessSnapshot>,
    pub last_children: Vec<ProcessSnapshot>,
}

impl RootProcess {
    pub fn new(root_pid: u32) -> Self {
        Self {
            root_pid,
            first_tick: true,
            root_appeared: false,
            prev_child_pids: HashSet::new(),
            work_done: false,
            root_exited: false,
            children_ever_seen: false,
            last_root: None,
            last_children: Vec::new(),
        }
    }
}

impl From<&RootProcess> for kw_types::process::ProcessTree {
    fn from(root_process: &RootProcess) -> Self {
        Self {
            root_pid: root_process.root_pid,
            root: root_process.last_root.clone(),
            children: root_process.last_children.clone(),
            child_count: root_process.last_children.len(),
            work_done: root_process.work_done,
            timestamp: now_rfc3339(),
        }
    }
}

impl From<&RootProcess> for kw_types::process::ProcessStatus {
    fn from(root_process: &RootProcess) -> Self {
        Self {
            root_alive: root_process.last_root.is_some(),
            root_pid: root_process.last_root.as_ref().map(|p| p.pid),
            root_name: root_process.last_root.as_ref().map(|p| p.name.clone()),
            child_count: root_process.last_children.len(),
            work_done: root_process.work_done,
            timestamp: now_rfc3339(),
        }
    }
}
