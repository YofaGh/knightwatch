#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{cell::Cell, time::SystemTime};
use tokio::sync::oneshot;

use kw_types::{
    event::EventPayload,
    polling::PollStatus,
    resources::{
        AlarmSnapshot, BatterySnapshot, CpuSnapshot, DiskKind, DiskSnapshot, GpuSnapshot, HostInfo,
        MemorySnapshot, NetworkSnapshot, SystemHealth, ThermalSnapshot,
    },
};

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The readings a refresh can change.
#[derive(Clone)]
struct Readings {
    cpu: f32,
    memory: f32,
    disks: Vec<(String, f32)>,
    networks: Vec<String>,
    temperatures: Vec<String>,
}

/// `live` is the machine; `seen` is what the last refresh saw, per the mask.
/// Gpus and battery have no refresh step, so they read live.
struct Fake {
    live: Readings,
    seen: Readings,
    gpus: Vec<String>,
    gpu_reads: Cell<u32>,
    battery: Option<(BatteryState, f32)>,
}

impl ResourceSource for Fake {
    fn refresh(&mut self, mask: &RefreshMask) {
        if mask.cpu {
            self.seen.cpu = self.live.cpu;
        }
        if mask.memory {
            self.seen.memory = self.live.memory;
        }
        if mask.disks {
            self.seen.disks.clone_from(&self.live.disks);
        }
        if mask.networks {
            self.seen.networks.clone_from(&self.live.networks);
        }
        if mask.temperatures {
            self.seen.temperatures.clone_from(&self.live.temperatures);
        }
    }

    fn cpu(&self) -> CpuSnapshot {
        CpuSnapshot {
            usage_percent: self.seen.cpu,
            cores: vec![],
            frequency_mhz: 0,
            brand: "fake".into(),
            physical_core_count: None,
            load_avg: None,
        }
    }

    fn memory(&self) -> MemorySnapshot {
        MemorySnapshot {
            total_bytes: 0,
            used_bytes: 0,
            available_bytes: 0,
            free_bytes: 0,
            used_percent: self.seen.memory,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            swap_used_percent: None,
        }
    }

    fn disks(&self) -> Vec<DiskSnapshot> {
        self.seen
            .disks
            .iter()
            .map(|(mount_point, used_percent)| DiskSnapshot {
                name: "disk".into(),
                mount_point: mount_point.clone(),
                file_system: "ext4".into(),
                kind: DiskKind::Ssd,
                is_removable: false,
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                used_percent: *used_percent,
            })
            .collect()
    }

    fn networks(&self) -> Vec<NetworkSnapshot> {
        self.seen
            .networks
            .iter()
            .map(|interface| NetworkSnapshot {
                interface: interface.clone(),
                rx_bytes_per_sec: 0,
                tx_bytes_per_sec: 0,
                rx_total_bytes: 0,
                tx_total_bytes: 0,
                rx_packets_per_sec: 0,
                tx_packets_per_sec: 0,
                rx_errors: 0,
                tx_errors: 0,
            })
            .collect()
    }

    fn temperatures(&self) -> Vec<ThermalSnapshot> {
        self.seen
            .temperatures
            .iter()
            .map(|label| ThermalSnapshot {
                label: label.clone(),
                temperature_celsius: None,
                temperature_max_celsius: None,
                temperature_critical_celsius: None,
            })
            .collect()
    }

    fn gpus(&self) -> Vec<GpuSnapshot> {
        self.gpu_reads.set(self.gpu_reads.get() + 1);
        self.gpus
            .iter()
            .map(|name| GpuSnapshot {
                name: name.clone(),
                usage_percent: None,
                vram_used_bytes: None,
                vram_total_bytes: None,
                vram_used_percent: None,
                temperature_celsius: None,
                power_draw_watts: None,
                power_limit_watts: None,
                fan_speed_percent: vec![],
            })
            .collect()
    }

    fn battery(&self) -> Option<BatterySnapshot> {
        self.battery
            .as_ref()
            .map(|(state, charge_percent)| BatterySnapshot {
                charge_percent: *charge_percent,
                state: state.clone(),
                time_to_empty_secs: None,
                time_to_full_secs: None,
                power_draw_watts: None,
                cycle_count: None,
                health_percent: None,
            })
    }

