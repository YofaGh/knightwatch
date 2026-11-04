#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::time::{Duration, SystemTime};

use kw_types::{
    docker::ContainerStats,
    process::{FDType, FileDescriptorInfo, IOStats, ProcessState},
    resources::{AlarmStatus, DiskKind, LoadAverage},
    systemd::{UnitLoadState, UnitType},
};

use super::display::TelegramDisplay;
use crate::{
    docker_tracker::{ContainerHealth, ContainerSnapshot, ContainerStatus},
    process_tracker::{ProcessSnapshot, ProcessTree},
    system_resources::{
        AlarmSnapshot, BatterySnapshot, BatteryState, CpuSnapshot, DiskSnapshot, GpuSnapshot,
        HostInfo, MemorySnapshot, NetworkSnapshot, SystemHealth, SystemSnapshot, ThermalSnapshot,
    },
    systemd::{SystemdSnapshot, UnitActiveState, UnitSnapshot},
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

const HOSTILE: &str = "x`y*z_w\\[a](b).c-d!";
const TIMESTAMP: &str = "2024-05-01T10:00:00+00:00";
const TIMESTAMP_ESCAPED: &str = "2024\\-05\\-01T10:00:00\\+00:00";

const KIB: u64 = 1_024;
const GIB: u64 = 1_073_741_824;

// The same small MarkdownV2 checker as `utils_tests.rs`. Outside code spans only
// `*`, `_` and the backtick may appear unescaped, and the markers must pair up.
fn assert_valid_mdv2(s: &str) {
    let mut in_code = false;
    let mut escaped = false;
    let mut stars = 0_u32;
    let mut unders = 0_u32;
    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '`' {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        match c {
            '*' => stars += 1,
            '_' => unders += 1,
            '[' | ']' | '(' | ')' | '~' | '>' | '#' | '+' | '-' | '=' | '|' | '{' | '}' | '.'
            | '!' => panic!("unescaped {c:?} outside a code span in {s:?}"),
            _ => {}
        }
    }
    assert!(!escaped, "dangling backslash in {s:?}");
    assert!(!in_code, "unterminated code span in {s:?}");
    assert_eq!(stars % 2, 0, "unbalanced * in {s:?}");
    assert_eq!(unders % 2, 0, "unbalanced _ in {s:?}");
}

fn process(pid: u32, name: &str) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        name: name.to_string(),
        state: ProcessState::Running,
        cpu_usage: 12.5,
        memory_bytes: 1_536,
        disk_usage: 0,
        cwd: None,
        cmdline: vec![],
        open_files: vec![],
        io_stats: None,
    }
}

fn extended_process() -> ProcessSnapshot {
    let fd = |n| FileDescriptorInfo {
        fd: n,
        target: "x".to_string(),
        fd_type: FDType::File,
    };
    ProcessSnapshot {
        cwd: Some("/srv/my_app".to_string()),
        cmdline: vec![
            "python3".to_string(),
            "app.py".to_string(),
            "--port=80".to_string(),
        ],
        open_files: vec![fd(0), fd(1)],
        io_stats: Some(IOStats {
            read_bytes: 4_096,
            write_bytes: 1_024,
            read_chars: 1,
            write_chars: 2,
        }),
        ..process(5, "nginx")
    }
}

// The line the process display always adds on Linux, for a process with no open files.
const NO_FDS: &str = if cfg!(target_os = "linux") {
    "\n   ├ FDs: `0`"
} else {
    ""
};

fn tree(root: Option<ProcessSnapshot>, children: Vec<ProcessSnapshot>) -> ProcessTree {
    ProcessTree {
        root_pid: 1,
        root,
        child_count: children.len(),
        children,
        work_done: false,
        timestamp: TIMESTAMP.to_string(),
    }
}

fn snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp: TIMESTAMP.to_string(),
        cpu: CpuSnapshot {
            usage_percent: 12.5,
            cores: vec![],
            frequency_mhz: 3_600,
            brand: "Intel(R) i7".to_string(),
            physical_core_count: Some(4),
            load_avg: None,
        },
        memory: MemorySnapshot {
            total_bytes: 8 * GIB,
            used_bytes: 4 * GIB,
            available_bytes: 4 * GIB,
            free_bytes: 4 * GIB,
            used_percent: 50.0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            swap_used_percent: None,
        },
        disks: vec![],
        networks: vec![],
        gpus: vec![],
        battery: None,
        temperatures: vec![],
        host: HostInfo {
            hostname: Some("box-1".to_string()),
            os_name: Some("Ubuntu 24.04".to_string()),
            kernel_version: Some("6.8.0".to_string()),
            cpu_arch: Some("x86_64".to_string()),
            uptime_secs: 3_661,
            process_count: 200,
        },
        health: SystemHealth::Healthy,
    }
}

fn disk(mount_point: &str, file_system: &str, kind: DiskKind) -> DiskSnapshot {
    DiskSnapshot {
        name: "sda".to_string(),
        mount_point: mount_point.to_string(),
        file_system: file_system.to_string(),
        kind,
        is_removable: false,
        total_bytes: 2 * GIB,
        used_bytes: GIB,
        available_bytes: GIB,
        used_percent: 45.5,
    }
}

