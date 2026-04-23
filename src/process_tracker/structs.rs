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