    fn host(&self) -> HostInfo {
        HostInfo {
            hostname: Some("fake".into()),
            os_name: None,
            kernel_version: None,
            cpu_arch: None,
            uptime_secs: 7,
            process_count: 3,
        }
    }
}

/// Everything is healthy, there is one disk at `/` and no battery. Thresholds are the
/// defaults: 90 for cpu, memory and disk, and 15 for battery.
fn tracker() -> SystemResources<Fake> {
    let readings = Readings {
        cpu: 10.0,
        memory: 10.0,
        disks: vec![("/".into(), 10.0)],
        networks: vec!["eth0".into()],
        temperatures: vec!["cpu".into()],
    };
    SystemResources {
        state: SystemResourcesState::new(),
        channels: SystemResourcesChannels::new(),
        source: Fake {
            live: readings.clone(),
            seen: readings,
            gpus: vec!["gpu0".into()],
            gpu_reads: Cell::new(0),
            battery: None,
        },
        poll: Poll::new(1),
        thresholds: Thresholds::default(),
        first_tick: true,
        refresh_mask: RefreshMask::default(),
    }
}

impl SystemResources<Fake> {
    fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> SystemResourcesQuery) -> T {
        let (response, mut rx) = oneshot::channel();
        self.handle_query(make(response));
        rx.try_recv().unwrap()
    }

    fn run(
        &mut self,
        make: impl FnOnce(DisplayUser, oneshot::Sender<Result<()>>) -> SystemResourcesCommand,
    ) -> Result<()> {
        let (response, mut rx) = oneshot::channel();
        self.handle_command(make(user(), response));
        rx.try_recv().unwrap()
    }

    fn alarms(&self) -> AlarmSnapshot {
        self.ask(|response| SystemResourcesQuery::Alarms { response })
    }

    fn snapshot(&self) -> SystemSnapshot {
        self.ask(|response| SystemResourcesQuery::Snapshot { response })
            .unwrap()
    }

    fn poll_status(&self) -> PollStatus {
        self.ask(|response| SystemResourcesQuery::PollStatus { response })
            .unwrap()
    }
}

fn user() -> DisplayUser {
    DisplayUser {
        username: "alice".into(),
        telegram_chat_id: None,
    }
}

/// Events as short strings, because `SystemResourcesEvent` has no `PartialEq`.
fn drain(rx: &mut broadcast::Receiver<SystemResourcesEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(match event {
            SystemResourcesEvent::InitialSnapshot { .. } => "initial".to_string(),
            SystemResourcesEvent::Tick { .. } => "tick".to_string(),
            SystemResourcesEvent::CpuThresholdExceeded {
                usage_percent,
                threshold,
            } => format!("cpu {usage_percent}/{threshold}"),
            SystemResourcesEvent::MemoryThresholdExceeded {
                used_percent,
                threshold,
            } => format!("memory {used_percent}/{threshold}"),
            SystemResourcesEvent::DiskThresholdExceeded {
                mount_point,
                used_percent,
                threshold,
            } => format!("disk {mount_point} {used_percent}/{threshold}"),
            SystemResourcesEvent::BatteryLow {
                charge_percent,
                threshold,
            } => format!("battery_low {charge_percent}/{threshold}"),
            SystemResourcesEvent::BatteryStateChanged { state } => {
                format!("battery_state {state:?}")
            }
            SystemResourcesEvent::CommandExecuted {
                action, success, ..
            } => format!("command {} {success}", action.name()),
        });
    }
    out
}

/// The events of one tick, leaving out the `tick` every tick emits.
fn tick(
    t: &mut SystemResources<Fake>,
    rx: &mut broadcast::Receiver<SystemResourcesEvent>,
) -> Vec<String> {
    t.handle_tick();
    let mut events = drain(rx);
    assert_eq!(events.pop().as_deref(), Some("tick"));
    events
}

/// Past the first tick, with a subscriber.
fn running() -> (
    SystemResources<Fake>,
    broadcast::Receiver<SystemResourcesEvent>,
) {
    let mut t = tracker();
    t.handle_tick();
    let rx = t.channels.event_tx.subscribe();
    (t, rx)
}

/// As if the alarm last fired longer ago than the repeat cooldown.
fn cooldown_passed(alarm: &mut ThresholdAlarm) {
    let long_ago = THRESHOLD_REPEAT_COOLDOWN + Duration::from_secs(1);
    alarm.last_emitted = Some(SystemTime::now() - long_ago);
}

