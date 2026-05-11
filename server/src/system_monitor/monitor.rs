use battery::Manager as BatterManager;
use nvml_wrapper::Nvml;
use std::sync::OnceLock;
use sysinfo::{Components, CpuRefreshKind, Disks, Networks, System};
use tokio::{
    sync::{broadcast, mpsc},
    time::Duration,
};

use super::{enums::*, structs::*, utils::*};
use crate::prelude::*;

pub struct SystemMonitorChannels {
    pub query_tx: mpsc::Sender<SystemMonitorQuery>,
    pub query_rx: Option<mpsc::Receiver<SystemMonitorQuery>>,
    pub event_tx: broadcast::Sender<SystemMonitorEvent>,
}

impl SystemMonitorChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            event_tx,
        }
    }

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<SystemMonitorQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::SystemMonitor("Query receiver already taken".into()))
    }
}

struct SystemMonitorState {
    last_snapshot: Option<SystemSnapshot>,
    last_battery_state: Option<BatteryState>,
}

impl SystemMonitorState {
    fn new() -> Self {
        Self {
            last_snapshot: None,
            last_battery_state: None,
        }
    }
}

struct SystemMonitor {
    state: SystemMonitorState,
    channels: SystemMonitorChannels,
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    nvml: Option<Nvml>,
    poll_interval: Duration,
    poll_interval_timer: Option<tokio::time::Interval>,
    thresholds: Thresholds,
    first_tick: bool,
    static_host_info: StaticHostInfo,
    uptime_baseline: u64,
    uptime_started: std::time::Instant,
}

impl SystemMonitor {
    pub fn new() -> Self {
        Self {
            state: SystemMonitorState::new(),
            channels: SystemMonitorChannels::new(),
            sys: System::new_with_specifics(
                sysinfo::RefreshKind::nothing()
                    .with_cpu(CpuRefreshKind::everything())
                    .with_memory(sysinfo::MemoryRefreshKind::everything())
                    .with_processes(sysinfo::ProcessRefreshKind::nothing()),
            ),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            nvml: Nvml::init().ok(),
            poll_interval: Duration::from_secs(1),
            poll_interval_timer: None,
            thresholds: Thresholds::default(),
            first_tick: true,
            static_host_info: super::utils::get_static_host_info(),
            uptime_baseline: System::uptime(),
            uptime_started: std::time::Instant::now(),
        }
    }

    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    fn emit_event(&self, event: SystemMonitorEvent) {
        // Err means no subscribers — that's fine.
        let _ = self.channels.event_tx.send(event);
    }

