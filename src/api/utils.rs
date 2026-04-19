#[cfg(target_os = "linux")]
use super::models::{FileDescriptorResponse, IOStatsResponse};

use super::models::ProcessSnapshotResponse;

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn format_memory(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub fn snapshot_to_response(
    s: crate::core::process_tracker::ProcessSnapshot,
) -> ProcessSnapshotResponse {
    ProcessSnapshotResponse {
        memory_human: format_memory(s.memory_bytes),
        pid: s.pid,
        name: s.name,
        state: s.state.to_string(),
        cpu_usage: s.cpu_usage,
        memory_bytes: s.memory_bytes,
        #[cfg(target_os = "linux")]
        cwd: s.cwd,
        #[cfg(target_os = "linux")]
        cmdline: s.cmdline,
        #[cfg(target_os = "linux")]
        open_fds: s.open_files.len(),
        #[cfg(target_os = "linux")]
        open_files: s
            .open_files
            .into_iter()
            .map(|f| {
                use crate::core::process_tracker::FDType;
                FileDescriptorResponse {
                    fd: f.fd,
                    target: f.target,
                    fd_type: match f.fd_type {
                        FDType::File => "file",
                        FDType::Socket => "socket",
                        FDType::Pipe => "pipe",
                        FDType::Other => "other",
                    }
                    .to_string(),
                }
            })
            .collect(),
        #[cfg(target_os = "linux")]
        io_stats: s.io_stats.map(|io| IOStatsResponse {
            read_bytes: io.read_bytes,
            write_bytes: io.write_bytes,
            read_chars: io.read_chars,
            write_chars: io.write_chars,
        }),
    }
}