// ─────────────────────────────────────────────────────────────────────────────
// tick events
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_first_tick_is_the_initial_snapshot_and_later_ones_are_ticks() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();

    t.handle_tick();
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["initial", "tick"]);
}

#[test]
fn alarms_fire_before_the_snapshot_event_of_the_same_tick() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();
    t.source.live.cpu = 95.0;

    t.handle_tick();

    assert_eq!(drain(&mut rx), ["cpu 95/90", "initial"]);
}

#[test]
fn the_snapshot_carries_the_readings_and_the_derived_health() {
    let mut t = tracker();
    t.source.live.memory = 96.0;

    t.handle_tick();

    let snapshot = t.snapshot();
    assert_eq!(snapshot.cpu.usage_percent, 10.0);
    assert_eq!(snapshot.memory.used_percent, 96.0);
    assert_eq!(snapshot.disks[0].mount_point, "/");
    assert_eq!(snapshot.networks[0].interface, "eth0");
    assert_eq!(snapshot.temperatures[0].label, "cpu");
    assert_eq!(snapshot.gpus[0].name, "gpu0");
    assert_eq!(snapshot.host.uptime_secs, 7);
    assert!(snapshot.battery.is_none());
    assert_eq!(snapshot.health, SystemHealth::Critical);
}

// ─────────────────────────────────────────────────────────────────────────────
// threshold alarms
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn an_alarm_fires_at_the_threshold_and_not_just_below_it() {
    let (mut t, mut rx) = running();

    t.source.live.cpu = 89.9;
    assert!(tick(&mut t, &mut rx).is_empty());

    t.source.live.cpu = 90.0;
    assert_eq!(tick(&mut t, &mut rx), ["cpu 90/90"]);
}

#[test]
fn a_held_alarm_stays_quiet_until_the_cooldown_passes() {
    let (mut t, mut rx) = running();
    t.source.live.cpu = 95.0;
    tick(&mut t, &mut rx);
    let since = t.state.cpu_alarm.since;

    assert!(tick(&mut t, &mut rx).is_empty());

    cooldown_passed(&mut t.state.cpu_alarm);
    assert_eq!(tick(&mut t, &mut rx), ["cpu 95/90"]);
    assert!(tick(&mut t, &mut rx).is_empty(), "the cooldown restarted");
    assert_eq!(t.state.cpu_alarm.since, since, "a repeat is not a new edge");
}

#[test]
fn an_alarm_only_clears_below_the_hysteresis_band() {
    let (mut t, mut rx) = running();
    t.source.live.cpu = 95.0;
    tick(&mut t, &mut rx);

    t.source.live.cpu = 85.1;
    tick(&mut t, &mut rx);
    assert!(t.alarms().cpu.active, "still inside the band");

    t.source.live.cpu = 95.0;
    assert!(tick(&mut t, &mut rx).is_empty(), "not a new edge");

    t.source.live.cpu = 85.0;
    tick(&mut t, &mut rx);
    assert!(!t.alarms().cpu.active, "cleared at threshold - 5");

    t.source.live.cpu = 95.0;
    assert_eq!(tick(&mut t, &mut rx), ["cpu 95/90"], "a new edge");
}

#[test]
fn memory_alarms_work_like_cpu_alarms() {
    let (mut t, mut rx) = running();

    t.source.live.memory = 91.0;
    assert_eq!(tick(&mut t, &mut rx), ["memory 91/90"]);
    assert!(t.alarms().memory.active);
    assert!(!t.alarms().cpu.active);
}

#[test]
fn each_disk_has_its_own_alarm() {
    let (mut t, mut rx) = running();
    t.source.live.disks = vec![("/".into(), 95.0), ("/data".into(), 10.0)];

    assert_eq!(tick(&mut t, &mut rx), ["disk / 95/90"]);

    t.source.live.disks[1].1 = 92.0;
    assert_eq!(tick(&mut t, &mut rx), ["disk /data 92/90"]);
}

