#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use kw_types::resources::{
    BatterySnapshot, BatteryState, CpuSnapshot, DiskKind, DiskSnapshot, MemorySnapshot,
    SystemHealth, Thresholds,
};

use super::utils::derive_health;

use SystemHealth::{Critical, Healthy, Warning};

/// A value that is healthy for every subsystem.
const OK: f32 = 10.0;

fn mk_cpu(usage_percent: f32) -> CpuSnapshot {
    CpuSnapshot {
        usage_percent,
        cores: vec![],
        frequency_mhz: 0,
        brand: "test cpu".into(),
        physical_core_count: None,
        load_avg: None,
    }
}

fn mk_memory(used_percent: f32) -> MemorySnapshot {
    MemorySnapshot {
        total_bytes: 0,
        used_bytes: 0,
        available_bytes: 0,
        free_bytes: 0,
        used_percent,
        swap_total_bytes: 0,
        swap_used_bytes: 0,
        swap_free_bytes: 0,
        swap_used_percent: None,
    }
}

fn mk_disk(used_percent: f32) -> DiskSnapshot {
    DiskSnapshot {
        name: "disk".into(),
        mount_point: "/".into(),
        file_system: "ext4".into(),
        kind: DiskKind::Ssd,
        is_removable: false,
        total_bytes: 0,
        used_bytes: 0,
        available_bytes: 0,
        used_percent,
    }
}

fn mk_battery(state: BatteryState, charge_percent: f32) -> BatterySnapshot {
    BatterySnapshot {
        charge_percent,
        state,
        time_to_empty_secs: None,
        time_to_full_secs: None,
        power_draw_watts: None,
        cycle_count: None,
        health_percent: None,
    }
}

/// Runs `derive_health` from plain numbers.
fn run(cpu: f32, memory: f32, disks: &[f32], battery: Option<(BatteryState, f32)>) -> SystemHealth {
    let cpu = mk_cpu(cpu);
    let memory = mk_memory(memory);
    let disks: Vec<DiskSnapshot> = disks.iter().map(|p| mk_disk(*p)).collect();
    let battery = battery.map(|(s, c)| mk_battery(s, c));
    derive_health(&cpu, &memory, &disks, battery.as_ref())
}

fn on_battery(charge: f32) -> Option<(BatteryState, f32)> {
    Some((BatteryState::Discharging, charge))
}

// ---------------------------------------------------------------------------
// Baseline
// ---------------------------------------------------------------------------

#[test]
fn everything_low_is_healthy() {
    assert_eq!(run(OK, OK, &[OK], on_battery(80.0)), Healthy);
}

#[test]
fn no_disks_and_no_battery_is_healthy() {
    assert_eq!(run(OK, OK, &[], None), Healthy);
}

#[test]
fn everything_at_zero_is_healthy() {
    assert_eq!(
        run(0.0, 0.0, &[0.0], Some((BatteryState::Full, 100.0))),
        Healthy
    );
}

// ---------------------------------------------------------------------------
// Thresholds, one subsystem at a time
// ---------------------------------------------------------------------------

#[test]
fn cpu_thresholds_are_80_warning_and_95_critical() {
    let table = [
        (0.0, Healthy),
        (79.99, Healthy),
        (80.0, Warning),
        (94.99, Warning),
        (95.0, Critical),
        (100.0, Critical),
        (250.0, Critical),
    ];
    for (value, expected) in table {
        assert_eq!(run(value, OK, &[OK], None), expected, "cpu {value}");
    }
}

#[test]
fn memory_thresholds_are_80_warning_and_95_critical() {
    let table = [
        (0.0, Healthy),
        (79.99, Healthy),
        (80.0, Warning),
        (94.99, Warning),
        (95.0, Critical),
        (100.0, Critical),
        (250.0, Critical),
    ];
    for (value, expected) in table {
        assert_eq!(run(OK, value, &[OK], None), expected, "memory {value}");
    }
}

#[test]
fn disk_thresholds_are_90_warning_and_95_critical() {
    let table = [
        (0.0, Healthy),
        (80.0, Healthy),
        (89.99, Healthy),
        (90.0, Warning),
        (94.99, Warning),
        (95.0, Critical),
        (100.0, Critical),
    ];
    for (value, expected) in table {
        assert_eq!(run(OK, OK, &[value], None), expected, "disk {value}");
    }
}

