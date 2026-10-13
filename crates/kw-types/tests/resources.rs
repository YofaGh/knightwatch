use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, from_value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kw_types::api::{SetRefreshMaskRequest, SetThresholdsRequest};
use kw_types::resources::*;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Serialize -> deserialize -> serialize again; both JSON values must match.
fn assert_roundtrip<T: Serialize + DeserializeOwned>(v: &T) {
    let first = serde_json::to_value(v).unwrap();
    let back: T = serde_json::from_value(first.clone()).unwrap();
    let second = serde_json::to_value(&back).unwrap();
    assert_eq!(first, second);
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .expect("expected a JSON object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

const GIB: u64 = 1024 * 1024 * 1024;

// ---- builders --------------------------------------------------------------

fn cpu_core(name: &str, usage: f32, mhz: u64) -> CpuCoreSnapshot {
    CpuCoreSnapshot {
        name: name.into(),
        usage_percent: usage,
        frequency_mhz: mhz,
    }
}

fn cpu() -> CpuSnapshot {
    CpuSnapshot {
        usage_percent: 12.0,
        cores: vec![cpu_core("cpu0", 5.0, 3500)],
        frequency_mhz: 3600,
        brand: "Test CPU".into(),
        physical_core_count: Some(8),
        load_avg: None,
    }
}

fn memory() -> MemorySnapshot {
    MemorySnapshot {
        total_bytes: 16 * GIB,
        used_bytes: 8 * GIB,
        available_bytes: 8 * GIB,
        free_bytes: 4 * GIB,
        used_percent: 50.0,
        swap_total_bytes: 2 * GIB,
        swap_used_bytes: GIB / 2,
        swap_free_bytes: 3 * GIB / 2,
        swap_used_percent: Some(25.0),
    }
}

fn disk() -> DiskSnapshot {
    DiskSnapshot {
        name: "/dev/sda1".into(),
        mount_point: "/".into(),
        file_system: "ext4".into(),
        kind: DiskKind::Ssd,
        is_removable: false,
        total_bytes: 100 * GIB,
        used_bytes: 50 * GIB,
        available_bytes: 50 * GIB,
        used_percent: 50.0,
    }
}

fn network() -> NetworkSnapshot {
    NetworkSnapshot {
        interface: "eth0".into(),
        rx_bytes_per_sec: 1000,
        tx_bytes_per_sec: 2000,
        rx_total_bytes: 10_000,
        tx_total_bytes: 20_000,
        rx_packets_per_sec: 10,
        tx_packets_per_sec: 20,
        rx_errors: 0,
        tx_errors: 1,
    }
}

fn gpu_empty() -> GpuSnapshot {
    GpuSnapshot {
        name: "Test GPU".into(),
        usage_percent: None,
        vram_used_bytes: None,
        vram_total_bytes: None,
        vram_used_percent: None,
        temperature_celsius: None,
        power_draw_watts: None,
        power_limit_watts: None,
        fan_speed_percent: vec![],
    }
}

fn gpu_full() -> GpuSnapshot {
    GpuSnapshot {
        name: "Test GPU".into(),
        usage_percent: Some(55.0),
        vram_used_bytes: Some(GIB),
        vram_total_bytes: Some(8 * GIB),
        vram_used_percent: Some(12.5),
        temperature_celsius: Some(65.4),
        power_draw_watts: Some(120.0),
        power_limit_watts: Some(450.0),
        fan_speed_percent: vec![30.0, 31.0],
    }
}

fn battery(state: BatteryState) -> BatterySnapshot {
    BatterySnapshot {
        charge_percent: 87.0,
        state,
        time_to_empty_secs: None,
        time_to_full_secs: None,
        power_draw_watts: Some(12.5),
        cycle_count: Some(300),
        health_percent: None,
    }
}

fn thermal() -> ThermalSnapshot {
    ThermalSnapshot {
        label: "coretemp".into(),
        temperature_celsius: Some(45.0),
        temperature_max_celsius: Some(90.0),
        temperature_critical_celsius: None,
    }
}

fn host() -> HostInfo {
    HostInfo {
        hostname: Some("box".into()),
        os_name: Some("Ubuntu".into()),
        kernel_version: Some("6.8".into()),
        cpu_arch: Some("x86_64".into()),
        uptime_secs: 3600,
        process_count: 250,
    }
}

fn minimal_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp: "2026-01-01T00:00:00Z".into(),
        cpu: cpu(),
        memory: memory(),
        disks: vec![],
        networks: vec![],
        gpus: vec![],
        battery: None,
        temperatures: vec![],
        host: host(),
        health: SystemHealth::Healthy,
    }
}