#[test]
fn new_thresholds_apply_from_the_next_tick() {
    let (mut t, mut rx) = running();
    let thresholds = Thresholds {
        cpu_warn: 5.0,
        memory_warn: 90.0,
        disk_warn: 90.0,
        battery_low: 15.0,
    };

    t.run(|user, response| SystemResourcesCommand::SetThresholds {
        user,
        thresholds,
        response,
    })
    .unwrap();

    assert_eq!(drain(&mut rx), ["command set_thresholds true"]);
    assert_eq!(tick(&mut t, &mut rx), ["cpu 10/5"]);
}

#[test]
fn the_alarm_query_reports_when_an_active_alarm_started() {
    let (mut t, mut rx) = running();
    assert!(t.alarms().cpu.since.is_none());

    t.source.live.cpu = 95.0;
    tick(&mut t, &mut rx);
    assert!(t.alarms().cpu.since.is_some());

    t.source.live.cpu = 10.0;
    tick(&mut t, &mut rx);
    assert!(t.alarms().cpu.since.is_none());
}

#[test]
fn a_removed_disk_keeps_its_alarm() {
    // Pinned (finding 58): `disk_alarms` entries are never removed, so an unmounted
    // disk that was full reads as an active alarm until the server restarts.
    let (mut t, mut rx) = running();
    t.source.live.disks = vec![("/usb".into(), 95.0)];
    tick(&mut t, &mut rx);

    t.source.live.disks.clear();
    tick(&mut t, &mut rx);

    let disks = t.alarms().disks;
    let usb = disks.iter().find(|(mount, _)| mount == "/usb").unwrap();
    assert!(usb.1.active);
}

#[test]
#[ignore = "finding 57: alarm disks come out in HashMap order, though the type says stable"]
fn alarm_disks_are_in_a_stable_order() {
    let (mut t, mut rx) = running();
    t.source.live.disks = (0..16).map(|i| (format!("/{i:02}"), 10.0)).collect();
    tick(&mut t, &mut rx);

    let mounts: Vec<String> = t.alarms().disks.into_iter().map(|d| d.0).collect();

    let mut expected = mounts.clone();
    expected.sort();
    assert_eq!(mounts, expected);
}

// ─────────────────────────────────────────────────────────────────────────────
// battery
// ─────────────────────────────────────────────────────────────────────────────

/// A subscribed tracker on battery at `charge`, with the state change already drained.
fn on_battery(
    charge: f32,
) -> (
    SystemResources<Fake>,
    broadcast::Receiver<SystemResourcesEvent>,
) {
    let (mut t, mut rx) = running();
    t.source.battery = Some((BatteryState::Discharging, charge));
    tick(&mut t, &mut rx);
    (t, rx)
}

#[test]
fn no_battery_emits_no_battery_events() {
    let (mut t, mut rx) = running();

    assert!(tick(&mut t, &mut rx).is_empty());
    assert!(!t.alarms().battery_low.active);
}

#[test]
fn battery_state_is_reported_when_first_seen_and_when_it_changes() {
    let (mut t, mut rx) = running();
    t.source.battery = Some((BatteryState::Charging, 50.0));

    assert_eq!(tick(&mut t, &mut rx), ["battery_state Charging"]);
    assert!(tick(&mut t, &mut rx).is_empty());

    t.source.battery = Some((BatteryState::Full, 100.0));
    assert_eq!(tick(&mut t, &mut rx), ["battery_state Full"]);
}

#[test]
fn battery_low_fires_at_the_threshold_only_while_discharging() {
    let (mut t, mut rx) = on_battery(15.1);
    assert!(!t.alarms().battery_low.active);

    t.source.battery = Some((BatteryState::Discharging, 15.0));
    assert_eq!(tick(&mut t, &mut rx), ["battery_low 15/15"]);
    assert!(t.alarms().battery_low.active);

    let (mut t, mut rx) = running();
    t.source.battery = Some((BatteryState::Charging, 5.0));
    assert_eq!(tick(&mut t, &mut rx), ["battery_state Charging"]);
    assert!(!t.alarms().battery_low.active);
}

#[test]
fn a_held_battery_alarm_repeats_after_the_cooldown() {
    let (mut t, mut rx) = on_battery(10.0);
    assert!(tick(&mut t, &mut rx).is_empty());

    cooldown_passed(&mut t.state.battery_low_alarm);
    assert_eq!(tick(&mut t, &mut rx), ["battery_low 10/15"]);
}

