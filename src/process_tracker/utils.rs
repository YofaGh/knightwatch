use super::structs::ProcessInfo;

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

pub fn snapshot_to_response(s: &super::structs::ProcessSnapshot) -> ProcessInfo {
    ProcessInfo {
        memory_human: format_memory(s.memory_bytes),
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
