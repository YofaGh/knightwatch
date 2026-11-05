use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

use kw_types::process::ProcessSnapshot;

/// What the tracker needs from the OS, so its diff logic can run on a fake in tests.
pub trait ProcessSource: Send {
    /// Refresh every process with cpu, memory and disk usage, dropping dead ones.
    fn refresh_usage(&mut self);
    /// Refresh every process without usage numbers, dropping dead ones.
    fn refresh_all(&mut self);
    /// Refresh one process without usage numbers, keeping the others as they are.
    fn refresh_pid(&mut self, pid: u32);
    fn snapshot(&self, pid: u32) -> Option<ProcessSnapshot>;
    /// `(pid, parent)` for every process that has a parent.
    fn parents(&self) -> Vec<(u32, u32)>;
    /// `(pid, cpu, memory, disk)` for every process.
    fn usage(&self) -> Vec<(u32, f32, u64, u64)>;
    /// False if the process is missing or the kill failed.
    fn kill(&self, pid: u32) -> bool;
    /// `None` if the process is missing, otherwise whether the signal was delivered.
    fn kill_with(&self, pid: u32, signal: Signal) -> Option<bool>;
}

pub struct SysinfoSource(pub System);

impl ProcessSource for SysinfoSource {
    fn refresh_usage(&mut self) {
        self.0.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage(),
        );
    }

    fn refresh_all(&mut self) {
        self.0.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
    }

    fn refresh_pid(&mut self, pid: u32) {
        self.0.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            false,
            ProcessRefreshKind::nothing(),
        );
    }

    fn snapshot(&self, pid: u32) -> Option<ProcessSnapshot> {
        self.0.process(Pid::from_u32(pid)).map(Into::into)
    }

    fn parents(&self) -> Vec<(u32, u32)> {
        self.0
            .processes()
            .iter()
            .filter_map(|(pid, proc)| Some((pid.as_u32(), proc.parent()?.as_u32())))
            .collect()
    }

    fn usage(&self) -> Vec<(u32, f32, u64, u64)> {
        self.0
            .processes()
            .values()
            .map(|p| {
                let disk_usage = p.disk_usage();
                (
                    p.pid().as_u32(),
                    p.cpu_usage(),
                    p.memory(),
                    disk_usage
                        .written_bytes
                        .saturating_add(disk_usage.read_bytes),
                )
            })
            .collect()
    }

    fn kill(&self, pid: u32) -> bool {
        self.0
            .process(Pid::from_u32(pid))
            .is_some_and(sysinfo::Process::kill)
    }

    fn kill_with(&self, pid: u32, signal: Signal) -> Option<bool> {
        self.0
            .process(Pid::from_u32(pid))
            .map(|p| p.kill_with(signal).unwrap_or(false))
    }
}
