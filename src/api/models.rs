use serde::Serialize;

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub uptime: String,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotImage {
    pub data: String,
    pub mime: String,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    pub screens: Vec<ScreenshotImage>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Process tracking
// ---------------------------------------------------------------------------

/// I/O counters sourced from `/proc/<pid>/io` (Linux only).
#[cfg(target_os = "linux")]
#[derive(Debug, Serialize, Clone)]
pub struct IOStatsResponse {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_chars: u64,
    pub write_chars: u64,
}

/// Serialisable mirror of `ProcessSnapshot`.
#[derive(Debug, Serialize, Clone)]
pub struct ProcessSnapshotResponse {
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
    #[cfg(target_os = "linux")]
    pub io_stats: Option<IOStatsResponse>,
}

/// Response for `GET /process` — full tree.
#[derive(Debug, Serialize)]
pub struct ProcessTreeResponse {
    pub root: Option<ProcessSnapshotResponse>,
    pub children: Vec<ProcessSnapshotResponse>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}

/// Response for `GET /process/status` — lightweight poll-friendly summary.
#[derive(Debug, Serialize)]
pub struct ProcessStatusResponse {
    pub root_alive: bool,
    pub root_pid: Option<u32>,
    pub root_name: Option<String>,
    pub child_count: usize,
    pub work_done: bool,
    pub timestamp: String,
}