    async fn start_monitoring_loop(mut self) -> Result<()> {
        let mut query_rx = self
            .channels
            .take_query_rx()
            .expect("Failed to take query receiver");
        self.poll_interval_timer = Some(tokio::time::interval(self.poll_interval));
        loop {
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                _ = async { self.poll_interval_timer.as_mut().unwrap().tick().await }, if self.poll_interval_timer.is_some() => {
                    self.handle_tick().await;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Query handler
    // -----------------------------------------------------------------------

    fn handle_query(&self, query: SystemMonitorQuery) {
        match query {
            SystemMonitorQuery::Snapshot { response } => {
                let _ = response.send(self.state.last_snapshot.clone());
            }
            SystemMonitorQuery::Cpu { response } => {
                let _ = response.send(self.state.last_snapshot.as_ref().map(|s| s.cpu.clone()));
            }
            SystemMonitorQuery::Memory { response } => {
                let _ = response.send(self.state.last_snapshot.as_ref().map(|s| s.memory.clone()));
            }
            SystemMonitorQuery::Disks { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.disks.clone())
                        .unwrap_or_default(),
                );
            }
            SystemMonitorQuery::Networks { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.networks.clone())
                        .unwrap_or_default(),
                );
            }
            SystemMonitorQuery::Gpus { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.gpus.clone())
                        .unwrap_or_default(),
                );
            }
            SystemMonitorQuery::Battery { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .and_then(|s| s.battery.clone()),
                );
            }
            SystemMonitorQuery::HostInfo { response } => {
                let _ = response.send(self.build_host_info().into());
            }
            SystemMonitorQuery::Temperatures { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.temperatures.clone())
                        .unwrap_or_default(),
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tick — refresh sysinfo, build snapshot, emit events
    // -----------------------------------------------------------------------

    async fn handle_tick(&mut self) {
        self.refresh_all();
        let snapshot = self.build_snapshot();
        // CPU
        if snapshot.cpu.usage_percent >= self.thresholds.cpu_warn {
            self.emit_event(SystemMonitorEvent::CpuThresholdExceeded {
                usage_percent: snapshot.cpu.usage_percent,
                threshold: self.thresholds.cpu_warn,
            });
        }
        // Memory
        if snapshot.memory.used_percent >= self.thresholds.memory_warn {
            self.emit_event(SystemMonitorEvent::MemoryThresholdExceeded {
                used_percent: snapshot.memory.used_percent,
                threshold: self.thresholds.memory_warn,
            });
        }
        // Disks
        for disk in &snapshot.disks {
            if disk.used_percent >= self.thresholds.disk_warn {
                self.emit_event(SystemMonitorEvent::DiskThresholdExceeded {
                    mount_point: disk.mount_point.clone(),
                    used_percent: disk.used_percent,
                    threshold: self.thresholds.disk_warn,
                });
            }
        }
        // Battery
        if let Some(ref bat) = snapshot.battery {
            if bat.state == BatteryState::Discharging
                && bat.charge_percent <= self.thresholds.battery_low
            {
                self.emit_event(SystemMonitorEvent::BatteryLow {
                    charge_percent: bat.charge_percent,
                    threshold: self.thresholds.battery_low,
                });
            }
            let prev_state = self.state.last_battery_state.take();
            if prev_state.as_ref() != Some(&bat.state) {
                self.emit_event(SystemMonitorEvent::BatteryStateChanged {
                    state: bat.state.clone(),
                });
            }
            self.state.last_battery_state = Some(bat.state.clone());
        }
        if self.first_tick {
            info!("System Monitor: initial snapshot ready");
            self.emit_event(SystemMonitorEvent::InitialSnapshot {
                snapshot: snapshot.clone(),
            });
            self.first_tick = false;
        } else {
            self.emit_event(SystemMonitorEvent::Tick {
                snapshot: snapshot.clone(),
            });
        }
        self.state.last_snapshot = Some(snapshot);
    }

    // -----------------------------------------------------------------------
    // Refresh
    // -----------------------------------------------------------------------

    fn refresh_all(&mut self) {
        // sysinfo requires two CPU ticks to produce non-zero usage numbers.
        self.sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        self.sys.refresh_memory();
        self.disks.refresh(false);
        self.networks.refresh(false);
        self.components.refresh(false);
    }

    // -----------------------------------------------------------------------
    // Snapshot construction
    // -----------------------------------------------------------------------

    fn build_snapshot(&self) -> SystemSnapshot {
        let cpu = self.build_cpu_snapshot();
        let memory = self.build_memory_snapshot();
        let disks = self.build_disk_snapshots();
        let battery = self.build_battery_snapshot();
        let health = derive_health(&cpu, &memory, &disks, &battery);
        SystemSnapshot {
            timestamp: crate::utils::now_rfc3339(),
            cpu,
            memory,
            disks,
            networks: self.build_network_snapshots(),
            gpus: self.build_gpu_snapshots(),
            battery,
            temperatures: self.build_thermal_snapshots(),
            host: self.build_host_info(),
            health,
        }
    }

    fn build_cpu_snapshot(&self) -> CpuSnapshot {
        let cpus = self.sys.cpus();
        let usage_percent = self.sys.global_cpu_usage();
        let cores = cpus.iter().map(Into::into).collect();
        let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
        let brand = cpus
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default();
        CpuSnapshot {
            usage_percent,
            cores,
            frequency_mhz,
            brand,
            physical_core_count: System::physical_core_count(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            load_avg: System::load_average().into(),
        }
    }

    fn build_memory_snapshot(&self) -> MemorySnapshot {
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let used_percent = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        let swap_total = self.sys.total_swap();
        let swap_used = self.sys.used_swap();
        let swap_used_percent = if swap_total > 0 {
            Some((swap_used as f32 / swap_total as f32) * 100.0)
        } else {
            None
        };
        MemorySnapshot {
            total_bytes: total,
            used_bytes: used,
            available_bytes: self.sys.available_memory(),
            free_bytes: self.sys.free_memory(),
            used_percent,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_used,
            swap_free_bytes: self.sys.free_swap(),
            swap_used_percent,
        }
    }

    fn build_disk_snapshots(&self) -> Vec<DiskSnapshot> {
        self.disks.iter().map(Into::into).collect()
    }

    fn build_network_snapshots(&self) -> Vec<NetworkSnapshot> {
        self.networks.iter().map(Into::into).collect()
    }

    fn build_thermal_snapshots(&self) -> Vec<ThermalSnapshot> {
        self.components.iter().map(Into::into).collect()
    }

    fn build_battery_snapshot(&self) -> Option<BatterySnapshot> {
        BatterManager::new()
            .ok()?
            .batteries()
            .ok()?
            .next()?
            .ok()
            .map(Into::into)
    }

    fn build_gpu_snapshots(&self) -> Vec<GpuSnapshot> {
        let Some(ref nvml) = self.nvml else {
            return vec![];
        };
        let devices_count = nvml.device_count().unwrap_or(0);
        (0..devices_count)
            .filter_map(|i| nvml.device_by_index(i).ok().map(Into::into))
            .collect()
    }

    fn build_host_info(&self) -> HostInfo {
        HostInfo {
            hostname: self.static_host_info.hostname.clone(),
            os_name: self.static_host_info.os_name.clone(),
            kernel_version: self.static_host_info.kernel_version.clone(),
            cpu_arch: self.static_host_info.cpu_arch.clone(),
            uptime_secs: self.uptime_baseline + self.uptime_started.elapsed().as_secs(),
            process_count: self.sys.processes().len(),
        }
    }
}

pub static SYSTEM_MONITOR_QUERY_SENDER: OnceLock<mpsc::Sender<SystemMonitorQuery>> =
    OnceLock::new();
pub static SYSTEM_MONITOR_EVENT_SENDER: OnceLock<broadcast::Sender<SystemMonitorEvent>> =
    OnceLock::new();

pub fn init_system_monitor() {
    if !get_config().args.system_monitor {
        return;
    }
    let monitor = SystemMonitor::new();
    SYSTEM_MONITOR_QUERY_SENDER
        .set(monitor.channels.query_tx.clone())
        .unwrap();
    SYSTEM_MONITOR_EVENT_SENDER
        .set(monitor.channels.event_tx.clone())
        .unwrap();
    tokio::spawn(async move {
        if let Err(e) = monitor.start_monitoring_loop().await {
            error!(?e, "system monitor loop exited with error");
        }
    });
    info!("System Monitor started");
}