#[test]
fn discharging_battery_thresholds_are_20_warning_and_5_critical() {
    let table = [
        (100.0, Healthy),
        (50.0, Healthy),
        (20.01, Healthy),
        (20.0, Warning),
        (5.01, Warning),
        (5.0, Critical),
        (0.0, Critical),
    ];
    for (value, expected) in table {
        assert_eq!(
            run(OK, OK, &[OK], on_battery(value)),
            expected,
            "battery {value}"
        );
    }
}

#[test]
fn disk_warning_starts_later_than_cpu_and_memory_warning() {
    // Pinned asymmetry: 80 is a warning for cpu and memory but fine for a disk.
    assert_eq!(run(80.0, OK, &[OK], None), Warning);
    assert_eq!(run(OK, 80.0, &[OK], None), Warning);
    assert_eq!(run(OK, OK, &[80.0], None), Healthy);
}

// ---------------------------------------------------------------------------
// Battery state
// ---------------------------------------------------------------------------

#[test]
fn low_charge_only_matters_while_discharging() {
    for state in [
        BatteryState::Charging,
        BatteryState::Full,
        BatteryState::Unknown,
    ] {
        for charge in [0.0, 3.0, 15.0] {
            assert_eq!(
                run(OK, OK, &[OK], Some((state.clone(), charge))),
                Healthy,
                "{state} at {charge}%"
            );
        }
    }
}

#[test]
fn unknown_battery_state_with_a_nearly_empty_battery_is_healthy() {
    // Pinned: if the OS cannot tell the state, a 1% battery is not reported.
    assert_eq!(
        run(OK, OK, &[OK], Some((BatteryState::Unknown, 1.0))),
        Healthy
    );
}

#[test]
fn full_battery_never_raises_health() {
    assert_eq!(
        run(OK, OK, &[OK], Some((BatteryState::Full, 100.0))),
        Healthy
    );
}

#[test]
fn negative_charge_while_discharging_is_critical() {
    assert_eq!(run(OK, OK, &[OK], on_battery(-1.0)), Critical);
}

#[test]
fn other_battery_fields_do_not_matter() {
    let cpu = mk_cpu(OK);
    let memory = mk_memory(OK);
    let mut battery = mk_battery(BatteryState::Discharging, 50.0);
    battery.time_to_empty_secs = Some(1);
    battery.health_percent = Some(1.0);
    battery.cycle_count = Some(100_000);
    battery.power_draw_watts = Some(500.0);
    assert_eq!(derive_health(&cpu, &memory, &[], Some(&battery)), Healthy);
}

// ---------------------------------------------------------------------------
// Several disks
// ---------------------------------------------------------------------------

#[test]
fn one_bad_disk_among_many_decides_the_result() {
    assert_eq!(run(OK, OK, &[10.0, 20.0, 91.0, 30.0], None), Warning);
    assert_eq!(run(OK, OK, &[10.0, 96.0, 20.0], None), Critical);
}

#[test]
fn the_bad_disk_can_be_first_or_last() {
    assert_eq!(run(OK, OK, &[96.0, 10.0, 10.0], None), Critical);
    assert_eq!(run(OK, OK, &[10.0, 10.0, 96.0], None), Critical);
}

#[test]
fn several_warning_disks_are_still_only_a_warning() {
    assert_eq!(run(OK, OK, &[91.0, 92.0, 93.0], None), Warning);
}

#[test]
fn warning_disk_and_critical_disk_together_are_critical() {
    assert_eq!(run(OK, OK, &[91.0, 99.0], None), Critical);
}

// ---------------------------------------------------------------------------
// Combinations
// ---------------------------------------------------------------------------

#[test]
fn critical_beats_warning() {
    assert_eq!(run(85.0, 96.0, &[OK], None), Critical);
    assert_eq!(run(96.0, 85.0, &[OK], None), Critical);
    assert_eq!(run(85.0, OK, &[96.0], None), Critical);
    assert_eq!(
        run(OK, OK, &[91.0], Some((BatteryState::Discharging, 4.0))),
        Critical
    );
}

#[test]
fn several_warnings_together_are_still_a_warning() {
    assert_eq!(run(85.0, 85.0, &[91.0], on_battery(15.0)), Warning);
}

#[test]
fn every_subsystem_critical_is_critical() {
    assert_eq!(run(99.0, 99.0, &[99.0], on_battery(1.0)), Critical);
}