fn full_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        disks: vec![disk()],
        networks: vec![network()],
        gpus: vec![gpu_full()],
        battery: Some(battery(BatteryState::Discharging)),
        temperatures: vec![thermal()],
        health: SystemHealth::Warning,
        ..minimal_snapshot()
    }
}

// ---------------------------------------------------------------------------
// enums: SystemHealth, BatteryState, DiskKind
// ---------------------------------------------------------------------------

mod enums {
    use super::*;

    #[test]
    fn system_health_display_and_serde() {
        let cases = [
            (SystemHealth::Healthy, "healthy", "Healthy"),
            (SystemHealth::Warning, "warning", "Warning"),
            (SystemHealth::Critical, "critical", "Critical"),
        ];
        for (v, display, wire) in cases {
            assert_eq!(v.to_string(), display);
            assert_eq!(serde_json::to_value(&v).unwrap(), json!(wire));
            let back: SystemHealth = from_value(json!(wire)).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn battery_state_display_and_serde() {
        let cases = [
            (BatteryState::Charging, "charging", "Charging"),
            (BatteryState::Discharging, "discharging", "Discharging"),
            (BatteryState::Full, "full", "Full"),
            (BatteryState::Unknown, "unknown", "Unknown"),
        ];
        for (v, display, wire) in cases {
            assert_eq!(v.to_string(), display);
            assert_eq!(serde_json::to_value(&v).unwrap(), json!(wire));
            let back: BatteryState = from_value(json!(wire)).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn disk_kind_display_and_serde() {
        let cases = [
            (DiskKind::Ssd, "SSD", "Ssd"),
            (DiskKind::Hdd, "HDD", "Hdd"),
            (DiskKind::Unknown, "unknown", "Unknown"),
        ];
        for (v, display, wire) in cases {
            assert_eq!(v.to_string(), display);
            assert_eq!(serde_json::to_value(&v).unwrap(), json!(wire));
            let back: DiskKind = from_value(json!(wire)).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn serde_names_are_capitalised_unlike_display() {
        // Display is lowercase ("healthy") but the wire format is "Healthy".
        // Clients (dashboard, scripts) must send/expect the capitalised form.
        assert!(from_value::<SystemHealth>(json!("healthy")).is_err());
        assert!(from_value::<BatteryState>(json!("charging")).is_err());
        assert!(from_value::<DiskKind>(json!("SSD")).is_err());
        assert!(from_value::<DiskKind>(json!("ssd")).is_err());
    }
}

// ---------------------------------------------------------------------------
// Thresholds & RefreshMask
// ---------------------------------------------------------------------------

mod settings {
    use super::*;

    #[test]
    fn thresholds_default_values() {
        assert_eq!(
            Thresholds::default(),
            Thresholds {
                cpu_warn: 90.0,
                memory_warn: 90.0,
                disk_warn: 90.0,
                battery_low: 15.0,
            }
        );
    }

    #[test]
    fn thresholds_display() {
        assert_eq!(
            Thresholds::default().to_string(),
            "cpu_warn=90.0%, memory_warn=90.0%, disk_warn=90.0%, battery_low=15.0%"
        );
        let t = Thresholds {
            cpu_warn: 75.5,
            memory_warn: 80.0,
            disk_warn: 95.0,
            battery_low: 20.0,
        };
        assert_eq!(
            t.to_string(),
            "cpu_warn=75.5%, memory_warn=80.0%, disk_warn=95.0%, battery_low=20.0%"
        );
    }

    #[test]
    fn thresholds_json_matches_the_set_thresholds_request() {
        // The API request is converted into `Thresholds`; the two shapes must
        // stay interchangeable.
        let v = serde_json::to_value(Thresholds::default()).unwrap();
        assert_eq!(
            sorted_keys(&v),
            ["battery_low", "cpu_warn", "disk_warn", "memory_warn"]
        );
        let req: SetThresholdsRequest = from_value(v).unwrap();
        assert_eq!(req.cpu_warn, 90.0);
        assert_eq!(req.battery_low, 15.0);

        let back: Thresholds = from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(back, Thresholds::default());
    }

    #[test]
    fn refresh_mask_default_is_everything_on() {
        let m = RefreshMask::default();
        assert!(m.cpu && m.memory && m.disks && m.networks && m.temperatures && m.gpus);
    }

    #[test]
    fn refresh_mask_display() {
        assert_eq!(
            RefreshMask::default().to_string(),
            "cpu, memory, disks, networks, temperatures, gpus"
        );

        let none = RefreshMask {
            cpu: false,
            memory: false,
            disks: false,
            networks: false,
            temperatures: false,
            gpus: false,
        };
        assert_eq!(none.to_string(), "none");

        let some = RefreshMask {
            gpus: true,
            cpu: true,
            ..none.clone()
        };
        // fixed order regardless of how the struct was built
        assert_eq!(some.to_string(), "cpu, gpus");

        let one = RefreshMask {
            networks: true,
            ..none
        };
        assert_eq!(one.to_string(), "networks");
    }

    #[test]
    fn refresh_mask_json_matches_the_set_refresh_mask_request() {
        let v = serde_json::to_value(RefreshMask::default()).unwrap();
        assert_eq!(
            sorted_keys(&v),
            ["cpu", "disks", "gpus", "memory", "networks", "temperatures"]
        );
        let req: SetRefreshMaskRequest = from_value(v).unwrap();
        assert!(req.cpu && req.memory && req.disks && req.networks && req.temperatures && req.gpus);

        let back: RefreshMask = from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(back, RefreshMask::default());
    }

    #[test]
    fn settings_roundtrip() {
        assert_roundtrip(&Thresholds::default());
        assert_roundtrip(&RefreshMask::default());
    }
}

// ---------------------------------------------------------------------------
// Alarms
// ---------------------------------------------------------------------------

mod alarms {
    use super::*;

    fn clear() -> AlarmStatus {
        AlarmStatus {
            active: false,
            since: None,
        }
    }

    fn active() -> AlarmStatus {
        AlarmStatus {
            active: true,
            since: None,
        }
    }

    fn snapshot() -> AlarmSnapshot {
        AlarmSnapshot {
            cpu: clear(),
            memory: clear(),
            disks: vec![("/".into(), clear()), ("/home".into(), clear())],
            battery_low: clear(),
        }
    }

    // ---- AlarmStatus -------------------------------------------------------

    #[test]
    fn status_display_clear() {
        assert_eq!(clear().to_string(), "clear");
    }

    #[test]
    fn status_display_clear_ignores_since() {
        let s = AlarmStatus {
            active: false,
            since: Some(SystemTime::now() - Duration::from_secs(60)),
        };
        assert_eq!(s.to_string(), "clear");
    }

    #[test]
    fn status_display_active_without_since() {
        assert_eq!(active().to_string(), "active");
    }

    #[test]
    fn status_display_active_with_past_since_shows_duration() {
        let s = AlarmStatus {
            active: true,
            since: Some(SystemTime::now() - Duration::from_secs(5)),
        };
        let out = s.to_string();
        assert!(out.starts_with("active (for "), "got: {out}");
        assert!(out.ends_with(')'), "got: {out}");
    }

    #[test]
    fn status_display_active_with_future_since_falls_back_to_plain_active() {
        // clock skew / clock set backwards: elapsed() fails, we must not panic
        let s = AlarmStatus {
            active: true,
            since: Some(SystemTime::now() + Duration::from_secs(3600)),
        };
        assert_eq!(s.to_string(), "active");
    }

    #[test]
    fn status_json_shape() {
        // Clients (e.g. the Svelte dashboard) have to parse SystemTime's serde
        // shape, which is a struct of secs + nanos since the epoch.
        let s = AlarmStatus {
            active: true,
            since: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            json!({
                "active": true,
                "since": { "secs_since_epoch": 1_700_000_000_u64, "nanos_since_epoch": 0 }
            })
        );
        assert_eq!(
            serde_json::to_value(clear()).unwrap(),
            json!({ "active": false, "since": null })
        );
    }

    #[test]
    fn status_roundtrip_is_exact() {
        let s = AlarmStatus {
            active: true,
            since: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        };
        let back: AlarmStatus = from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let back: AlarmStatus = from_value(serde_json::to_value(clear()).unwrap()).unwrap();
        assert_eq!(back, clear());
    }

    // ---- AlarmSnapshot -----------------------------------------------------

    #[test]
    fn snapshot_display_when_nothing_is_active() {
        assert_eq!(snapshot().to_string(), "no active alarms");
    }

    #[test]
    fn snapshot_display_when_nothing_is_active_even_with_no_disks() {
        let mut s = snapshot();
        s.disks.clear();
        assert_eq!(s.to_string(), "no active alarms");
    }

    #[test]
    fn snapshot_display_lists_only_active_alarms() {
        let mut s = snapshot();
        s.memory = active();
        assert_eq!(s.to_string(), "memory: active");

        let mut s = snapshot();
        s.disks[1].1 = active();
        assert_eq!(s.to_string(), "disk[/home]: active");
    }

    #[test]
    fn snapshot_display_order_is_cpu_memory_battery_then_disks() {
        let s = AlarmSnapshot {
            cpu: active(),
            memory: active(),
            disks: vec![("/".into(), active()), ("/home".into(), active())],
            battery_low: active(),
        };
        assert_eq!(
            s.to_string(),
            "cpu: active, memory: active, battery: active, disk[/]: active, disk[/home]: active"
        );
    }

    #[test]
    fn snapshot_display_keeps_disk_order_as_given() {
        let s = AlarmSnapshot {
            cpu: clear(),
            memory: clear(),
            disks: vec![("/z".into(), active()), ("/a".into(), active())],
            battery_low: clear(),
        };
        assert_eq!(s.to_string(), "disk[/z]: active, disk[/a]: active");
    }

    #[test]
    fn snapshot_json_shape_disks_are_pairs_not_a_map() {
        let v = serde_json::to_value(snapshot()).unwrap();
        assert_eq!(sorted_keys(&v), ["battery_low", "cpu", "disks", "memory"]);
        assert_eq!(
            v["disks"],
            json!([
                ["/", { "active": false, "since": null }],
                ["/home", { "active": false, "since": null }]
            ])
        );
    }

    #[test]
    fn snapshot_roundtrip_is_exact() {
        let mut s = snapshot();
        s.cpu = AlarmStatus {
            active: true,
            since: Some(UNIX_EPOCH + Duration::from_secs(42)),
        };
        let back: AlarmSnapshot = from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}

// ---------------------------------------------------------------------------
// Display of the individual snapshot types
// ---------------------------------------------------------------------------

mod display {
    use super::*;

    // ---- CPU ---------------------------------------------------------------

    #[test]
    fn cpu_display_exact() {
        let expected = format!(
            "CPU: Test CPU  12.0%  3600 MHz  (8 physical cores)\n  cpu0{}5.0%  3500 MHz\n",
            " ".repeat(7)
        );
        assert_eq!(cpu().to_string(), expected);
    }

    #[test]
    fn cpu_display_unknown_physical_core_count() {
        let mut c = cpu();
        c.physical_core_count = None;
        assert!(
            c.to_string()
                .starts_with("CPU: Test CPU  12.0%  3600 MHz  (? physical cores)\n")
        );
    }

    #[test]
    fn cpu_display_one_line_per_core() {
        let mut c = cpu();
        c.cores = vec![
            cpu_core("cpu0", 1.0, 1),
            cpu_core("cpu1", 2.0, 2),
            cpu_core("cpu2", 3.0, 3),
        ];
        assert_eq!(c.to_string().lines().count(), 4); // header + 3 cores
        c.cores.clear();
        assert_eq!(c.to_string().lines().count(), 1);
    }

    // ---- memory ------------------------------------------------------------

    #[test]
    fn memory_display_with_swap() {
        let out = memory().to_string();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "got:\n{out}");
        assert!(
            lines[0].starts_with("RAM:  50.0% used  "),
            "got: {}",
            lines[0]
        );
        assert!(lines[0].contains("(free: "), "got: {}", lines[0]);
        assert!(
            lines[1].starts_with("Swap: 25% used  "),
            "got: {}",
            lines[1]
        );
    }

    #[test]
    fn memory_display_without_swap() {
        let mut m = memory();
        m.swap_total_bytes = 0;
        m.swap_used_bytes = 0;
        m.swap_free_bytes = 0;
        m.swap_used_percent = None;
        let out = m.to_string();
        assert_eq!(out.lines().nth(1), Some("Swap: none"));
    }

    #[test]
    fn memory_display_swap_configured_but_percent_unknown() {
        let mut m = memory();
        m.swap_used_percent = None;
        let out = m.to_string();
        assert!(
            out.lines().nth(1).unwrap().starts_with("Swap: 0% used  "),
            "got:\n{out}"
        );
    }

    // ---- disk --------------------------------------------------------------

    #[test]
    fn disk_display() {
        let out = disk().to_string();
        assert!(out.starts_with("/ (ext4)  SSD  50.0% used  "), "got: {out}");
        assert!(out.contains("  avail: "), "got: {out}");
        assert!(!out.contains("[removable]"), "got: {out}");
    }

    #[test]
    fn disk_display_removable() {
        let mut d = disk();
        d.is_removable = true;
        d.kind = DiskKind::Unknown;
        let out = d.to_string();
        assert!(out.starts_with("/ (ext4)  unknown  "), "got: {out}");
        assert!(out.ends_with("  [removable]"), "got: {out}");
    }

    // ---- network -----------------------------------------------------------

    #[test]
    fn network_display() {
        let out = network().to_string();
        // interface name is padded to 12 columns, then two spaces
        assert!(
            out.starts_with(&format!("eth0{}↓ ", " ".repeat(10))),
            "got: {out}"
        );
        assert!(out.contains("/s  ↑ "), "got: {out}");
        assert!(out.contains("  total ↓ "), "got: {out}");
    }

    // ---- gpu ---------------------------------------------------------------

    #[test]
    fn gpu_display_with_no_metrics_is_just_the_name() {
        assert_eq!(gpu_empty().to_string(), "Test GPU");
    }

    #[test]
    fn gpu_display_with_all_metrics() {
        let out = gpu_full().to_string();
        assert!(out.starts_with("Test GPU  gpu=55.0%  vram="), "got: {out}");
        assert!(out.ends_with("  temp=65°C  power=120W"), "got: {out}");
    }

    #[test]
    fn gpu_display_vram_needs_both_used_and_total() {
        let mut g = gpu_empty();
        g.vram_used_bytes = Some(GIB);
        assert!(!g.to_string().contains("vram="));

        let mut g = gpu_empty();
        g.vram_total_bytes = Some(8 * GIB);
        assert!(!g.to_string().contains("vram="));

        let mut g = gpu_empty();
        g.vram_used_bytes = Some(GIB);
        g.vram_total_bytes = Some(8 * GIB);
        assert!(g.to_string().contains("  vram="));
    }

    #[test]
    fn gpu_display_individual_metrics_are_independent() {
        let mut g = gpu_empty();
        g.temperature_celsius = Some(70.0);
        assert_eq!(g.to_string(), "Test GPU  temp=70°C");

        let mut g = gpu_empty();
        g.power_draw_watts = Some(99.0);
        assert_eq!(g.to_string(), "Test GPU  power=99W");
    }

    // ---- battery -----------------------------------------------------------

    #[test]
    fn battery_display_discharging_unknown_time() {
        assert_eq!(
            battery(BatteryState::Discharging).to_string(),
            "87%  discharging  remaining: N/A"
        );
    }

    #[test]
    fn battery_display_charging_unknown_time() {
        let mut b = battery(BatteryState::Charging);
        b.charge_percent = 50.0;
        assert_eq!(b.to_string(), "50%  charging  full in: N/A");
    }

    #[test]
    fn battery_display_known_times() {
        let mut b = battery(BatteryState::Discharging);
        b.time_to_empty_secs = Some(3600);
        let out = b.to_string();
        assert!(
            out.starts_with("87%  discharging  remaining: "),
            "got: {out}"
        );
        assert!(!out.contains("N/A"), "got: {out}");

        let mut b = battery(BatteryState::Charging);
        b.time_to_full_secs = Some(1800);
        let out = b.to_string();
        assert!(out.starts_with("87%  charging  full in: "), "got: {out}");
        assert!(!out.contains("N/A"), "got: {out}");
    }

    #[test]
    fn battery_display_full_and_unknown_have_no_time_suffix() {
        let mut b = battery(BatteryState::Full);
        b.charge_percent = 100.0;
        assert_eq!(b.to_string(), "100%  full");

        let mut b = battery(BatteryState::Unknown);
        b.charge_percent = 10.0;
        assert_eq!(b.to_string(), "10%  unknown");
    }

    #[test]
    fn battery_display_health_suffix() {
        let mut b = battery(BatteryState::Full);
        b.charge_percent = 100.0;
        b.health_percent = Some(95.0);
        assert_eq!(b.to_string(), "100%  full  health=95%");
    }

    #[test]
    fn battery_display_ignores_the_wrong_time_field_for_the_state() {
        // Discharging only looks at time_to_empty, Charging only at time_to_full.
        let mut b = battery(BatteryState::Discharging);
        b.time_to_full_secs = Some(100);
        assert_eq!(b.to_string(), "87%  discharging  remaining: N/A");
    }

    // ---- thermal -----------------------------------------------------------

    #[test]
    fn thermal_display() {
        let out = thermal().to_string();
        assert!(out.starts_with("coretemp"), "got: {out}");
        assert!(out.ends_with("  45°C (max: 90°C crit: ?)"), "got: {out}");
        // label column is 40 wide
        assert_eq!(
            out.chars().take(40).collect::<String>().trim_end(),
            "coretemp"
        );
    }

    #[test]
    fn thermal_display_unknown_values() {
        let t = ThermalSnapshot {
            label: "acpitz".into(),
            temperature_celsius: None,
            temperature_max_celsius: None,
            temperature_critical_celsius: Some(105.0),
        };
        assert!(t.to_string().ends_with("  N/A (max: ? crit: 105°C)"));
    }

    // ---- host --------------------------------------------------------------

    #[test]
    fn host_display_with_values() {
        let out = host().to_string();
        assert!(
            out.starts_with("hostname: box  os: Ubuntu  kernel: 6.8  arch: x86_64  uptime: "),
            "got: {out}"
        );
        assert!(out.ends_with("  processes: 250\n"), "got: {out}");
    }

    #[test]
    fn host_display_with_unknowns() {
        let h = HostInfo {
            hostname: None,
            os_name: None,
            kernel_version: None,
            cpu_arch: None,
            uptime_secs: 0,
            process_count: 3,
        };
        let out = h.to_string();
        assert!(
            out.starts_with("hostname: ?  os: ?  kernel: ?  arch: ?  uptime: "),
            "got: {out}"
        );
        assert!(out.ends_with("  processes: 3\n"), "got: {out}");
    }

    // ---- whole snapshot ----------------------------------------------------

    #[test]
    fn system_snapshot_display_header() {
        let out = minimal_snapshot().to_string();
        assert!(
            out.starts_with("=== System Snapshot  2026-01-01T00:00:00Z  health=healthy ===\n"),
            "got:\n{out}"
        );
        assert!(out.contains("hostname: box"));
        assert!(out.contains("CPU: Test CPU"));
        assert!(out.contains("RAM:  50.0% used"));
    }

    #[test]
    fn system_snapshot_display_omits_empty_sections() {
        let out = minimal_snapshot().to_string();
        for header in ["Disks:", "Networks:", "GPUs:", "Battery:", "Temperatures:"] {
            assert!(
                !out.contains(header),
                "'{header}' should be omitted:\n{out}"
            );
        }
    }

    #[test]
    fn system_snapshot_display_includes_all_sections_in_order() {
        let out = full_snapshot().to_string();
        assert!(out.contains("health=warning"), "got:\n{out}");

        let positions: Vec<usize> = ["Disks:", "Networks:", "GPUs:", "Battery:", "Temperatures:"]
            .iter()
            .map(|h| {
                out.find(h)
                    .unwrap_or_else(|| panic!("missing '{h}' in:\n{out}"))
            })
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "sections out of order: {positions:?}"
        );
    }

    #[test]
    fn system_snapshot_display_indents_list_entries() {
        let out = full_snapshot().to_string();
        assert!(out.contains("\n  / (ext4)  SSD"), "got:\n{out}");
        assert!(out.contains("\n  eth0"), "got:\n{out}");
        assert!(out.contains("\n  Test GPU"), "got:\n{out}");
        assert!(out.contains("\nBattery: 87%  discharging"), "got:\n{out}");
        assert!(out.contains("\n  coretemp"), "got:\n{out}");
    }
}

// ---------------------------------------------------------------------------
// serde
// ---------------------------------------------------------------------------

mod serde_shapes {
    use super::*;

    #[test]
    fn every_snapshot_type_roundtrips() {
        assert_roundtrip(&cpu());
        assert_roundtrip(&cpu_core("cpu0", 5.0, 3500));
        assert_roundtrip(&memory());
        assert_roundtrip(&disk());
        assert_roundtrip(&network());
        assert_roundtrip(&gpu_empty());
        assert_roundtrip(&gpu_full());
        assert_roundtrip(&battery(BatteryState::Charging));
        assert_roundtrip(&thermal());
        assert_roundtrip(&host());
        assert_roundtrip(&LoadAverage {
            one: 0.5,
            five: 1.25,
            fifteen: 2.0,
        });
    }

    #[test]
    fn system_snapshot_roundtrips_minimal_and_full() {
        assert_roundtrip(&minimal_snapshot());
        assert_roundtrip(&full_snapshot());
    }

    #[test]
    fn system_snapshot_json_has_the_ten_top_level_keys() {
        let v = serde_json::to_value(minimal_snapshot()).unwrap();
        assert_eq!(
            sorted_keys(&v),
            [
                "battery",
                "cpu",
                "disks",
                "gpus",
                "health",
                "host",
                "memory",
                "networks",
                "temperatures",
                "timestamp",
            ]
        );
        assert_eq!(v["health"], json!("Healthy"));
        // `battery` has no skip_serializing_if: a missing battery is an explicit null
        assert_eq!(v["battery"], Value::Null);
        assert_eq!(v["disks"], json!([]));
    }

    #[test]
    fn cpu_load_avg_is_omitted_when_none_and_present_when_some() {
        let v = serde_json::to_value(cpu()).unwrap();
        assert!(!v.as_object().unwrap().contains_key("load_avg"));

        let mut c = cpu();
        c.load_avg = Some(LoadAverage {
            one: 0.5,
            five: 1.0,
            fifteen: 1.5,
        });
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(
            v["load_avg"],
            json!({ "one": 0.5, "five": 1.0, "fifteen": 1.5 })
        );
        assert_roundtrip(&c);
    }

    #[test]
    fn cpu_deserializes_without_load_avg() {
        // e.g. a payload from a Windows host
        let v = json!({
            "usage_percent": 10.0,
            "cores": [],
            "frequency_mhz": 3000,
            "brand": "X",
            "physical_core_count": null
        });
        let c: CpuSnapshot = from_value(v).unwrap();
        assert!(c.load_avg.is_none());
        assert!(c.physical_core_count.is_none());
    }

    #[test]
    fn optional_gpu_metrics_serialize_as_null() {
        let v = serde_json::to_value(gpu_empty()).unwrap();
        for key in [
            "usage_percent",
            "vram_used_bytes",
            "vram_total_bytes",
            "vram_used_percent",
            "temperature_celsius",
            "power_draw_watts",
            "power_limit_watts",
        ] {
            assert_eq!(v[key], Value::Null, "{key}");
        }
        assert_eq!(v["fan_speed_percent"], json!([]));
    }

    #[test]
    fn optional_battery_and_thermal_fields_serialize_as_null() {
        let v = serde_json::to_value(battery(BatteryState::Unknown)).unwrap();
        for key in ["time_to_empty_secs", "time_to_full_secs", "health_percent"] {
            assert_eq!(v[key], Value::Null, "{key}");
        }

        let t = ThermalSnapshot {
            label: "x".into(),
            temperature_celsius: None,
            temperature_max_celsius: None,
            temperature_critical_celsius: None,
        };
        let v = serde_json::to_value(&t).unwrap();
        for key in [
            "temperature_celsius",
            "temperature_max_celsius",
            "temperature_critical_celsius",
        ] {
            assert_eq!(v[key], Value::Null, "{key}");
        }
        assert_roundtrip(&t);
    }

    #[test]
    fn host_info_roundtrips_with_unknown_fields() {
        let h = HostInfo {
            hostname: None,
            os_name: None,
            kernel_version: None,
            cpu_arch: None,
            uptime_secs: 0,
            process_count: 0,
        };
        assert_roundtrip(&h);
    }

    #[test]
    fn large_values_survive_json() {
        let mut d = disk();
        d.total_bytes = u64::MAX / 2; // well past f64's exact-integer range
        d.used_bytes = (1_u64 << 60) + 1;
        let back: DiskSnapshot = from_value(serde_json::to_value(&d).unwrap()).unwrap();
        assert_eq!(back.total_bytes, d.total_bytes);
        assert_eq!(back.used_bytes, d.used_bytes);
    }
}