fn network(interface: &str) -> NetworkSnapshot {
    NetworkSnapshot {
        interface: interface.to_string(),
        rx_bytes_per_sec: KIB,
        tx_bytes_per_sec: 512,
        rx_total_bytes: 0,
        tx_total_bytes: 0,
        rx_packets_per_sec: 0,
        tx_packets_per_sec: 0,
        rx_errors: 0,
        tx_errors: 0,
    }
}

fn bare_gpu(name: &str) -> GpuSnapshot {
    GpuSnapshot {
        name: name.to_string(),
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

fn full_gpu(name: &str) -> GpuSnapshot {
    GpuSnapshot {
        usage_percent: Some(75.0),
        vram_used_bytes: Some(GIB),
        vram_total_bytes: Some(2 * GIB),
        vram_used_percent: Some(50.0),
        temperature_celsius: Some(65.0),
        power_draw_watts: Some(200.5),
        power_limit_watts: Some(450.0),
        fan_speed_percent: vec![30.0, 40.0],
        ..bare_gpu(name)
    }
}

const fn bare_battery(state: BatteryState) -> BatterySnapshot {
    BatterySnapshot {
        charge_percent: 80.5,
        state,
        time_to_empty_secs: None,
        time_to_full_secs: None,
        power_draw_watts: None,
        cycle_count: None,
        health_percent: None,
    }
}

fn thermal(label: &str, temp: Option<f32>, critical: Option<f32>) -> ThermalSnapshot {
    ThermalSnapshot {
        label: label.to_string(),
        temperature_celsius: temp,
        temperature_max_celsius: None,
        temperature_critical_celsius: critical,
    }
}

const fn alarm_off() -> AlarmStatus {
    AlarmStatus {
        active: false,
        since: None,
    }
}

const fn alarm_on(since: Option<SystemTime>) -> AlarmStatus {
    AlarmStatus {
        active: true,
        since,
    }
}

fn alarms() -> AlarmSnapshot {
    AlarmSnapshot {
        cpu: alarm_off(),
        memory: alarm_off(),
        disks: vec![],
        battery_low: alarm_off(),
    }
}

fn unit(name: &str, active_state: UnitActiveState) -> UnitSnapshot {
    UnitSnapshot {
        unit_name: name.to_string(),
        unit_type: UnitType::Service,
        load_state: UnitLoadState::Loaded,
        active_state,
        sub_state: "running".to_string(),
        description: String::new(),
        main_pid: None,
        memory_bytes: None,
        cpu_usage_ns: None,
        restart_count: None,
        since: None,
        fragment_path: None,
    }
}

fn systemd_snapshot(failed_count: u32, units: Vec<UnitSnapshot>) -> SystemdSnapshot {
    SystemdSnapshot {
        timestamp: "2024-05-01".to_string(),
        units,
        failed_count,
        active_count: 2,
        inactive_count: 1,
    }
}

fn container() -> ContainerSnapshot {
    ContainerSnapshot {
        id: "abc123def456789".to_string(),
        short_id: "abc123def456".to_string(),
        name: "web_1".to_string(),
        image: "nginx:1.25".to_string(),
        status: ContainerStatus::Running,
        health: ContainerHealth::None,
        stats: None,
    }
}

const fn stats() -> ContainerStats {
    ContainerStats {
        cpu_percent: 12.5,
        memory_bytes: 536_870_912,
        memory_limit_bytes: GIB,
        memory_percent: Some(0.5),
        net_rx_bytes: KIB,
        net_tx_bytes: 2 * KIB,
        block_read_bytes: 3 * KIB,
        block_write_bytes: 4 * KIB,
        pid_count: 7,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ProcessSnapshot
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_exact_output() {
    let msg = TelegramDisplay(&process(5, "nginx")).to_string();
    let expected = format!(
        "🔹 *nginx* `\\(PID 5\\)`\n   ├ State: `running`\n   ├ CPU: `12.5%`\n   └ Mem: `1\\.5 KB`\n   └ Disk: `0 B`{NO_FDS}"
    );
    assert_eq!(msg, expected);
    assert_valid_mdv2(&msg);
}

#[test]
fn process_prints_two_closing_connectors() {
    // Pinned (finding 45, cosmetic): Mem and Disk both start with the closing connector.
    let msg = TelegramDisplay(&process(5, "nginx")).to_string();
    assert!(msg.contains("   └ Mem: "));
    assert!(msg.contains("   └ Disk: "));
}

#[test]
fn process_escapes_the_name() {
    let msg = TelegramDisplay(&process(5, "my_app.bin")).to_string();
    assert!(msg.starts_with("🔹 *my\\_app\\.bin* `\\(PID 5\\)`\n"));
    assert_valid_mdv2(&msg);
}

#[test]
fn process_shows_every_state() {
    for (state, text) in [
        (ProcessState::Running, "running"),
        (ProcessState::Sleeping, "sleeping"),
        (ProcessState::Gone, "gone"),
        (
            ProcessState::Other("Zombie".to_string()),
            "other\\(Zombie\\)",
        ),
    ] {
        let snap = ProcessSnapshot {
            state,
            ..process(5, "a")
        };
        let msg = TelegramDisplay(&snap).to_string();
        assert!(msg.contains(&format!("├ State: `{text}`\n")), "{msg}");
        assert_valid_mdv2(&msg);
    }
}

#[test]
fn process_rounds_cpu_to_one_decimal_and_formats_bytes() {
    let snap = ProcessSnapshot {
        cpu_usage: 99.96,
        memory_bytes: 1_048_576,
        disk_usage: 2 * GIB,
        ..process(5, "a")
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.contains("├ CPU: `100.0%`\n"), "{msg}");
    assert!(msg.contains("└ Mem: `1\\.0 MB`\n"), "{msg}");
    assert!(msg.contains("└ Disk: `2\\.0 GB`"), "{msg}");
}

#[test]
fn process_extended_fields_only_show_on_linux() {
    let msg = TelegramDisplay(&extended_process()).to_string();
    let base = "🔹 *nginx* `\\(PID 5\\)`\n   ├ State: `running`\n   ├ CPU: `12.5%`\n   └ Mem: `1\\.5 KB`\n   └ Disk: `0 B`";
    let expected = if cfg!(target_os = "linux") {
        format!(
            "{base}\n   ├ CWD: `/srv/my\\_app`\n   ├ FDs: `2`\n   ├ I/O Read: `4096` / Write: `1024`\n   └ CMD: `python3 app\\.py \\-\\-port\\=80`"
        )
    } else {
        base.to_string()
    };
    assert_eq!(msg, expected);
    assert_valid_mdv2(&msg);
}

#[test]
fn process_with_hostile_text_is_valid_markdown() {
    let snap = ProcessSnapshot {
        state: ProcessState::Other(HOSTILE.to_string()),
        cwd: Some(HOSTILE.to_string()),
        cmdline: vec![HOSTILE.to_string(), HOSTILE.to_string()],
        ..process(5, HOSTILE)
    };
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// ProcessTree
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tree_with_no_root_and_no_children() {
    let msg = TelegramDisplay(&tree(None, vec![])).to_string();
    assert_eq!(
        msg,
        format!(
            "⏳ *Process Tree*\n🕐 `{TIMESTAMP}`\n\n*Root Process*\n_No root process_\n\n*Children:* _none_\n"
        )
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn tree_shows_a_check_mark_when_work_is_done() {
    let t = ProcessTree {
        work_done: true,
        ..tree(None, vec![])
    };
    assert!(
        TelegramDisplay(&t)
            .to_string()
            .starts_with("✅ *Process Tree*\n")
    );
}

#[test]
fn tree_with_root_and_children_exact_output() {
    let root = process(1, "root");
    let first = process(2, "first");
    let second = process(3, "second");
    let msg = TelegramDisplay(&tree(
        Some(root.clone()),
        vec![first.clone(), second.clone()],
    ))
    .to_string();
    let (root, first, second) = (
        TelegramDisplay(&root).to_string(),
        TelegramDisplay(&first).to_string(),
        TelegramDisplay(&second).to_string(),
    );
    assert_eq!(
        msg,
        format!(
            "⏳ *Process Tree*\n🕐 `{TIMESTAMP}`\n\n*Root Process*\n{root}\n\n*Children* \\(2\\):\n{first}\n\n{second}\n\n"
        )
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn tree_with_root_and_no_children() {
    let root = process(1, "root");
    let msg = TelegramDisplay(&tree(Some(root.clone()), vec![])).to_string();
    let root = TelegramDisplay(&root).to_string();
    assert!(msg.ends_with(&format!("*Root Process*\n{root}\n\n*Children:* _none_\n")));
}

#[test]
fn tree_header_trusts_child_count_over_the_list() {
    // Pinned: the two fields are not cross-checked.
    let t = ProcessTree {
        child_count: 5,
        ..tree(None, vec![process(2, "only")])
    };
    let msg = TelegramDisplay(&t).to_string();
    assert!(msg.contains("*Children* \\(5\\):\n"));
    assert!(msg.contains("*only*"));

    let t = ProcessTree {
        child_count: 0,
        ..tree(None, vec![process(2, "hidden")])
    };
    let msg = TelegramDisplay(&t).to_string();
    assert!(msg.contains("*Children:* _none_"));
    assert!(!msg.contains("hidden"));
}

#[test]
fn tree_prints_the_timestamp_without_escaping() {
    // Pinned (finding 47): safe for `now_rfc3339` values, because `-`, `+`, `:` and `.`
    // need no escape inside a code span, but it is the only unescaped text in this file.
    let t = ProcessTree {
        timestamp: "a.b".to_string(),
        ..tree(None, vec![])
    };
    assert!(TelegramDisplay(&t).to_string().contains("🕐 `a.b`\n"));
}

#[test]
fn tree_with_hostile_process_names_is_valid_markdown() {
    let t = tree(
        Some(process(1, HOSTILE)),
        vec![process(2, HOSTILE), process(3, HOSTILE)],
    );
    assert_valid_mdv2(&TelegramDisplay(&t).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemSnapshot: header, CPU, memory, health
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn system_snapshot_minimal_exact_output() {
    let msg = TelegramDisplay(&snapshot()).to_string();
    let expected = format!(
        "🖥️ *box\\-1* — `{TIMESTAMP_ESCAPED}`\n\
         ├ OS: `Ubuntu 24\\.04`\n\
         ├ Kernel: `6\\.8\\.0`\n\
         ├ Arch: `x86\\_64`\n\
         ├ Uptime: `1h 1m 1s`\n\
         └ Processes: `200`\n\
         \n🔲 *CPU* — `Intel\\(R\\) i7`\n\
         ├ Usage: `12.5%`\n\
         └ Freq: `3600 MHz`\n\
         \n🧠 *Memory*\n\
         ├ RAM: `4\\.0 GB` / `8\\.0 GB` \\(`50.0%`\\)\n\
         └ Swap: `0 B` / `0 B`\n\
         \n✅ *Health*: `Healthy`"
    );
    assert_eq!(msg, expected);
    assert_valid_mdv2(&msg);
}

#[test]
fn system_snapshot_falls_back_when_host_fields_are_missing() {
    let mut snap = snapshot();
    snap.host.hostname = None;
    snap.host.os_name = None;
    snap.host.kernel_version = None;
    snap.host.cpu_arch = None;
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.starts_with("🖥️ *unknown* — "), "{msg}");
    assert!(
        msg.contains("├ OS: `?`\n├ Kernel: `?`\n├ Arch: `?`\n"),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn system_snapshot_load_average_only_shows_on_linux_and_macos() {
    let mut snap = snapshot();
    snap.cpu.load_avg = Some(LoadAverage {
        one: 0.5,
        five: 1.25,
        fifteen: 2.0,
    });
    let msg = TelegramDisplay(&snap).to_string();
    let line = "└ Freq: `3600 MHz`\n   └ Load avg: `0.50` / `1.25` / `2.00`\n";
    assert_eq!(
        msg.contains(line),
        cfg!(any(target_os = "linux", target_os = "macos")),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn system_snapshot_swap_percent_is_added_when_known() {
    let mut snap = snapshot();
    snap.memory.swap_total_bytes = 2 * GIB;
    snap.memory.swap_used_bytes = GIB;
    snap.memory.swap_used_percent = Some(50.0);
    let msg = TelegramDisplay(&snap).to_string();
    assert!(
        msg.contains("└ Swap: `1\\.0 GB` / `2\\.0 GB` \\(`50.0%`\\)\n"),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn system_snapshot_ends_with_the_health_line_for_each_state() {
    for (health, line) in [
        (SystemHealth::Healthy, "\n✅ *Health*: `Healthy`"),
        (SystemHealth::Warning, "\n⚠️ *Health*: `Warning`"),
        (SystemHealth::Critical, "\n🔴 *Health*: `Critical`"),
    ] {
        let snap = SystemSnapshot {
            health,
            ..snapshot()
        };
        let msg = TelegramDisplay(&snap).to_string();
        assert!(msg.ends_with(line), "{msg}");
    }
}

#[test]
fn system_snapshot_prints_a_nan_reading_as_text() {
    // Pinned: a NaN is not hidden or replaced, and stays valid inside the code span.
    let mut snap = snapshot();
    snap.cpu.usage_percent = f32::NAN;
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.contains("├ Usage: `NaN%`\n"), "{msg}");
    assert_valid_mdv2(&msg);
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemSnapshot: disks and networks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn disks_section_is_left_out_when_empty() {
    assert!(!TelegramDisplay(&snapshot()).to_string().contains("*Disks*"));
}

#[test]
fn single_disk_uses_the_closing_connector() {
    let snap = SystemSnapshot {
        disks: vec![disk("/", "ext4", DiskKind::Ssd)],
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(
        msg.contains(
            "\n💾 *Disks*\n└ `/` \\(ext4, Ssd\\): `45.5%` used \\(`1\\.0 GB` / `2\\.0 GB`\\)\n"
        ),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn several_disks_only_close_on_the_last() {
    let snap = SystemSnapshot {
        disks: vec![
            disk("/", "ext4", DiskKind::Ssd),
            disk("/mnt/data_1", "xfs", DiskKind::Hdd),
            disk("C:\\", "fuse.sshfs", DiskKind::Unknown),
        ],
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.contains("├ `/` \\(ext4, Ssd\\): "), "{msg}");
    assert!(msg.contains("├ `/mnt/data\\_1` \\(xfs, Hdd\\): "), "{msg}");
    assert!(
        msg.contains("└ `C:\\\\` \\(fuse\\.sshfs, Unknown\\): "),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn disk_kind_uses_debug_text_not_display() {
    // Pinned: `Ssd`, while `DiskKind`'s Display says `SSD`.
    let snap = SystemSnapshot {
        disks: vec![disk("/", "ext4", DiskKind::Ssd)],
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.contains(", Ssd\\)"));
    assert!(!msg.contains("SSD"));
}

#[test]
fn networks_section_is_left_out_when_empty() {
    assert!(
        !TelegramDisplay(&snapshot())
            .to_string()
            .contains("*Network*")
    );
}

#[test]
fn networks_show_rates_and_connectors() {
    let snap = SystemSnapshot {
        networks: vec![network("eth0"), network("wi-fi_0")],
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(
        msg.contains(
            "\n🌐 *Network*\n├ `eth0`: ↓ `1\\.0 KB/s` ↑ `512 B/s`\n└ `wi\\-fi\\_0`: ↓ `1\\.0 KB/s` ↑ `512 B/s`\n"
        ),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemSnapshot: GPUs
// ─────────────────────────────────────────────────────────────────────────────

fn with_gpus(gpus: Vec<GpuSnapshot>) -> String {
    let snap = SystemSnapshot { gpus, ..snapshot() };
    let msg = TelegramDisplay(&snap).to_string();
    assert_valid_mdv2(&msg);
    msg
}

#[test]
fn gpu_section_is_left_out_when_empty() {
    assert!(!with_gpus(vec![]).contains("*GPU*"));
}

#[test]
fn gpu_with_every_field() {
    let msg = with_gpus(vec![full_gpu("RTX 4090")]);
    assert!(
        msg.contains(
            "\n🎮 *GPU*\n└ *RTX 4090*\n   ├ Core: `75.0%`\n   ├ VRAM: `1\\.0 GB` / `2\\.0 GB` \\(`50.0%`\\)\n   ├ Temp: `65.0°C`\n   ├ Power: `200.5W` / `450.0W`\n   └ Fans: `30%`, `40%`\n"
        ),
        "{msg}"
    );
}

#[test]
fn gpu_with_only_a_name() {
    let msg = with_gpus(vec![bare_gpu("Intel(R) UHD")]);
    assert!(
        msg.contains("\n🎮 *GPU*\n└ *Intel\\(R\\) UHD*\n\n✅ *Health*"),
        "{msg}"
    );
}

#[test]
fn several_gpus_only_close_on_the_last() {
    let msg = with_gpus(vec![bare_gpu("a"), bare_gpu("b")]);
    assert!(msg.contains("├ *a*\n└ *b*\n"), "{msg}");
}

#[test]
fn gpu_vram_needs_both_used_and_total() {
    let used_only = GpuSnapshot {
        vram_used_bytes: Some(GIB),
        ..bare_gpu("a")
    };
    let total_only = GpuSnapshot {
        vram_total_bytes: Some(GIB),
        ..bare_gpu("a")
    };
    assert!(!with_gpus(vec![used_only]).contains("VRAM"));
    assert!(!with_gpus(vec![total_only]).contains("VRAM"));
}

#[test]
fn gpu_vram_without_a_percent_has_no_brackets() {
    let gpu = GpuSnapshot {
        vram_used_bytes: Some(GIB),
        vram_total_bytes: Some(2 * GIB),
        ..bare_gpu("a")
    };
    assert!(with_gpus(vec![gpu]).contains("   ├ VRAM: `1\\.0 GB` / `2\\.0 GB`\n"));
}

#[test]
fn gpu_power_shows_the_limit_only_with_a_draw() {
    let draw_only = GpuSnapshot {
        power_draw_watts: Some(200.5),
        ..bare_gpu("a")
    };
    let limit_only = GpuSnapshot {
        power_limit_watts: Some(450.0),
        ..bare_gpu("a")
    };
    assert!(with_gpus(vec![draw_only]).contains("   ├ Power: `200.5W`\n"));
    assert!(!with_gpus(vec![limit_only]).contains("Power"));
}

#[test]
fn gpu_fan_label_follows_the_count() {
    let one = GpuSnapshot {
        fan_speed_percent: vec![30.0],
        ..bare_gpu("a")
    };
    assert!(with_gpus(vec![one]).contains("   └ Fan: `30%`\n"));
}

#[test]
fn gpu_without_fans_never_closes_its_list() {
    // Pinned (cosmetic): only the fan line uses the closing connector.
    let gpu = GpuSnapshot {
        temperature_celsius: Some(65.0),
        ..bare_gpu("a")
    };
    assert!(with_gpus(vec![gpu]).contains("└ *a*\n   ├ Temp: `65.0°C`\n\n"));
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemSnapshot: battery and thermals
// ─────────────────────────────────────────────────────────────────────────────

fn with_battery(battery: BatterySnapshot) -> String {
    let snap = SystemSnapshot {
        battery: Some(battery),
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert_valid_mdv2(&msg);
    msg
}

#[test]
fn battery_section_is_left_out_when_absent() {
    assert!(
        !TelegramDisplay(&snapshot())
            .to_string()
            .contains("*Battery*")
    );
}

#[test]
fn battery_with_only_a_charge_and_state() {
    for (state, text) in [
        (BatteryState::Charging, "Charging"),
        (BatteryState::Discharging, "Discharging"),
        (BatteryState::Full, "Full"),
        (BatteryState::Unknown, "Unknown"),
    ] {
        let msg = with_battery(bare_battery(state));
        assert!(
            msg.contains(&format!(
                "\n🔋 *Battery* — `80.5%` \\({text}\\)\n\n✅ *Health*"
            )),
            "{msg}"
        );
    }
}

#[test]
fn battery_with_every_field() {
    let msg = with_battery(BatterySnapshot {
        time_to_empty_secs: Some(3_600),
        time_to_full_secs: Some(1_800),
        power_draw_watts: Some(15.5),
        health_percent: Some(95.0),
        cycle_count: Some(120),
        ..bare_battery(BatteryState::Discharging)
    });
    assert!(
        msg.contains(
            "\n🔋 *Battery* — `80.5%` \\(Discharging\\)\n   ├ Time to empty: `1h`\n   ├ Time to full: `30m`\n   ├ Draw: `15.5W`\n   ├ Health: `95.0%`\n   └ Cycles: `120`\n"
        ),
        "{msg}"
    );
}

#[test]
fn thermals_section_is_left_out_when_empty() {
    assert!(
        !TelegramDisplay(&snapshot())
            .to_string()
            .contains("*Thermals*")
    );
}

#[test]
fn thermals_show_reading_critical_and_connectors() {
    let snap = SystemSnapshot {
        temperatures: vec![
            thermal("CPU_Pkg", Some(65.0), Some(100.0)),
            thermal("gpu", Some(40.5), None),
            thermal("acpi", None, None),
        ],
        ..snapshot()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(
        msg.contains(
            "\n🌡️ *Thermals*\n├ `CPU\\_Pkg`: `65.0°C` \\(crit `100.0°C`\\)\n├ `gpu`: `40.5°C`\n└ `acpi`: `n/a`\n"
        ),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemSnapshot: all together
// ─────────────────────────────────────────────────────────────────────────────

fn full_snapshot(text: &str) -> SystemSnapshot {
    SystemSnapshot {
        disks: vec![disk(text, text, DiskKind::Ssd)],
        networks: vec![network(text)],
        gpus: vec![full_gpu(text)],
        battery: Some(bare_battery(BatteryState::Full)),
        temperatures: vec![thermal(text, Some(65.0), Some(100.0))],
        ..snapshot()
    }
}

#[test]
fn system_snapshot_sections_come_in_a_fixed_order() {
    let msg = TelegramDisplay(&full_snapshot("x")).to_string();
    let positions = [
        "*CPU*",
        "*Memory*",
        "*Disks*",
        "*Network*",
        "*GPU*",
        "*Battery*",
        "*Thermals*",
        "*Health*",
    ]
    .map(|heading| msg.find(heading).unwrap());
    let mut sorted = positions;
    sorted.sort_unstable();
    assert_eq!(positions, sorted);
}

#[test]
fn system_snapshot_with_hostile_text_is_valid_markdown() {
    let mut snap = full_snapshot(HOSTILE);
    snap.timestamp = HOSTILE.to_string();
    snap.cpu.brand = HOSTILE.to_string();
    snap.host.hostname = Some(HOSTILE.to_string());
    snap.host.os_name = Some(HOSTILE.to_string());
    snap.host.kernel_version = Some(HOSTILE.to_string());
    snap.host.cpu_arch = Some(HOSTILE.to_string());
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// AlarmSnapshot
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn alarms_all_clear_exact_output() {
    let msg = TelegramDisplay(&alarms()).to_string();
    assert_eq!(
        msg,
        "✅ *Alarms* — `0` active\n├ ⚫ `CPU`\n├ ⚫ `Memory`\n└ ⚫ `Battery Low`\n"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn alarms_list_disks_last_and_escape_mounts() {
    let snap = AlarmSnapshot {
        disks: vec![
            ("/".to_string(), alarm_off()),
            ("C:\\".to_string(), alarm_off()),
            ("/mnt/x_y".to_string(), alarm_off()),
        ],
        ..alarms()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert_eq!(
        msg,
        "✅ *Alarms* — `0` active\n├ ⚫ `CPU`\n├ ⚫ `Memory`\n├ ⚫ `Battery Low`\n├ ⚫ `/`\n├ ⚫ `C:\\\\`\n└ ⚫ `/mnt/x\\_y`\n"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn alarms_count_every_active_alarm_including_disks() {
    let snap = AlarmSnapshot {
        cpu: alarm_on(None),
        memory: alarm_off(),
        disks: vec![("/".to_string(), alarm_on(None))],
        battery_low: alarm_on(None),
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert_eq!(
        msg,
        "🚨 *Alarms* — `3` active\n├ 🔴 `CPU`\n├ ⚫ `Memory`\n├ 🔴 `Battery Low`\n└ 🔴 `/`\n"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn active_alarm_shows_how_long_it_has_been_active() {
    let since = SystemTime::now() - Duration::from_secs(7_200);
    let snap = AlarmSnapshot {
        cpu: alarm_on(Some(since)),
        ..alarms()
    };
    let msg = TelegramDisplay(&snap).to_string();
    // Only the hours are asserted, since the clock moves while the test runs.
    assert!(msg.contains("├ 🔴 `CPU` — active `2h"), "{msg}");
    assert_valid_mdv2(&msg);
}

#[test]
fn active_alarm_with_a_future_start_has_no_duration() {
    let since = SystemTime::now() + Duration::from_secs(3_600);
    let snap = AlarmSnapshot {
        cpu: alarm_on(Some(since)),
        ..alarms()
    };
    assert!(TelegramDisplay(&snap).to_string().contains("├ 🔴 `CPU`\n"));
}

#[test]
fn inactive_alarm_hides_its_last_start_time() {
    let snap = AlarmSnapshot {
        cpu: AlarmStatus {
            active: false,
            since: Some(SystemTime::now() - Duration::from_secs(7_200)),
        },
        ..alarms()
    };
    assert!(TelegramDisplay(&snap).to_string().contains("├ ⚫ `CPU`\n"));
}

#[test]
fn alarms_with_a_hostile_mount_are_valid_markdown() {
    let snap = AlarmSnapshot {
        disks: vec![(HOSTILE.to_string(), alarm_on(Some(SystemTime::now())))],
        ..alarms()
    };
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// UnitSnapshot
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_minimal_exact_output() {
    let msg = TelegramDisplay(&unit("nginx.service", UnitActiveState::Active)).to_string();
    assert_eq!(msg, "🟢 *nginx\\.service*  `running`\n");
    assert_valid_mdv2(&msg);
}

#[test]
fn unit_with_every_field_exact_output() {
    let snap = UnitSnapshot {
        description: "A web server".to_string(),
        main_pid: Some(42),
        memory_bytes: Some(1_536),
        cpu_usage_ns: Some(1_500_000_000),
        restart_count: Some(3),
        since: Some("2024-05-01".to_string()),
        fragment_path: Some("/usr/lib/nginx.service".to_string()),
        ..unit("nginx.service", UnitActiveState::Active)
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert_eq!(
        msg,
        "🟢 *nginx\\.service*  `running`\n   ├ `A web server`\n   ├ PID: `42`\n   ├ Mem: `1\\.5 KB`\n   ├ CPU time: `1.50s`\n   ├ Restarts: `3`\n   ├ Since: `2024\\-05\\-01`\n   └ File: `/usr/lib/nginx\\.service`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn unit_only_ends_without_a_newline_when_a_file_is_shown() {
    // Pinned: every other line uses `writeln!`, so the two forms end differently.
    let plain = unit("a.service", UnitActiveState::Active);
    let with_file = UnitSnapshot {
        fragment_path: Some("/a".to_string()),
        ..unit("a.service", UnitActiveState::Active)
    };
    assert!(TelegramDisplay(&plain).to_string().ends_with('\n'));
    assert!(!TelegramDisplay(&with_file).to_string().ends_with('\n'));
}

#[test]
fn unit_description_is_hidden_when_it_repeats_the_name() {
    let same = UnitSnapshot {
        description: "a.service".to_string(),
        ..unit("a.service", UnitActiveState::Active)
    };
    assert_eq!(
        TelegramDisplay(&same).to_string(),
        "🟢 *a\\.service*  `running`\n"
    );
}

#[test]
fn unit_emoji_follows_the_active_state() {
    for (state, emoji) in [
        (UnitActiveState::Active, "🟢"),
        (UnitActiveState::Reloading, "🔄"),
        (UnitActiveState::Inactive, "⚫"),
        (UnitActiveState::Failed, "🔴"),
        (UnitActiveState::Activating, "🟡"),
        (UnitActiveState::Deactivating, "🟠"),
    ] {
        let msg = TelegramDisplay(&unit("a", state)).to_string();
        assert!(msg.starts_with(&format!("{emoji} *a*")), "{msg}");
    }
}

#[test]
fn unit_shows_zero_values() {
    let snap = UnitSnapshot {
        main_pid: Some(0),
        memory_bytes: Some(0),
        cpu_usage_ns: Some(0),
        restart_count: Some(0),
        ..unit("a", UnitActiveState::Active)
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.contains("├ PID: `0`\n"));
    assert!(msg.contains("├ Mem: `0 B`\n"));
    assert!(msg.contains("├ CPU time: `0.00s`\n"));
    assert!(msg.contains("├ Restarts: `0`\n"));
}

#[test]
fn unit_with_hostile_text_is_valid_markdown() {
    let snap = UnitSnapshot {
        sub_state: HOSTILE.to_string(),
        description: HOSTILE.to_string(),
        since: Some(HOSTILE.to_string()),
        fragment_path: Some(HOSTILE.to_string()),
        ..unit(HOSTILE, UnitActiveState::Failed)
    };
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemdSnapshot
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn systemd_snapshot_without_failures_exact_output() {
    let msg = TelegramDisplay(&systemd_snapshot(0, vec![])).to_string();
    assert_eq!(
        msg,
        "🔧 *Systemd* — `2024\\-05\\-01`\n├ 🟢 Active: `2`\n├ ⚫ Inactive: `1`\n└ 🔴 Failed: `0`\n"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn systemd_snapshot_lists_only_the_failed_units() {
    let failed = UnitSnapshot {
        sub_state: "exit-code".to_string(),
        ..unit("a.service", UnitActiveState::Failed)
    };
    let units = vec![
        failed,
        unit("ok.service", UnitActiveState::Active),
        unit("b.service", UnitActiveState::Failed),
    ];
    let msg = TelegramDisplay(&systemd_snapshot(2, units)).to_string();
    assert!(
        msg.ends_with(
            "└ 🔴 Failed: `2`\n\n🔴 *Failed Units:*\n• `a\\.service` — exit\\-code\n• `b\\.service` — running\n"
        ),
        "{msg}"
    );
    assert!(!msg.contains("ok"));
    assert_valid_mdv2(&msg);
}

#[test]
fn systemd_snapshot_with_a_count_but_no_failed_units_has_no_list() {
    let units = vec![unit("ok.service", UnitActiveState::Active)];
    let msg = TelegramDisplay(&systemd_snapshot(1, units)).to_string();
    assert!(msg.contains("└ 🔴 Failed: `1`\n"));
    assert!(!msg.contains("*Failed Units:*"));
}

#[test]
fn systemd_snapshot_trusts_the_count_over_the_units() {
    // Pinned: a failed unit is not listed if `failed_count` is zero.
    let units = vec![unit("a.service", UnitActiveState::Failed)];
    let msg = TelegramDisplay(&systemd_snapshot(0, units)).to_string();
    assert!(!msg.contains("*Failed Units:*"));
    assert!(!msg.contains("a\\.service"));
}

#[test]
fn systemd_snapshot_with_hostile_text_is_valid_markdown() {
    let failed = UnitSnapshot {
        sub_state: HOSTILE.to_string(),
        ..unit(HOSTILE, UnitActiveState::Failed)
    };
    let snap = SystemdSnapshot {
        timestamp: HOSTILE.to_string(),
        ..systemd_snapshot(1, vec![failed])
    };
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// ContainerSnapshot
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn container_without_stats_or_health_exact_output() {
    let msg = TelegramDisplay(&container()).to_string();
    assert_eq!(
        msg,
        "🟢 *web\\_1* `abc123def456`\n   ├ Image: `nginx:1\\.25`\n   ├ Status: `running`\n   └ Stats: `unavailable`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn container_with_stats_exact_output() {
    let snap = ContainerSnapshot {
        stats: Some(stats()),
        ..container()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert_eq!(
        msg,
        "🟢 *web\\_1* `abc123def456`\n   ├ Image: `nginx:1\\.25`\n   ├ Status: `running`\n   ├ CPU: `12.5%`\n   ├ Mem: `512\\.0 MB` / `1\\.0 GB` \\(`50.0%`\\)\n   ├ Net: ↓ `1\\.0 KB` ↑ `2\\.0 KB`\n   ├ Block: R `3\\.0 KB` W `4\\.0 KB`\n   └ PIDs: `7`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn container_memory_without_a_limit_shows_only_the_usage() {
    let snap = ContainerSnapshot {
        stats: Some(ContainerStats {
            memory_limit_bytes: 0,
            memory_percent: None,
            ..stats()
        }),
        ..container()
    };
    assert!(
        TelegramDisplay(&snap)
            .to_string()
            .contains("   ├ Mem: `512\\.0 MB`\n")
    );
}

#[test]
fn container_memory_fraction_is_shown_as_a_percent() {
    let snap = ContainerSnapshot {
        stats: Some(ContainerStats {
            memory_percent: Some(0.25),
            ..stats()
        }),
        ..container()
    };
    assert!(TelegramDisplay(&snap).to_string().contains("\\(`25.0%`\\)"));
}

#[test]
fn container_health_line_is_hidden_only_for_none() {
    let snap = container();
    assert!(!TelegramDisplay(&snap).to_string().contains("Health"));

    for (health, line) in [
        (ContainerHealth::Starting, "   ├ Health: 🤍 `starting`\n"),
        (ContainerHealth::Healthy, "   ├ Health: 💚 `healthy`\n"),
        (ContainerHealth::Unhealthy, "   ├ Health: ❤️ `unhealthy`\n"),
        (ContainerHealth::Unknown, "   ├ Health: 🩶 `unknown`\n"),
    ] {
        let snap = ContainerSnapshot {
            health,
            ..container()
        };
        let msg = TelegramDisplay(&snap).to_string();
        assert!(msg.contains(line), "{msg}");
        assert_valid_mdv2(&msg);
    }
}

#[test]
fn container_status_text_and_emoji_for_each_state() {
    for (status, emoji, text) in [
        (ContainerStatus::Created, "🔵", "created"),
        (ContainerStatus::Running, "🟢", "running"),
        (ContainerStatus::Paused, "🟡", "paused"),
        (ContainerStatus::Restarting, "🔄", "restarting"),
        (ContainerStatus::Removing, "🟠", "removing"),
        (ContainerStatus::Exited, "⚫", "exited"),
        (ContainerStatus::Dead, "🔴", "dead"),
        (ContainerStatus::Stopping, "🟠", "stopping"),
    ] {
        let snap = ContainerSnapshot {
            status,
            ..container()
        };
        let msg = TelegramDisplay(&snap).to_string();
        assert!(msg.starts_with(&format!("{emoji} *web")), "{msg}");
        assert!(msg.contains(&format!("├ Status: `{text}`\n")), "{msg}");
    }
}

#[test]
fn container_unknown_status_uses_lowercased_debug_text() {
    // Pinned: quoted and lowercased here, while `ContainerStatus`'s Display (used by the
    // status-changed event) gives `unknown(Weird)`.
    let snap = ContainerSnapshot {
        status: ContainerStatus::Unknown("Weird".to_string()),
        ..container()
    };
    let msg = TelegramDisplay(&snap).to_string();
    assert!(msg.starts_with("❓ *web"), "{msg}");
    assert!(
        msg.contains("├ Status: `unknown\\(\"weird\"\\)`\n"),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn container_with_hostile_text_is_valid_markdown() {
    let snap = ContainerSnapshot {
        short_id: HOSTILE.to_string(),
        name: HOSTILE.to_string(),
        image: HOSTILE.to_string(),
        status: ContainerStatus::Unknown(HOSTILE.to_string()),
        health: ContainerHealth::Unhealthy,
        stats: Some(stats()),
        ..container()
    };
    assert_valid_mdv2(&TelegramDisplay(&snap).to_string());
}
