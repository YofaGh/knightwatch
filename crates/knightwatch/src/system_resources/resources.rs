use nvml_wrapper::Nvml;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use sysinfo::{Components, CpuRefreshKind, Disks, Networks, System};
use tokio::{
    sync::{broadcast, mpsc},
    time::Duration,
};

use kw_types::{
    polling::Poll,
    resources::{
        BatterySnapshot, BatteryState, CpuSnapshot, DiskSnapshot, GpuSnapshot, HostInfo,
        MemorySnapshot, NetworkSnapshot, RefreshMask, SystemSnapshot, ThermalSnapshot, Thresholds,
    },
};
use kw_utils::conv::u64_ratio_percent_f32;

use super::{
    commands::{SystemResourcesChannels, SystemResourcesCommand, SystemResourcesQuery},
    event::SystemResourcesEvent,
    system::{StaticHostInfo, ThresholdAlarm},
};
use crate::prelude::*;

/// How far below the threshold a value must drop before we consider it "cleared".
/// Prevents rapid on/off flapping when a value hovers right at the line.
const THRESHOLD_HYSTERESIS: f32 = 5.0;

/// Once alarmed, how long to wait before re-emitting the same alert if the
/// condition is still active (instead of spamming every tick).
const THRESHOLD_REPEAT_COOLDOWN: Duration = Duration::from_mins(5);

struct SystemResourcesState {
    last_snapshot: Option<SystemSnapshot>,
    last_battery_state: Option<BatteryState>,
    cpu_alarm: ThresholdAlarm,
    memory_alarm: ThresholdAlarm,
    disk_alarms: HashMap<String, ThresholdAlarm>,
    battery_low_alarm: ThresholdAlarm,
}

impl SystemResourcesState {
    fn new() -> Self {
        Self {
            last_snapshot: None,
            last_battery_state: None,
            cpu_alarm: ThresholdAlarm::default(),
            memory_alarm: ThresholdAlarm::default(),
            disk_alarms: HashMap::new(),
            battery_low_alarm: ThresholdAlarm::default(),
        }
    }
}

impl From<&SystemResourcesState> for kw_types::resources::AlarmSnapshot {
    fn from(s: &SystemResourcesState) -> Self {
        Self {
            cpu: (&s.cpu_alarm).into(),
            memory: (&s.memory_alarm).into(),
            disks: s
                .disk_alarms
                .iter()
                .map(|(mount, alarm)| (mount.clone(), alarm.into()))
                .collect(),
            battery_low: (&s.battery_low_alarm).into(),
        }
    }
}

struct SystemResources {
    state: SystemResourcesState,
    channels: SystemResourcesChannels,
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    nvml: Option<Nvml>,
    poll: Poll,
    thresholds: Thresholds,
    first_tick: bool,
    static_host_info: StaticHostInfo,
    refresh_mask: RefreshMask,
    uptime_baseline: u64,
    uptime_started: std::time::Instant,
}

impl SystemResources {
    pub fn new() -> Self {
        Self {
            state: SystemResourcesState::new(),
            channels: SystemResourcesChannels::new(),
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
            poll: Poll::new(1),
            thresholds: Thresholds::default(),
            first_tick: true,
            static_host_info: super::utils::get_static_host_info(),
            refresh_mask: RefreshMask::default(),
            uptime_baseline: System::uptime(),
            uptime_started: std::time::Instant::now(),
        }
    }

    fn emit_event(&self, event: SystemResourcesEvent) {
        // Err means no subscribers — that's fine.
        let _ = self.channels.event_tx.send(event);
    }