#[test]
#[ignore = "finding 56: the battery alarm never clears, because 0.0 is not <= 1.0 - 5.0"]
fn plugging_in_clears_the_battery_alarm() {
    let (mut t, mut rx) = on_battery(10.0);

    t.source.battery = Some((BatteryState::Charging, 10.0));
    tick(&mut t, &mut rx);

    assert!(!t.alarms().battery_low.active);
}

#[test]
#[ignore = "finding 56: the second low within the cooldown is silent, as the alarm never cleared"]
fn running_low_again_after_charging_alarms_again() {
    let (mut t, mut rx) = on_battery(10.0);
    t.source.battery = Some((BatteryState::Charging, 50.0));
    tick(&mut t, &mut rx);

    t.source.battery = Some((BatteryState::Discharging, 10.0));

    assert_eq!(
        tick(&mut t, &mut rx),
        ["battery_low 10/15", "battery_state Discharging"]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// refresh mask
// ─────────────────────────────────────────────────────────────────────────────

fn nothing() -> RefreshMask {
    RefreshMask {
        cpu: false,
        memory: false,
        disks: false,
        networks: false,
        temperatures: false,
        gpus: false,
    }
}

/// Changes every reading, then ticks with `mask` set through the command.
fn tick_with_changes(mask: RefreshMask) -> SystemSnapshot {
    let (mut t, _rx) = running();
    t.run(|user, response| SystemResourcesCommand::SetRefreshMask {
        user,
        mask,
        response,
    })
    .unwrap();
    t.source.live = Readings {
        cpu: 20.0,
        memory: 30.0,
        disks: vec![("/new".into(), 40.0)],
        networks: vec!["wlan0".into()],
        temperatures: vec!["gpu".into()],
    };
    t.handle_tick();
    t.snapshot()
}

#[test]
fn the_default_mask_refreshes_everything() {
    let snapshot = tick_with_changes(RefreshMask::default());

    assert_eq!(snapshot.cpu.usage_percent, 20.0);
    assert_eq!(snapshot.memory.used_percent, 30.0);
    assert_eq!(snapshot.disks[0].mount_point, "/new");
    assert_eq!(snapshot.networks[0].interface, "wlan0");
    assert_eq!(snapshot.temperatures[0].label, "gpu");
}

#[test]
fn a_masked_subsystem_keeps_its_last_reading() {
    let snapshot = tick_with_changes(nothing());

    assert_eq!(snapshot.cpu.usage_percent, 10.0);
    assert_eq!(snapshot.memory.used_percent, 10.0);
    assert_eq!(snapshot.disks[0].mount_point, "/");
    assert_eq!(snapshot.networks[0].interface, "eth0");
    assert_eq!(snapshot.temperatures[0].label, "cpu");
}

#[test]
fn each_mask_bit_controls_only_its_own_subsystem() {
    let snapshot = tick_with_changes(RefreshMask {
        memory: true,
        ..nothing()
    });

    assert_eq!(snapshot.memory.used_percent, 30.0);
    assert_eq!(snapshot.cpu.usage_percent, 10.0);
}

#[test]
fn a_fully_masked_tick_still_emits_a_tick() {
    let (mut t, mut rx) = running();
    t.refresh_mask = nothing();

    assert!(tick(&mut t, &mut rx).is_empty());
}

#[test]
#[ignore = "finding 54: `RefreshMask.gpus` is never read, so gpus are queried every tick"]
fn masking_gpus_stops_reading_them() {
    let (mut t, _rx) = running();
    t.refresh_mask = nothing();
    let reads = t.source.gpu_reads.get();

    t.handle_tick();

    assert_eq!(t.source.gpu_reads.get(), reads);
}

// ─────────────────────────────────────────────────────────────────────────────
// queries
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn queries_before_the_first_tick_are_empty_except_host_and_settings() {
    let t = tracker();

    assert!(
        t.ask(|response| SystemResourcesQuery::Snapshot { response })
            .is_none()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Cpu { response })
            .is_none()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Memory { response })
            .is_none()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Disks { response })
            .is_empty()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Networks { response })
            .is_empty()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Gpus { response })
            .is_empty()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Temperatures { response })
            .is_empty()
    );
    assert!(
        t.ask(|response| SystemResourcesQuery::Battery { response })
            .is_none()
    );

    let host = t.ask(|response| SystemResourcesQuery::HostInfo { response });
    assert_eq!(host.unwrap().hostname.as_deref(), Some("fake"));
    let thresholds = t.ask(|response| SystemResourcesQuery::GetThresholds { response });
    assert_eq!(thresholds, Some(Thresholds::default()));
    let mask = t.ask(|response| SystemResourcesQuery::GetRefreshMask { response });
    assert_eq!(mask, Some(RefreshMask::default()));
}

