#[cfg(target_os = "linux")]
use procfs::process::{FDTarget, Process};

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

// Linux-only helper functions
#[cfg(target_os = "linux")]
pub fn collect_file_descriptors(pid: u32) -> Vec<super::structs::FileDescriptorInfo> {
    use super::enums::FDType;

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
                fds.push(super::structs::FileDescriptorInfo {
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
pub fn collect_io_stats(pid: u32) -> Option<super::structs::IOStats> {
    Process::new(pid as i32)
        .ok()
        .and_then(|p| p.io().ok())
        .map(|io| super::structs::IOStats {
            read_bytes: io.read_bytes,
            write_bytes: io.write_bytes,
            read_chars: io.rchar,
            write_chars: io.wchar,
        })
}

#[cfg(target_os = "linux")]
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