    async fn start_resource_loop(mut self) -> Result<()> {
        let mut query_rx = self.channels.take_query_rx()?;
        let mut command_rx = self.channels.take_command_rx()?;
        self.poll.resume();
        loop {
            let tick = async {
                match self.poll.interval_timer.as_mut() {
                    Some(timer) => timer.tick().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(command) = command_rx.recv() => {
                    self.handle_command(command);
                }
                _ = tick => {
                    self.handle_tick();
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Query handler
    // -----------------------------------------------------------------------

    fn handle_query(&self, query: SystemResourcesQuery) {
        match query {
            SystemResourcesQuery::Snapshot { response } => {
                let _ = response.send(self.state.last_snapshot.clone());
            }
            SystemResourcesQuery::Cpu { response } => {
                let _ = response.send(self.state.last_snapshot.as_ref().map(|s| s.cpu.clone()));
            }
            SystemResourcesQuery::Memory { response } => {
                let _ = response.send(self.state.last_snapshot.as_ref().map(|s| s.memory.clone()));
            }
            SystemResourcesQuery::Disks { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.disks.clone())
                        .unwrap_or_default(),
                );
            }
            SystemResourcesQuery::Networks { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.networks.clone())
                        .unwrap_or_default(),
                );
            }
            SystemResourcesQuery::Gpus { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.gpus.clone())
                        .unwrap_or_default(),
                );
            }
            SystemResourcesQuery::Battery { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .and_then(|s| s.battery.clone()),
                );
            }
            SystemResourcesQuery::HostInfo { response } => {
                let _ = response.send(self.build_host_info().into());
            }
            SystemResourcesQuery::Temperatures { response } => {
                let _ = response.send(
                    self.state
                        .last_snapshot
                        .as_ref()
                        .map(|s| s.temperatures.clone())
                        .unwrap_or_default(),
                );
            }
            SystemResourcesQuery::Alarms { response } => {
                let _ = response.send((&self.state).into());
            }
            SystemResourcesQuery::PollStatus { response } => {
                let _ = response.send(Some((&self.poll).into()));
            }
            SystemResourcesQuery::GetThresholds { response } => {
                let _ = response.send(Some(self.thresholds.clone()));
            }
            SystemResourcesQuery::GetRefreshMask { response } => {
                let _ = response.send(Some(self.refresh_mask.clone()));
            }
        }
    }

    fn handle_command(&mut self, command: SystemResourcesCommand) {
        match command {
            SystemResourcesCommand::SetThresholds {
                thresholds,
                response,
            } => {
                self.thresholds = thresholds;
                info!("thresholds updated");
                let _ = response.send(Ok(()));
            }
            SystemResourcesCommand::SetRefreshMask { mask, response } => {
                self.refresh_mask = mask;
                info!(
                    cpu = self.refresh_mask.cpu,
                    memory = self.refresh_mask.memory,
                    disks = self.refresh_mask.disks,
                    networks = self.refresh_mask.networks,
                    temperatures = self.refresh_mask.temperatures,
                    gpus = self.refresh_mask.gpus,
                    "refresh mask updated"
                );
                let _ = response.send(Ok(()));
            }
            SystemResourcesCommand::SetPollInterval { interval, response } => {
                self.poll.set_interval(interval);
                info!(
                    ms = interval.as_millis(),
                    "system resources poll interval updated"
                );
                let _ = response.send(Ok(()));
            }
            SystemResourcesCommand::PausePoll { response } => {
                self.poll.pause();
                info!("system resources polling paused");
                let _ = response.send(Ok(()));
            }
            SystemResourcesCommand::ResumePoll { response } => {
                self.poll.resume();
                info!("system resources polling resumed");
                let _ = response.send(Ok(()));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tick — refresh sysinfo, build snapshot, emit events
    // -----------------------------------------------------------------------

    fn handle_tick(&mut self) {
        self.refresh_all();
        let snapshot = self.build_snapshot();
        // CPU
        if Self::should_emit(
            &mut self.state.cpu_alarm,
            snapshot.cpu.usage_percent,
            self.thresholds.cpu_warn,
        ) {
            self.emit_event(SystemResourcesEvent::CpuThresholdExceeded {
                usage_percent: snapshot.cpu.usage_percent,
                threshold: self.thresholds.cpu_warn,
            });
        }
        // Memory
        if Self::should_emit(
            &mut self.state.memory_alarm,
            snapshot.memory.used_percent,
            self.thresholds.memory_warn,
        ) {
            self.emit_event(SystemResourcesEvent::MemoryThresholdExceeded {
                used_percent: snapshot.memory.used_percent,
                threshold: self.thresholds.memory_warn,
            });
        }
        // Disks
        for disk in &snapshot.disks {
            let alarm = self
                .state
                .disk_alarms
                .entry(disk.mount_point.clone())
                .or_default();
            if Self::should_emit(alarm, disk.used_percent, self.thresholds.disk_warn) {
                self.emit_event(SystemResourcesEvent::DiskThresholdExceeded {
                    mount_point: disk.mount_point.clone(),
                    used_percent: disk.used_percent,
                    threshold: self.thresholds.disk_warn,
                });
            }
        }
        // Battery
        if let Some(ref bat) = snapshot.battery {
            let is_low = bat.state == BatteryState::Discharging
                && bat.charge_percent <= self.thresholds.battery_low;
            if Self::should_emit(
                &mut self.state.battery_low_alarm,
                if is_low { 1.0 } else { 0.0 },
                1.0,
            ) {
                self.emit_event(SystemResourcesEvent::BatteryLow {
                    charge_percent: bat.charge_percent,
                    threshold: self.thresholds.battery_low,
                });
            }
            let prev_state = self.state.last_battery_state.take();
            if prev_state.as_ref() != Some(&bat.state) {
                self.emit_event(SystemResourcesEvent::BatteryStateChanged {
                    state: bat.state.clone(),
                });
            }
            self.state.last_battery_state = Some(bat.state.clone());
        }
        if self.first_tick {
            info!("System Resources: initial snapshot ready");
            self.emit_event(SystemResourcesEvent::InitialSnapshot {
                snapshot: snapshot.clone(),
            });
            self.first_tick = false;
        } else {
            self.emit_event(SystemResourcesEvent::Tick {
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
        if self.refresh_mask.cpu {
            self.sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        }
        if self.refresh_mask.memory {
            self.sys.refresh_memory();
        }
        if self.refresh_mask.disks {
            self.disks.refresh(false);
        }
        if self.refresh_mask.networks {
            self.networks.refresh(false);
        }
        if self.refresh_mask.temperatures {
            self.components.refresh(false);
        }
    }

    // -----------------------------------------------------------------------
    // Snapshot construction
    // -----------------------------------------------------------------------

    fn build_snapshot(&self) -> SystemSnapshot {
        let cpu = self.build_cpu_snapshot();
        let memory = self.build_memory_snapshot();
        let disks = self.build_disk_snapshots();
        let battery = Self::build_battery_snapshot();
        let health = super::utils::derive_health(&cpu, &memory, &disks, battery.as_ref());
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
        let frequency_mhz = cpus.first().map_or(0, sysinfo::Cpu::frequency);
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
            load_avg: Some(System::load_average().into()),
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            load_avg: None,
        }
    }

    fn build_memory_snapshot(&self) -> MemorySnapshot {
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let used_percent = if total > 0 {
            u64_ratio_percent_f32(used, total)
        } else {
            0.0
        };

        let swap_total = self.sys.total_swap();
        let swap_used = self.sys.used_swap();
        let swap_used_percent = if swap_total > 0 {
            Some(u64_ratio_percent_f32(swap_used, swap_total))
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

    fn build_battery_snapshot() -> Option<BatterySnapshot> {
        starship_battery::Manager::new()
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
            uptime_secs: self
                .uptime_baseline
                .saturating_add(self.uptime_started.elapsed().as_secs()),
            process_count: self.sys.processes().len(),
        }
    }

    fn should_emit(alarm: &mut ThresholdAlarm, value: f32, threshold: f32) -> bool {
        let now = std::time::SystemTime::now();

        if value >= threshold {
            if alarm.exceeded {
                // still exceeded — only re-notify after the cooldown
                let should = alarm.last_emitted.is_none_or(|t| {
                    now.duration_since(t).unwrap_or_default() >= THRESHOLD_REPEAT_COOLDOWN
                });
                if should {
                    alarm.last_emitted = Some(now);
                }
                should
            } else {
                // rising edge — record when this alarm started, always emit
                alarm.exceeded = true;
                alarm.since = Some(now);
                alarm.last_emitted = Some(now);
                true
            }
        } else {
            if value <= threshold - THRESHOLD_HYSTERESIS {
                // cleared — reset everything so the next rise counts as a fresh edge
                alarm.exceeded = false;
                alarm.since = None;
                alarm.last_emitted = None;
            }
            false
        }
    }
}

pub static SYSTEM_RESOURCES_QUERY_SENDER: OnceLock<mpsc::Sender<SystemResourcesQuery>> =
    OnceLock::new();
pub static SYSTEM_RESOURCES_EVENT_SENDER: OnceLock<broadcast::Sender<SystemResourcesEvent>> =
    OnceLock::new();
pub static SYSTEM_RESOURCES_COMMAND_SENDER: OnceLock<mpsc::Sender<SystemResourcesCommand>> =
    OnceLock::new();

static SYSTEM_RESOURCES: OnceLock<Mutex<Option<SystemResources>>> = OnceLock::new();

pub fn init_system_resources() {
    let config = get_config();
    if !config.args.system_resources {
        return;
    }
    let resources = SystemResources::new();
    let _ = SYSTEM_RESOURCES_QUERY_SENDER.set(resources.channels.query_tx.clone());
    let _ = SYSTEM_RESOURCES_EVENT_SENDER.set(resources.channels.event_tx.clone());
    if config.args.allow_system_resources_commands {
        let _ = SYSTEM_RESOURCES_COMMAND_SENDER.set(resources.channels.command_tx.clone());
    }
    let _ = SYSTEM_RESOURCES.set(Mutex::new(Some(resources)));
}

pub fn start_system_resources() {
    let Some(resources) = SYSTEM_RESOURCES
        .get()
        .and_then(|cell| cell.lock().ok())
        .and_then(|mut guard| guard.take())
    else {
        return;
    };
    tokio::spawn(async move {
        if let Err(e) = Box::pin(resources.start_resource_loop()).await {
            error!(?e, "system resources loop exited with error");
        }
    });
    info!("System Resources started");
}
