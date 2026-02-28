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
    }
}