#[test]
fn each_subsystem_alone_can_cause_a_warning() {
    assert_eq!(run(85.0, OK, &[OK], on_battery(80.0)), Warning);
    assert_eq!(run(OK, 85.0, &[OK], on_battery(80.0)), Warning);
    assert_eq!(run(OK, OK, &[91.0], on_battery(80.0)), Warning);
    assert_eq!(run(OK, OK, &[OK], on_battery(15.0)), Warning);
}

#[test]
fn each_subsystem_alone_can_cause_critical() {
    assert_eq!(run(96.0, OK, &[OK], on_battery(80.0)), Critical);
    assert_eq!(run(OK, 96.0, &[OK], on_battery(80.0)), Critical);
    assert_eq!(run(OK, OK, &[96.0], on_battery(80.0)), Critical);
    assert_eq!(run(OK, OK, &[OK], on_battery(4.0)), Critical);
}

// ---------------------------------------------------------------------------
// Odd numbers (pinned)
// ---------------------------------------------------------------------------

#[test]
fn nan_readings_count_as_healthy() {
    // Pinned: NaN is false for every `>=` and `<=`, so a broken reading hides
    // problems. It should flip if NaN is ever treated as "unknown" or worse.
    assert_eq!(run(f32::NAN, OK, &[OK], None), Healthy);
    assert_eq!(run(OK, f32::NAN, &[OK], None), Healthy);
    assert_eq!(run(OK, OK, &[f32::NAN], None), Healthy);
    assert_eq!(run(OK, OK, &[OK], on_battery(f32::NAN)), Healthy);
}

#[test]
fn a_nan_does_not_hide_a_real_problem_elsewhere() {
    assert_eq!(run(f32::NAN, 99.0, &[OK], None), Critical);
    assert_eq!(run(f32::NAN, OK, &[f32::NAN, 91.0], None), Warning);
}

#[test]
fn infinities_and_negative_values() {
    assert_eq!(run(f32::INFINITY, OK, &[OK], None), Critical);
    assert_eq!(run(f32::NEG_INFINITY, OK, &[OK], None), Healthy);
    assert_eq!(run(-5.0, -5.0, &[-5.0], None), Healthy);
    assert_eq!(run(OK, OK, &[OK], on_battery(f32::NEG_INFINITY)), Critical);
    assert_eq!(run(OK, OK, &[OK], on_battery(f32::INFINITY)), Healthy);
}

// ---------------------------------------------------------------------------
// Thresholds type is not used (pinned)
// ---------------------------------------------------------------------------

#[test]
fn health_ignores_the_configurable_thresholds() {
    // Pinned (finding 37): `derive_health` has fixed limits (80/80/90/20 for
    // warnings), while `Thresholds::default()` says cpu_warn 90 and battery_low 15.
    // These two cases flip if the configured thresholds are wired in.
    let t = Thresholds::default();
    assert!(85.0 < t.cpu_warn);
    assert_eq!(run(85.0, OK, &[OK], None), Warning);
    assert!(18.0 > t.battery_low);
    assert_eq!(run(OK, OK, &[OK], on_battery(18.0)), Warning);
}

// ---------------------------------------------------------------------------
// Purity
// ---------------------------------------------------------------------------

#[test]
fn same_input_gives_the_same_result() {
    let cpu = mk_cpu(85.0);
    let memory = mk_memory(OK);
    let disks = vec![mk_disk(OK)];
    let battery = mk_battery(BatteryState::Discharging, 50.0);
    let first = derive_health(&cpu, &memory, &disks, Some(&battery));
    let second = derive_health(&cpu, &memory, &disks, Some(&battery));
    assert_eq!(first, second);
    assert_eq!(first, Warning);
}

#[test]
fn inputs_are_not_changed() {
    let cpu = mk_cpu(97.0);
    let memory = mk_memory(50.0);
    let disks = vec![mk_disk(91.0), mk_disk(10.0)];
    let battery = mk_battery(BatteryState::Discharging, 4.0);
    let before = (
        serde_json::to_string(&cpu).unwrap(),
        serde_json::to_string(&memory).unwrap(),
        serde_json::to_string(&disks).unwrap(),
        serde_json::to_string(&battery).unwrap(),
    );
    let _ = derive_health(&cpu, &memory, &disks, Some(&battery));
    let after = (
        serde_json::to_string(&cpu).unwrap(),
        serde_json::to_string(&memory).unwrap(),
        serde_json::to_string(&disks).unwrap(),
        serde_json::to_string(&battery).unwrap(),
    );
    assert_eq!(before, after);
}