#[test]
fn queries_after_a_tick_report_its_readings() {
    let mut t = tracker();
    t.source.live.cpu = 42.0;
    t.source.battery = Some((BatteryState::Full, 100.0));
    t.handle_tick();
    t.source.live.cpu = 43.0; // not refreshed yet

    let cpu = t.ask(|response| SystemResourcesQuery::Cpu { response });
    assert_eq!(cpu.unwrap().usage_percent, 42.0);
    let memory = t.ask(|response| SystemResourcesQuery::Memory { response });
    assert_eq!(memory.unwrap().used_percent, 10.0);
    let disks = t.ask(|response| SystemResourcesQuery::Disks { response });
    assert_eq!(disks.len(), 1);
    let networks = t.ask(|response| SystemResourcesQuery::Networks { response });
    assert_eq!(networks.len(), 1);
    let gpus = t.ask(|response| SystemResourcesQuery::Gpus { response });
    assert_eq!(gpus.len(), 1);
    let temperatures = t.ask(|response| SystemResourcesQuery::Temperatures { response });
    assert_eq!(temperatures.len(), 1);
    let battery = t.ask(|response| SystemResourcesQuery::Battery { response });
    assert_eq!(battery.unwrap().charge_percent, 100.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// commands and polling
// ─────────────────────────────────────────────────────────────────────────────

fn set_interval(t: &mut SystemResources<Fake>, interval: Duration) -> Result<()> {
    t.run(|user, response| SystemResourcesCommand::SetPollInterval {
        user,
        interval,
        response,
    })
}

#[test]
fn the_refresh_mask_command_stores_the_mask_and_reports_it() {
    let (mut t, mut rx) = running();

    t.run(|user, response| SystemResourcesCommand::SetRefreshMask {
        user,
        mask: nothing(),
        response,
    })
    .unwrap();

    assert_eq!(t.refresh_mask, nothing());
    assert_eq!(drain(&mut rx), ["command set_refresh_mask true"]);
}

#[tokio::test]
async fn poll_commands_change_the_poll_and_report_it() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();
    assert!(t.poll_status().paused, "paused until the loop starts");

    t.run(|user, response| SystemResourcesCommand::ResumePoll { user, response })
        .unwrap();
    assert!(!t.poll_status().paused);

    set_interval(&mut t, Duration::from_millis(250)).unwrap();
    let status = t.poll_status();
    assert_eq!(status.interval, 250);
    assert!(!status.paused);

    t.run(|user, response| SystemResourcesCommand::PausePoll { user, response })
        .unwrap();
    assert!(t.poll_status().paused);

    assert_eq!(
        drain(&mut rx),
        [
            "command resume_poll true",
            "command set_poll_interval true",
            "command pause_poll true",
        ]
    );
}

#[tokio::test]
async fn setting_the_interval_while_paused_stays_paused() {
    let mut t = tracker();

    set_interval(&mut t, Duration::from_millis(250)).unwrap();

    let status = t.poll_status();
    assert_eq!(status.interval, 250);
    assert!(status.paused);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_running() {
    let mut t = tracker();
    t.poll.resume();

    assert!(set_interval(&mut t, Duration::ZERO).is_err());
    assert_eq!(t.poll_status().interval, 1000);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_paused() {
    let mut t = tracker();

    assert!(set_interval(&mut t, Duration::ZERO).is_err());
    assert_eq!(t.poll_status().interval, 1000);
}

// ─────────────────────────────────────────────────────────────────────────────
// the running loop
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_loop_ticks_and_answers_queries_and_commands() {
    let t = tracker();
    let query_tx = t.channels.query_tx.clone();
    let command_tx = t.channels.command_tx.clone();
    let mut rx = t.channels.event_tx.subscribe();
    let task = tokio::spawn(t.start_resource_loop());
    let wait = Duration::from_secs(5);

    let first = tokio::time::timeout(wait, rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        first,
        SystemResourcesEvent::InitialSnapshot { .. }
    ));

    let (response, answer) = oneshot::channel();
    query_tx
        .send(SystemResourcesQuery::Cpu { response })
        .await
        .unwrap();
    let cpu = tokio::time::timeout(wait, answer).await.unwrap().unwrap();
    assert_eq!(cpu.unwrap().usage_percent, 10.0);

    let (response, answer) = oneshot::channel();
    command_tx
        .send(SystemResourcesCommand::PausePoll {
            user: user(),
            response,
        })
        .await
        .unwrap();
    tokio::time::timeout(wait, answer)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    task.abort();
}

// ─────────────────────────────────────────────────────────────────────────────
// event payloads
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn events_convert_to_their_public_names_and_data() {
    let cases = [
        (
            SystemResourcesEvent::CpuThresholdExceeded {
                usage_percent: 95.0,
                threshold: 90.0,
            },
            "resources.cpu_threshold_exceeded",
            serde_json::json!({ "usage_percent": 95.0, "threshold": 90.0 }),
        ),
        (
            // Pinned: the field is `used_percent`, the public key is `usage_percent`.
            SystemResourcesEvent::MemoryThresholdExceeded {
                used_percent: 95.0,
                threshold: 90.0,
            },
            "resources.memory_threshold_exceeded",
            serde_json::json!({ "usage_percent": 95.0, "threshold": 90.0 }),
        ),
        (
            SystemResourcesEvent::DiskThresholdExceeded {
                mount_point: "/".into(),
                used_percent: 95.0,
                threshold: 90.0,
            },
            "resources.disk_threshold_exceeded",
            serde_json::json!({ "mount_point": "/", "usage_percent": 95.0, "threshold": 90.0 }),
        ),
        (
            SystemResourcesEvent::BatteryLow {
                charge_percent: 5.0,
                threshold: 15.0,
            },
            "resources.battery_low",
            serde_json::json!({ "charge_percent": 5.0, "threshold": 15.0 }),
        ),
        (
            SystemResourcesEvent::BatteryStateChanged {
                state: BatteryState::Full,
            },
            "resources.battery_state_changed",
            serde_json::json!({ "state": "Full" }),
        ),
    ];

    for (event, name, data) in cases {
        let payload = EventPayload::from(&event);
        assert_eq!(payload.event, name);
        assert_eq!(payload.data, data, "{name}");
    }
}

#[test]
fn snapshot_events_carry_the_snapshot() {
    let mut t = tracker();
    t.handle_tick();
    let snapshot = t.snapshot();

    let initial = EventPayload::from(&SystemResourcesEvent::InitialSnapshot {
        snapshot: snapshot.clone(),
    });
    let tick = EventPayload::from(&SystemResourcesEvent::Tick { snapshot });

    assert_eq!(initial.event, "resources.initial_snapshot");
    assert_eq!(tick.event, "resources.tick");
    assert_eq!(tick.data["snapshot"]["cpu"]["usage_percent"], 10.0);
}

#[test]
fn command_events_carry_the_action_and_the_error() {
    let event = SystemResourcesEvent::CommandExecuted {
        user: user(),
        action: SystemResourcesCommandAction::PausePoll,
        success: false,
        error: Some("nope".into()),
    };

    let payload = EventPayload::from(&event);

    assert_eq!(payload.event, "resources.command_executed");
    assert_eq!(payload.data["action"], "pause_poll");
    assert_eq!(payload.data["success"], false);
    assert_eq!(payload.data["error"], "nope");
}

// ─────────────────────────────────────────────────────────────────────────────
// the real source
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sysinfo_source_reads_this_machine() {
    let mut source = SysinfoSource::new();

    source.refresh(&RefreshMask::default());

    assert!(!source.cpu().cores.is_empty());
    let memory = source.memory();
    assert!(memory.total_bytes > 0);
    assert!((0.0..=100.0).contains(&memory.used_percent));
    assert_eq!(
        memory.swap_total_bytes > 0,
        memory.swap_used_percent.is_some()
    );
    assert!(source.host().cpu_arch.is_some());
    // Machine-dependent lists: these only have to not panic.
    let _ = (source.disks(), source.networks(), source.temperatures());
    let _ = (source.gpus(), source.battery());
}
