use serde::Serialize;
use tokio::sync::{broadcast, mpsc};

use super::enums::*;

// Linux-only structures
#[cfg(target_os = "linux")]
#[derive(Debug, Serialize, Clone)]
pub struct FileDescriptorInfo {
    pub fd: i32,
    pub target: String,
    pub fd_type: FDType,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Serialize, Clone, Copy)]
pub struct IOStats {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_chars: u64,
    pub write_chars: u64,
}

/// Lightweight per-process data captured each tick.
#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub state: ProcessState,
    pub cpu_usage: f32,
    pub memory_bytes: u64,

    // Optional fields only available on Linux
    #[cfg(target_os = "linux")]
    pub cwd: Option<String>,
    #[cfg(target_os = "linux")]
    pub cmdline: Vec<String>,
    #[cfg(target_os = "linux")]
    pub open_files: Vec<FileDescriptorInfo>,
    #[cfg(target_os = "linux")]
    pub io_stats: Option<IOStats>,
}

impl From<&sysinfo::Process> for ProcessSnapshot {
    fn from(process: &sysinfo::Process) -> Self {
        let pid = process.pid().as_u32();
        #[cfg(target_os = "linux")]
        let (cwd, cmdline) = super::utils::collect_extended_info(pid);
        Self {
            pid,
            name: process.name().to_string_lossy().into_owned(),
            state: ProcessState::from(process.status()),
            cpu_usage: process.cpu_usage(),
            memory_bytes: process.memory(),
            #[cfg(target_os = "linux")]
            cwd,
            #[cfg(target_os = "linux")]
            cmdline,
            #[cfg(target_os = "linux")]
            open_files: super::utils::collect_file_descriptors(pid),
            #[cfg(target_os = "linux")]
            io_stats: super::utils::collect_io_stats(pid),
        }
    }
}

pub struct ProcessTrackerChannels {
    pub query_tx: mpsc::Sender<ProcessTrackerQuery>,
    pub query_rx: Option<mpsc::Receiver<ProcessTrackerQuery>>,
    pub event_tx: broadcast::Sender<ProcessTrackerEvent>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub state: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    /// Human-readable memory string, e.g. "42.3 MB".
    pub memory_human: String,

    // Linux-only fields — omitted entirely on other platforms.
    #[cfg(target_os = "linux")]
    pub cwd: Option<String>,
    #[cfg(target_os = "linux")]
    pub cmdline: Vec<String>,
    /// Number of open file descriptors.
    #[cfg(target_os = "linux")]
    pub open_fds: usize,
    /// Details for each open file descriptor.
    #[cfg(target_os = "linux")]
    pub open_files: Vec<FileDescriptorInfo>,
    #[cfg(target_os = "linux")]
    pub io_stats: Option<IOStats>,
}

impl From<&ProcessSnapshot> for ProcessInfo {
    fn from(s: &ProcessSnapshot) -> Self {
        Self {
            memory_human: super::utils::format_memory(s.memory_bytes),
            pid: s.pid,
            name: s.name.clone(),
            state: s.state.to_string(),
            cpu_usage: s.cpu_usage,
            memory_bytes: s.memory_bytes,
            #[cfg(target_os = "linux")]
            cwd: s.cwd.clone(),
            #[cfg(target_os = "linux")]
            cmdline: s.cmdline.clone(),
            #[cfg(target_os = "linux")]
            open_fds: s.open_files.len(),
            #[cfg(target_os = "linux")]
            open_files: s.open_files.clone(),
            #[cfg(target_os = "linux")]
            io_stats: s.io_stats.map(|io| super::structs::IOStats {
                read_bytes: io.read_bytes,
                write_bytes: io.write_bytes,
                read_chars: io.read_chars,
                write_chars: io.write_chars,
            }),
        }
    }
}

impl From<ProcessSnapshot> for ProcessInfo {
    fn from(s: ProcessSnapshot) -> Self {
        Self::from(&s)
    }
}

#[derive(Debug, Serialize)]
pub struct ProcessTree {
    pub root: Option<ProcessInfo>,
    pub children: Vec<ProcessInfo>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessStatus {
    pub root_alive: bool,
    pub root_pid: Option<u32>,
    pub root_name: Option<String>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}
