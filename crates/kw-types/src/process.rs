use serde::{Deserialize, Serialize};
use std::fmt;

#[cfg(all(feature = "server", target_os = "linux"))]
use procfs::process::Process;


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileDescriptorInfo {
    pub fd: i32,
    pub target: String,
    pub fd_type: FDType,
}

#[cfg(all(feature = "server", target_os = "linux"))]
impl From<procfs::process::FDInfo> for FileDescriptorInfo {
    fn from(fd_info: procfs::process::FDInfo) -> Self {
        Self {
            fd: fd_info.fd,
            target: format!("{:?}", fd_info.target),
            fd_type: fd_info.target.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct IOStats {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_chars: u64,
    pub write_chars: u64,
}

#[cfg(all(feature = "server", target_os = "linux"))]
impl From<procfs::process::Io> for IOStats {
    fn from(io: procfs::process::Io) -> Self {
        Self {
            read_bytes: io.read_bytes,
            write_bytes: io.write_bytes,
            read_chars: io.rchar,
            write_chars: io.wchar,
        }
    }
}

/// Lightweight per-process data captured each tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub state: ProcessState,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_usage: u64,

    /// Linux-only. `None`/empty when the reporting host isn't Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmdline: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_files: Vec<FileDescriptorInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_stats: Option<IOStats>,
}

impl fmt::Display for ProcessSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[{}] {}  state={} cpu={:.1}% mem={} disk={}",
            self.pid,
            self.name,
            self.state,
            self.cpu_usage,
            kw_utils::format_bytes(self.memory_bytes),
            kw_utils::format_bytes(self.disk_usage),
        )
    }
}

#[cfg(feature = "server")]
impl From<&sysinfo::Process> for ProcessSnapshot {
    fn from(process: &sysinfo::Process) -> Self {
        let pid = process.pid().as_u32();
        let ext = collect_extended_process_info(pid);
        Self {
            pid,
            name: process.name().to_string_lossy().into_owned(),
            state: ProcessState::from(process.status()),
            cpu_usage: process.cpu_usage(),
            memory_bytes: process.memory(),
            disk_usage: disk_usage_total(process.disk_usage()),
            cwd: ext.cwd,
            cmdline: ext.cmdline,
            open_files: ext.open_files,
            io_stats: ext.io_stats,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTree {
    pub root_pid: u32,
    pub root: Option<ProcessSnapshot>,
    pub children: Vec<ProcessSnapshot>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}

impl fmt::Display for ProcessTree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "root_pid: {}  children: {}  work_done: {}",
            self.root_pid, self.child_count, self.work_done
        )?;
        match &self.root {
            Some(p) => writeln!(f, "  root: {p}")?,
            None => writeln!(f, "  root: (exited)")?,
        }
        for child in &self.children {
            writeln!(f, "    {child}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProcessStatus {
    pub root_alive: bool,
    pub root_pid: Option<u32>,
    pub root_name: Option<String>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}

impl fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = self.root_name.as_deref().unwrap_or("?");
        let pid = self
            .root_pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".into());
        write!(
            f,
            "[{pid}] {name}  alive={} children={} work_done={}",
            self.root_alive, self.child_count, self.work_done
        )
    }
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSignal {
    Kill,
    #[serde(rename = "int")]
    Interrupt,
    Stop,
    #[serde(rename = "cont")]
    Continue,
    Term,
}

impl fmt::Display for ProcessSignal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Kill => write!(f, "kill"),
            Self::Interrupt => write!(f, "int"),
            Self::Stop => write!(f, "stop"),
            Self::Continue => write!(f, "cont"),
            Self::Term => write!(f, "term"),
        }
    }
}

impl TryFrom<&str> for ProcessSignal {
    type Error = String;

    fn try_from(signal: &str) -> Result<Self, Self::Error> {
        match signal {
            "kill" => Ok(Self::Kill),
            "int" => Ok(Self::Interrupt),
            "stop" => Ok(Self::Stop),
            "cont" => Ok(Self::Continue),
            "term" => Ok(Self::Term),
            _ => Err(format!("Invalid signal: '{signal}'.")),
        }
    }
}

impl ProcessSignal {
    /// Returns `None` on Windows for any signal other than Kill,
    /// since only forceful termination is supported there.
    #[cfg(feature = "server")]
    pub fn sysinfo_signal(&self) -> Option<sysinfo::Signal> {
        #[cfg(windows)]
        {
            // Windows only supports Kill; everything else is a no-op.
            match self {
                ProcessSignal::Kill => Some(sysinfo::Signal::Kill),
                _ => None,
            }
        }

        #[cfg(not(windows))]
        {
            Some(match self {
                ProcessSignal::Kill => sysinfo::Signal::Kill,
                ProcessSignal::Interrupt => sysinfo::Signal::Interrupt,
                ProcessSignal::Stop => sysinfo::Signal::Stop,
                ProcessSignal::Continue => sysinfo::Signal::Continue,
                ProcessSignal::Term => sysinfo::Signal::Term,
            })
        }
    }

