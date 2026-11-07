use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tokio::{
    sync::{broadcast, mpsc},
    time::Duration,
};

use kw_types::{
    polling::Poll,
    resources::{BatteryState, RefreshMask, SystemSnapshot, Thresholds},
};

use super::{
    commands::{
        SystemResourcesChannels, SystemResourcesCommand, SystemResourcesCommandAction,
        SystemResourcesQuery,
    },
    event::SystemResourcesEvent,
    source::{ResourceSource, SysinfoSource},
    system::ThresholdAlarm,
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

struct SystemResources<S = SysinfoSource> {
    state: SystemResourcesState,
    channels: SystemResourcesChannels,
    source: S,
    poll: Poll,
    thresholds: Thresholds,
    first_tick: bool,
    refresh_mask: RefreshMask,
}

impl SystemResources {
    pub fn new() -> Self {
        Self {
            state: SystemResourcesState::new(),
            channels: SystemResourcesChannels::new(),
            source: SysinfoSource::new(),
            poll: Poll::new(1),
            thresholds: Thresholds::default(),
            first_tick: true,
            refresh_mask: RefreshMask::default(),
        }
    }
}

impl<S: ResourceSource> SystemResources<S> {
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
                let _ = response.send(self.source.host().into());
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
                user,
                thresholds,
                response,
            } => {
                self.thresholds = thresholds.clone();
                info!("thresholds updated");
                let result = Ok(());
                self.emit_command_event(
                    user,
                    SystemResourcesCommandAction::SetThresholds { thresholds },
                    &result,
                );
                let _ = response.send(result);
            }
            SystemResourcesCommand::SetRefreshMask {
                user,
                mask,
                response,
            } => {
                self.refresh_mask = mask.clone();
                info!(
                    cpu = self.refresh_mask.cpu,
                    memory = self.refresh_mask.memory,
                    disks = self.refresh_mask.disks,
                    networks = self.refresh_mask.networks,
                    temperatures = self.refresh_mask.temperatures,
                    gpus = self.refresh_mask.gpus,
                    "refresh mask updated"
                );
                let result = Ok(());
                self.emit_command_event(
                    user,
                    SystemResourcesCommandAction::SetRefreshMask { refresh_mask: mask },
                    &result,
                );
                let _ = response.send(result);
            }
            SystemResourcesCommand::SetPollInterval {
                user,
                interval,
                response,
            } => {
                self.poll.set_interval(interval);
                info!(
                    ms = interval.as_millis(),
                    "system resources poll interval updated"
                );
                let result = Ok(());
                self.emit_command_event(
                    user,
                    SystemResourcesCommandAction::SetPollInterval { interval },
                    &result,
                );
                let _ = response.send(result);
            }
            SystemResourcesCommand::PausePoll { user, response } => {
                self.poll.pause();
                info!("system resources polling paused");
                let result = Ok(());
                self.emit_command_event(user, SystemResourcesCommandAction::PausePoll, &result);
                let _ = response.send(result);
            }
            SystemResourcesCommand::ResumePoll { user, response } => {
                self.poll.resume();
                info!("system resources polling resumed");
                let result = Ok(());
                self.emit_command_event(user, SystemResourcesCommandAction::ResumePoll, &result);
                let _ = response.send(result);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tick — refresh sysinfo, build snapshot, emit events
    // -----------------------------------------------------------------------

    fn handle_tick(&mut self) {
        self.source.refresh(&self.refresh_mask);
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
    // Snapshot construction
    // -----------------------------------------------------------------------

    fn build_snapshot(&self) -> SystemSnapshot {
        let cpu = self.source.cpu();
        let memory = self.source.memory();
        let disks = self.source.disks();
        let battery = self.source.battery();
        let health = super::utils::derive_health(&cpu, &memory, &disks, battery.as_ref());
        SystemSnapshot {
            timestamp: crate::utils::now_rfc3339(),
            cpu,
            memory,
            disks,
            networks: self.source.networks(),
            gpus: self.source.gpus(),
            battery,
            temperatures: self.source.temperatures(),
            host: self.source.host(),
            health,
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

    /// Emits a `CommandExecuted` event for any mutating command
    fn emit_command_event(
        &self,
        user: DisplayUser,
        action: SystemResourcesCommandAction,
        result: &Result<()>,
    ) {
        let (success, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };

        if success {
            info!(
                %user,
                action = %action,
                "system resources command executed"
            );
        } else {
            warn!(
                %user,
                action = %action,
                error,
                "system resources command failed"
            );
        }

        self.emit_event(SystemResourcesEvent::CommandExecuted {
            user,
            action,
            success,
            error,
        });
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

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
