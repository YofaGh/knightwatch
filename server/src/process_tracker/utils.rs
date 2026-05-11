#[cfg(target_os = "linux")]
use procfs::process::Process;

// Linux-only helper functions
#[cfg(target_os = "linux")]
pub fn collect_file_descriptors(pid: u32) -> Vec<super::structs::FileDescriptorInfo> {
    if let Ok(process) = Process::new(pid as i32)
        && let Ok(fd_iter) = process.fd()
    {
        fd_iter.flatten().map(|fd_info| fd_info.into()).collect()
    } else {
        vec![]
    }
}

#[cfg(target_os = "linux")]
pub fn collect_io_stats(pid: u32) -> Option<super::structs::IOStats> {
    Process::new(pid as i32)
        .ok()
        .and_then(|p| p.io().ok())
        .map(Into::into)
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