    /// True if this signal is deliverable on the current platform.
    pub fn is_supported(&self) -> bool {
        #[cfg(windows)]
        {
            // Windows only supports Kill; everything else is a no-op.
            matches!(self, ProcessSignal::Kill)
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    /// Returns a list of supported signal based on current platform.
    pub fn get_supported_signals() -> Vec<ProcessSignal> {
        #[cfg(windows)]
        {
            // Windows only supports Kill; everything else is a no-op.
            vec![Self::Kill]
        }
        #[cfg(not(windows))]
        {
            vec![
                Self::Kill,
                Self::Interrupt,
                Self::Stop,
                Self::Continue,
                Self::Term,
            ]
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum ProcessState {
    Running,
    Sleeping,
    Other(String),
    Gone,
}

#[cfg(feature = "server")]
impl From<sysinfo::ProcessStatus> for ProcessState {
    fn from(status: sysinfo::ProcessStatus) -> Self {
        use sysinfo::ProcessStatus;

        match status {
            ProcessStatus::Run => ProcessState::Running,
            ProcessStatus::Sleep | ProcessStatus::Idle => ProcessState::Sleeping,
            other => ProcessState::Other(format!("{other:?}")),
        }
    }
}

impl fmt::Display for ProcessState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ProcessState::Running => write!(f, "running"),
            ProcessState::Sleeping => write!(f, "sleeping"),
            ProcessState::Other(s) => write!(f, "other({s})"),
            ProcessState::Gone => write!(f, "gone"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FDType {
    File,
    Socket,
    Pipe,
    Other,
}

impl fmt::Display for FDType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FDType::File => "file",
            FDType::Socket => "socket",
            FDType::Pipe => "pipe",
            FDType::Other => "other",
        };
        write!(f, "{s}")
    }
}

#[cfg(all(feature = "server", target_os = "linux"))]
impl From<procfs::process::FDTarget> for FDType {
    fn from(fd_target: procfs::process::FDTarget) -> Self {
        use procfs::process::FDTarget;
        match fd_target {
            FDTarget::Path(_) => Self::File,
            FDTarget::Socket(_) => Self::Socket,
            FDTarget::Pipe(_) => Self::Pipe,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessesSortKey {
    Memory,
    Cpu,
    Disk,
}

impl fmt::Display for ProcessesSortKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Memory => write!(f, "Memory"),
            Self::Cpu => write!(f, "Cpu"),
            Self::Disk => write!(f, "Disk"),
        }
    }
}

impl TryFrom<&str> for ProcessesSortKey {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "memory" => Ok(Self::Memory),
            "cpu" => Ok(Self::Cpu),
            "disk" => Ok(Self::Disk),
            other => Err(format!(
                "invalid sort key '{other}', expected: cpu, memory, disk"
            )),
        }
    }
}

// Linux-only helper functions
#[cfg(all(feature = "server", target_os = "linux"))]
pub fn collect_file_descriptors(pid: u32) -> Vec<super::process::FileDescriptorInfo> {
    if let Ok(process) = Process::new(pid as i32)
        && let Ok(fd_iter) = process.fd()
    {
        fd_iter.flatten().map(|fd_info| fd_info.into()).collect()
    } else {
        vec![]
    }
}

#[cfg(all(feature = "server", target_os = "linux"))]
pub fn collect_io_stats(pid: u32) -> Option<super::process::IOStats> {
    Process::new(pid as i32)
        .ok()
        .and_then(|p| p.io().ok())
        .map(Into::into)
}

#[cfg(all(feature = "server", target_os = "linux"))]
pub fn collect_extended_info(pid: u32) -> (Option<String>, Vec<String>) {
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

#[cfg(feature = "server")]
pub fn disk_usage_total(disk_usage: sysinfo::DiskUsage) -> u64 {
    disk_usage.written_bytes + disk_usage.read_bytes
}

#[cfg(feature = "server")]
struct ExtendedProcessInfo {
    cwd: Option<String>,
    cmdline: Vec<String>,
    open_files: Vec<FileDescriptorInfo>,
    io_stats: Option<IOStats>,
}

#[cfg(all(feature = "server", target_os = "linux"))]
fn collect_extended_process_info(pid: u32) -> ExtendedProcessInfo {
    let (cwd, cmdline) = collect_extended_info(pid);
    ExtendedProcessInfo {
        cwd,
        cmdline,
        open_files: collect_file_descriptors(pid),
        io_stats: collect_io_stats(pid),
    }
}

#[cfg(all(feature = "server", not(target_os = "linux")))]
fn collect_extended_process_info(_pid: u32) -> ExtendedProcessInfo {
    ExtendedProcessInfo {
        cwd: None,
        cmdline: Vec::new(),
        open_files: Vec::new(),
        io_stats: None,
    }
}